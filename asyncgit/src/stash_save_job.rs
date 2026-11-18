//!

use crate::{
	asyncjob::{AsyncJob, RunParams},
	error::Result,
	sync::{stash_save, CommitId, RepoPath},
	AsyncGitNotification,
};

use std::sync::{Arc, Mutex};

enum JobState {
	Request {
		message: Option<String>,
		include_untracked: bool,
		keep_index: bool,
	},
	Response(Result<CommitId>),
}

/// Runs a (potentially slow) `stash_save` on a background thread so the UI
/// stays responsive and can show that stashing is in progress.
#[derive(Clone)]
pub struct AsyncStashSaveJob {
	state: Arc<Mutex<Option<JobState>>>,
	repo: RepoPath,
}

///
impl AsyncStashSaveJob {
	///
	pub fn new(
		repo: RepoPath,
		message: Option<String>,
		include_untracked: bool,
		keep_index: bool,
	) -> Self {
		Self {
			repo,
			state: Arc::new(Mutex::new(Some(JobState::Request {
				message,
				include_untracked,
				keep_index,
			}))),
		}
	}

	/// Returns the result of the stash operation once the job has finished.
	pub fn result(&self) -> Option<Result<CommitId>> {
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
}

impl AsyncJob for AsyncStashSaveJob {
	type Notification = AsyncGitNotification;
	type Progress = ();

	fn run(
		&mut self,
		_params: RunParams<Self::Notification, Self::Progress>,
	) -> Result<Self::Notification> {
		if let Ok(mut state) = self.state.lock() {
			*state = state.take().map(|state| match state {
				JobState::Request {
					message,
					include_untracked,
					keep_index,
				} => {
					let result = stash_save(
						&self.repo,
						message.as_deref(),
						include_untracked,
						keep_index,
					);

					JobState::Response(result)
				}
				JobState::Response(result) => {
					JobState::Response(result)
				}
			});
		}

		Ok(AsyncGitNotification::StashSave)
	}
}
