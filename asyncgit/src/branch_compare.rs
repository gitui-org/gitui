use crate::{
	asyncjob::{AsyncJob, RunParams},
	error::Result,
	sync::{branch::branch_compare_upstream, BranchCompare, RepoPath},
	AsyncGitNotification,
};
use std::sync::{Arc, Mutex};

enum JobState {
	Request {
		repo: RepoPath,
		branch: String,
	},
	Response(Result<BranchCompare>),
}

///
#[derive(Clone, Default)]
pub struct AsyncBranchCompareJob {
	state: Arc<Mutex<Option<JobState>>>,
}

impl AsyncBranchCompareJob {
	///
	pub fn new(repo: RepoPath, branch: String) -> Self {
		Self {
			state: Arc::new(Mutex::new(Some(JobState::Request {
				repo,
				branch,
			}))),
		}
	}

	///
	pub fn result(&self) -> Option<Result<BranchCompare>> {
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

impl AsyncJob for AsyncBranchCompareJob {
	type Notification = AsyncGitNotification;
	type Progress = ();

	fn run(
		&mut self,
		_params: RunParams<Self::Notification, Self::Progress>,
	) -> Result<Self::Notification> {
		if let Ok(mut state) = self.state.lock() {
			*state = state.take().map(|state| match state {
				JobState::Request { repo, branch } => {
					let compare =
						branch_compare_upstream(&repo, &branch);

					JobState::Response(compare)
				}
				JobState::Response(result) => {
					JobState::Response(result)
				}
			});
		}

		Ok(AsyncGitNotification::BranchCompare)
	}
}
