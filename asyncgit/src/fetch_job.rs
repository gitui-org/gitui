//!

use crate::{
	asyncjob::{AsyncJob, RunParams},
	error::Result,
	sync::remotes::fetch_all,
	sync::{cred::BasicAuthCredential, RepoPath},
	AsyncGitNotification, ProgressPercent,
};

use std::sync::{Arc, Mutex};

enum JobState {
	Request(Option<BasicAuthCredential>),
	Response(Result<()>),
}

///
#[derive(Clone)]
pub struct AsyncFetchJob {
	state: Arc<Mutex<Option<JobState>>>,
	repo: RepoPath,
}

///
impl AsyncFetchJob {
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
}

impl AsyncJob for AsyncFetchJob {
	type Notification = AsyncGitNotification;
	type Progress = ProgressPercent;

	fn run(
		&mut self,
		_params: RunParams<Self::Notification, Self::Progress>,
	) -> Result<Self::Notification> {
		if let Ok(mut state) = self.state.lock() {
			*state = state.take().map(|state| match state {
				JobState::Request(basic_credentials) => {
					//TODO: support progress
					let result = fetch_all(
						&self.repo,
						&basic_credentials,
						&None,
					);

					JobState::Response(result)
				}
				JobState::Response(result) => {
					JobState::Response(result)
				}
			});
		}

		Ok(AsyncGitNotification::Fetch)
	}
}
