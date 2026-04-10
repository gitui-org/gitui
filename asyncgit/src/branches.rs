use crate::{
	asyncjob::{AsyncJob, RunParams},
	error::Result,
	sync::{branch::get_branches_info, BranchInfo, RepoPath},
	AsyncGitNotification,
};
use std::sync::{Arc, Mutex};

enum JobState {
	Request {
		local_branches: bool,
		repo: RepoPath,
	},
	Response(Result<Vec<BranchInfo>>),
}

///
#[derive(Clone, Default)]
pub struct AsyncBranchesJob {
	state: Arc<Mutex<Option<JobState>>>,
}

///
impl AsyncBranchesJob {
	///
	pub fn new(repo: RepoPath, local_branches: bool) -> Self {
		Self {
			state: Arc::new(Mutex::new(Some(JobState::Request {
				repo,
				local_branches,
			}))),
		}
	}

	///
	pub fn result(&self) -> Option<Result<Vec<BranchInfo>>> {
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

impl AsyncJob for AsyncBranchesJob {
	type Notification = AsyncGitNotification;
	type Progress = ();

	fn run(
		&mut self,
		_params: RunParams<Self::Notification, Self::Progress>,
	) -> Result<Self::Notification> {
		if let Ok(mut state) = self.state.lock() {
			*state = state.take().map(|state| match state {
				JobState::Request {
					local_branches,
					repo,
				} => {
					let branches =
						get_branches_info(&repo, local_branches);

					JobState::Response(branches)
				}
				JobState::Response(result) => {
					JobState::Response(result)
				}
			});
		}

		Ok(AsyncGitNotification::Branches)
	}
}
