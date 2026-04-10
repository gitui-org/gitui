use rayon::{
	prelude::ParallelIterator,
	slice::{ParallelSlice, ParallelSliceMut},
};

use crate::{
	asyncjob::{AsyncJob, RunParams},
	error::Result,
	sync::{self, CommitId, RepoPath, SharedCommitFilterFn},
	AsyncGitNotification, ProgressPercent,
};
use std::{
	sync::{
		atomic::{AtomicBool, AtomicUsize, Ordering},
		Arc, Mutex,
	},
	time::{Duration, Instant},
};

///
pub struct CommitFilterResult {
	///
	pub result: Vec<CommitId>,
	///
	pub duration: Duration,
}

enum JobState {
	Request {
		commits: Vec<CommitId>,
		repo_path: RepoPath,
	},
	Response(Result<CommitFilterResult>),
}

///
#[derive(Clone)]
pub struct AsyncCommitFilterJob {
	state: Arc<Mutex<Option<JobState>>>,
	filter: SharedCommitFilterFn,
	cancellation_flag: Arc<AtomicBool>,
}

///
impl AsyncCommitFilterJob {
	///
	pub fn new(
		repo_path: RepoPath,
		commits: Vec<CommitId>,
		filter: SharedCommitFilterFn,
		cancellation_flag: Arc<AtomicBool>,
	) -> Self {
		Self {
			state: Arc::new(Mutex::new(Some(JobState::Request {
				repo_path,
				commits,
			}))),
			filter,
			cancellation_flag,
		}
	}

	///
	pub fn result(&self) -> Option<Result<CommitFilterResult>> {
		if let Ok(mut state) = self.state.lock() {
			if let Some(state) = state.take() {
				return match state {
					JobState::Request { .. } => None,
					JobState::Response(result) => Some(result),
				};
			}
		}

		None
	}

	fn run_request(
		&self,
		repo_path: &RepoPath,
		commits: Vec<CommitId>,
		params: &RunParams<AsyncGitNotification, ProgressPercent>,
	) -> JobState {
		let result = self
			.filter_commits(repo_path, commits, params)
			.map(|(start, result)| CommitFilterResult {
				result,
				duration: start.elapsed(),
			});

		JobState::Response(result)
	}

	fn filter_commits(
		&self,
		repo_path: &RepoPath,
		commits: Vec<CommitId>,
		params: &RunParams<AsyncGitNotification, ProgressPercent>,
	) -> Result<(Instant, Vec<CommitId>)> {
		scopetime::scope_time!("filter_commits");

		let total_amount = commits.len();
		let start = Instant::now();

		//note: for some reason >4 threads degrades search performance
		let pool =
			rayon::ThreadPoolBuilder::new().num_threads(4).build()?;

		let idx = AtomicUsize::new(0);

		let mut result = pool.install(|| {
			commits
				.into_iter()
				.enumerate()
				.collect::<Vec<(usize, CommitId)>>()
				.par_chunks(1000)
				.filter_map(|c| {
					//TODO: error log repo open errors
					sync::repo(repo_path).ok().map(|repo| {
						c.iter()
							.filter_map(|(e, c)| {
								let idx = idx.fetch_add(
								1,
								std::sync::atomic::Ordering::Relaxed,
							);

								if self
									.cancellation_flag
									.load(Ordering::Relaxed)
								{
									return None;
								}

								Self::update_progress(
									params,
									ProgressPercent::new(
										idx,
										total_amount,
									),
								);

								(*self.filter)(&repo, c)
									.ok()
									.and_then(|res| {
										res.then_some((*e, *c))
									})
							})
							.collect::<Vec<_>>()
					})
				})
				.flatten()
				.collect::<Vec<_>>()
		});

		result.par_sort_by(|a, b| a.0.cmp(&b.0));

		let result = result.into_iter().map(|c| c.1).collect();

		Ok((start, result))
	}

	fn update_progress(
		params: &RunParams<AsyncGitNotification, ProgressPercent>,
		new_progress: ProgressPercent,
	) {
		match params.set_progress(new_progress) {
			Err(e) => log::error!("progress error: {e}"),
			Ok(result) if result => {
				if let Err(e) =
					params.send(AsyncGitNotification::CommitFilter)
				{
					log::error!("send error: {e}");
				}
			}
			_ => (),
		}
	}
}

impl AsyncJob for AsyncCommitFilterJob {
	type Notification = AsyncGitNotification;
	type Progress = ProgressPercent;

	fn run(
		&mut self,
		params: RunParams<Self::Notification, Self::Progress>,
	) -> Result<Self::Notification> {
		if let Ok(mut state) = self.state.lock() {
			*state = state.take().map(|state| match state {
				JobState::Request { commits, repo_path } => {
					self.run_request(&repo_path, commits, &params)
				}
				JobState::Response(result) => {
					JobState::Response(result)
				}
			});
		}

		Ok(AsyncGitNotification::CommitFilter)
	}
}
