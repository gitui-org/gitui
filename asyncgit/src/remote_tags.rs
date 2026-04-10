//!

use crate::{
	asyncjob::{AsyncJob, RunParams},
	error::Result,
	sync::cred::BasicAuthCredential,
	sync::{
		remotes::{get_default_remote, tags_missing_remote},
		RepoPath,
	},
	AsyncGitNotification,
};

use std::sync::{Arc, Mutex};

enum JobState {
	Request(Option<BasicAuthCredential>),
	Response(Result<Vec<String>>),
}

///
#[derive(Clone)]
pub struct AsyncRemoteTagsJob {
	state: Arc<Mutex<Option<JobState>>>,
	repo: RepoPath,
}

///
impl AsyncRemoteTagsJob {
	///
	pub fn new(
		repo: RepoPath,
		basic_credential: Option<BasicAuthCredential>,
	) -> Self {
		Self {
			repo,
			state: Arc::new(Mutex::new(Some(JobState::Request(
				basic_credential,
			)))),
		}
	}

	///
	pub fn result(&self) -> Option<Result<Vec<String>>> {
		if let Ok(mut state) = self.state.lock() {
			if let Some(state) = state.take() {
				return match state {
					JobState::Request(_) => None,
					JobState::Response(result) => Some(result),
				};
			}
		}

		None
	}
}

impl AsyncJob for AsyncRemoteTagsJob {
	type Notification = AsyncGitNotification;
	type Progress = ();

	fn run(
		&mut self,
		_params: RunParams<Self::Notification, Self::Progress>,
	) -> Result<Self::Notification> {
		if let Ok(mut state) = self.state.lock() {
			*state = state.take().map(|state| match state {
				JobState::Request(basic_credential) => {
					let result = get_default_remote(&self.repo)
						.and_then(|remote| {
							tags_missing_remote(
								&self.repo,
								&remote,
								basic_credential,
							)
						});

					JobState::Response(result)
				}
				JobState::Response(result) => {
					JobState::Response(result)
				}
			});
		}

		Ok(AsyncGitNotification::RemoteTags)
	}
}
