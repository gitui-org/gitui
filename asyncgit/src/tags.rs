use crate::{
	asyncjob::{AsyncJob, AsyncSingleJob, RunParams},
	error::Result,
	hash,
	sync::{self, RepoPath},
	AsyncGitNotification,
};
use crossbeam_channel::Sender;
use std::{
	sync::{Arc, Mutex},
	time::{Duration, Instant},
};
use sync::Tags;

///
#[derive(Default, Clone)]
pub struct TagsResult {
	hash: u64,
	tags: Tags,
}

///
pub struct AsyncTags {
	last: Option<(Instant, TagsResult)>,
	sender: Sender<AsyncGitNotification>,
	job: AsyncSingleJob<AsyncTagsJob>,
	repo: RepoPath,
}

impl AsyncTags {
	///
	pub fn new(
		repo: RepoPath,
		sender: &Sender<AsyncGitNotification>,
	) -> Self {
		Self {
			repo,
			last: None,
			sender: sender.clone(),
			job: AsyncSingleJob::new(sender.clone()),
		}
	}

	/// last fetched result
	pub fn last(&self) -> Result<Option<Tags>> {
		Ok(self.last.as_ref().map(|result| result.1.tags.clone()))
	}

	///
	pub fn is_pending(&self) -> bool {
		self.job.is_pending()
	}

	///
	fn is_outdated(&self, dur: Duration) -> bool {
		self.last
			.as_ref()
			.is_none_or(|(last_time, _)| last_time.elapsed() > dur)
	}

	///
	pub fn request(
		&mut self,
		dur: Duration,
		force: bool,
	) -> Result<()> {
		log::trace!("request");

		if !force && self.job.is_pending() {
			return Ok(());
		}

		let outdated = self.is_outdated(dur);

		if !force && !outdated {
			return Ok(());
		}

		let repo = self.repo.clone();

		if outdated {
			self.job.spawn(AsyncTagsJob::new(
				self.last
					.as_ref()
					.map_or(0, |(_, result)| result.hash),
				repo,
			));

			if let Some(job) = self.job.take_last() {
				if let Some(Ok(result)) = job.result() {
					self.last = Some(result);
				}
			}
		} else {
			self.sender
				.send(AsyncGitNotification::FinishUnchanged)?;
		}

		Ok(())
	}
}

enum JobState {
	Request(u64, RepoPath),
	Response(Result<(Instant, TagsResult)>),
}

///
#[derive(Clone, Default)]
pub struct AsyncTagsJob {
	state: Arc<Mutex<Option<JobState>>>,
}

///
impl AsyncTagsJob {
	///
	pub fn new(last_hash: u64, repo: RepoPath) -> Self {
		Self {
			state: Arc::new(Mutex::new(Some(JobState::Request(
				last_hash, repo,
			)))),
		}
	}

	///
	pub fn result(&self) -> Option<Result<(Instant, TagsResult)>> {
		if let Ok(mut state) = self.state.lock() {
			if let Some(state) = state.take() {
				return match state {
					JobState::Request(_, _) => None,
					JobState::Response(result) => Some(result),
				};
			}
		}

		None
	}
}

impl AsyncJob for AsyncTagsJob {
	type Notification = AsyncGitNotification;
	type Progress = ();

	fn run(
		&mut self,
		_params: RunParams<Self::Notification, Self::Progress>,
	) -> Result<Self::Notification> {
		let mut notification = AsyncGitNotification::FinishUnchanged;
		if let Ok(mut state) = self.state.lock() {
			*state = state.take().map(|state| match state {
				JobState::Request(last_hash, repo) => {
					let tags = sync::get_tags(&repo);

					JobState::Response(tags.map(|tags| {
						let hash = hash(&tags);
						if last_hash != hash {
							notification = AsyncGitNotification::Tags;
						}

						(Instant::now(), TagsResult { hash, tags })
					}))
				}
				JobState::Response(result) => {
					JobState::Response(result)
				}
			});
		}

		Ok(notification)
	}
}
