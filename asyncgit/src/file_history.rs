use git2::Repository;

use crate::{
	asyncjob::{AsyncJob, RunParams},
	error::Result,
	sync::{
		self,
		commit_files::{
			commit_contains_file, commit_detect_file_rename,
		},
		CommitId, CommitInfo, LogWalker, RepoPath,
		SharedCommitFilterFn,
	},
	AsyncGitNotification,
};
use std::{
	sync::{Arc, Mutex, RwLock},
	time::{Duration, Instant},
};

///
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FileHistoryEntryDelta {
	///
	None,
	///
	Added,
	///
	Deleted,
	///
	Modified,
	///
	Renamed,
	///
	Copied,
	///
	Typechange,
}

impl From<git2::Delta> for FileHistoryEntryDelta {
	fn from(value: git2::Delta) -> Self {
		match value {
			git2::Delta::Unmodified
			| git2::Delta::Ignored
			| git2::Delta::Unreadable
			| git2::Delta::Conflicted
			| git2::Delta::Untracked => FileHistoryEntryDelta::None,
			git2::Delta::Added => FileHistoryEntryDelta::Added,
			git2::Delta::Deleted => FileHistoryEntryDelta::Deleted,
			git2::Delta::Modified => FileHistoryEntryDelta::Modified,
			git2::Delta::Renamed => FileHistoryEntryDelta::Renamed,
			git2::Delta::Copied => FileHistoryEntryDelta::Copied,
			git2::Delta::Typechange => {
				FileHistoryEntryDelta::Typechange
			}
		}
	}
}

///
#[derive(Debug, Clone, PartialEq)]
pub struct FileHistoryEntry {
	///
	pub commit: CommitId,
	///
	pub delta: FileHistoryEntryDelta,
	//TODO: arc and share since most will be the same over the history
	///
	pub file_path: String,
	///
	pub info: CommitInfo,
}

///
pub struct CommitFilterResult {
	///
	pub result: Vec<FileHistoryEntry>,
	pub duration: Duration,
}

enum JobState {
	Request {
		file_path: String,
		repo_path: RepoPath,
	},
	Response(Result<CommitFilterResult>),
}

#[derive(Clone, Default)]
pub struct AsyncFileHistoryResults(Arc<Mutex<Vec<FileHistoryEntry>>>);

impl PartialEq for AsyncFileHistoryResults {
	fn eq(&self, other: &Self) -> bool {
		if let Ok(left) = self.0.lock() {
			if let Ok(right) = other.0.lock() {
				return *left == *right;
			}
		}

		false
	}
}

impl AsyncFileHistoryResults {
	///
	pub fn extract_results(&self) -> Result<Vec<FileHistoryEntry>> {
		let mut results = self.0.lock()?;
		log::trace!("pull entries {}", results.len());
		let results =
			std::mem::replace(&mut *results, Vec::with_capacity(1));
		Ok(results)
	}
}

///
#[derive(Clone)]
pub struct AsyncFileHistoryJob {
	state: Arc<Mutex<Option<JobState>>>,
	results: AsyncFileHistoryResults,
}

///
impl AsyncFileHistoryJob {
	///
	pub fn new(repo_path: RepoPath, file_path: String) -> Self {
		Self {
			state: Arc::new(Mutex::new(Some(JobState::Request {
				repo_path,
				file_path,
			}))),
			results: AsyncFileHistoryResults::default(),
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

	///
	pub fn extract_results(&self) -> Result<Vec<FileHistoryEntry>> {
		self.results.extract_results()
	}

	fn file_history_filter(
		file_path: Arc<RwLock<String>>,
		results: Arc<Mutex<Vec<FileHistoryEntry>>>,
		params: &RunParams<
			AsyncGitNotification,
			AsyncFileHistoryResults,
		>,
	) -> SharedCommitFilterFn {
		let params = params.clone();

		Arc::new(Box::new(
			move |repo: &Repository,
			      commit_id: &CommitId|
			      -> Result<bool> {
				let file_path = file_path.clone();

				if fun_name(file_path, results.clone(), repo, commit_id)? {
					params.send(AsyncGitNotification::FileHistory)?;
					Ok(true)
				} else {
					Ok(false)
				}
			},
		))
	}

	fn run_request(
		&self,
		repo_path: &RepoPath,
		file_path: String,
		params: &RunParams<
			AsyncGitNotification,
			AsyncFileHistoryResults,
		>,
	) -> Result<CommitFilterResult> {
		let start = Instant::now();

		let file_name = Arc::new(RwLock::new(file_path));

		let filter = Self::file_history_filter(
			file_name,
			self.results.0.clone(),
			params,
		);

		let repo = sync::repo(repo_path)?;
		let mut walker =
			LogWalker::new(&repo, None)?.filter(Some(filter));

		walker.read(None)?;

//		let result =
//			std::mem::replace(&mut *result.lock()?, Vec::new());

		let result = CommitFilterResult {
			duration: start.elapsed(),
			result: Default::default(), // TODO
	//		result: self.results.0.into_inner()?,
		};

		Ok(result)
	}
}

fn fun_name(
	file_path: Arc<RwLock<String>>,
	results: Arc<Mutex<Vec<FileHistoryEntry>>>,
	repo: &Repository,
	commit_id: &CommitId,
) -> Result<bool> {
	let current_file_path = file_path.read()?.to_string();

	if let Some(delta) = commit_contains_file(
		repo,
		*commit_id,
		current_file_path.as_str(),
	)? {
		log::info!(
			"[history] edit: [{}] ({:?}) - {}",
			commit_id.get_short_string(),
			delta,
			&current_file_path
		);

		let commit_info =
			sync::get_commit_info_repo(repo, commit_id)?;

		let entry = FileHistoryEntry {
			commit: *commit_id,
			delta: delta.clone().into(),
			info: commit_info,
			file_path: current_file_path.clone(),
		};

		//note: only do rename test in case file looks like being added in this commit
		if matches!(delta, git2::Delta::Added) {
			let rename = commit_detect_file_rename(
				repo,
				*commit_id,
				current_file_path.as_str(),
			)?;

			if let Some(old_name) = rename {
				// log::info!(
				// 	"rename: [{}] {:?} <- {:?}",
				// 	commit_id.get_short_string(),
				// 	current_file_path,
				// 	old_name,
				// );

				(*file_path.write()?) = old_name;
			}
		}
		results.lock()?.push(entry);
		log::trace!("push entry {}", results.lock()?.len());

		return Ok(true);
	}

	Ok(false)
}

impl AsyncJob for AsyncFileHistoryJob {
	type Notification = AsyncGitNotification;
	type Progress = AsyncFileHistoryResults;

	fn run(
		&mut self,
		params: RunParams<Self::Notification, Self::Progress>,
	) -> Result<Self::Notification> {
		if let Ok(mut state) = self.state.lock() {
			*state = state.take().map(|state| match state {
				JobState::Request {
					file_path,
					repo_path,
				} => JobState::Response(
					self.run_request(&repo_path, file_path, &params),
				),
				JobState::Response(result) => {
					JobState::Response(result)
				}
			});
		}

		Ok(AsyncGitNotification::FileHistory)
	}
}
