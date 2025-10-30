use crate::{
	error::{Error, Result},
	sync::{
		cred::BasicAuthCredential,
		remotes::tags::{push_tags, PushTagsProgress},
		RepoPath,
	},
	AsyncGitNotification, RemoteProgress,
};
use crossbeam_channel::{unbounded, Sender};
use std::{
	sync::{Arc, Mutex},
	thread,
};

///
#[derive(Default, Clone, Debug)]
pub struct PushTagsRequest {
	///
	pub remote: String,
	///
	pub basic_credential: Option<BasicAuthCredential>,
}

//TODO: since this is empty we can go with a simple AtomicBool to mark that we are fetching or not
#[derive(Default, Clone, Debug)]
struct PushState {}

///
pub struct AsyncPushTags {
	state: Arc<Mutex<Option<PushState>>>,
	last_result: Arc<Mutex<Option<String>>>,
	progress: Arc<Mutex<Option<PushTagsProgress>>>,
	sender: Sender<AsyncGitNotification>,
	repo: RepoPath,
}

impl AsyncPushTags {
	///
	pub fn new(
		repo: RepoPath,
		sender: &Sender<AsyncGitNotification>,
	) -> Self {
		Self {
			repo,
			state: Arc::new(Mutex::new(None)),
			last_result: Arc::new(Mutex::new(None)),
			progress: Arc::new(Mutex::new(None)),
			sender: sender.clone(),
		}
	}

	///
	pub fn is_pending(&self) -> Result<bool> {
		let state = self.state.lock()?;
		Ok(state.is_some())
	}

	///
	pub fn last_result(&self) -> Result<Option<String>> {
		let res = self.last_result.lock()?;
		Ok(res.clone())
	}

	///
	pub fn progress(&self) -> Result<Option<PushTagsProgress>> {
		let res = self.progress.lock()?;
		Ok(*res)
	}

	///
	pub fn request(&self, params: PushTagsRequest) -> Result<()> {
		log::trace!("request");

		if self.is_pending()? {
			return Ok(());
		}

		self.set_request(&params)?;
		RemoteProgress::set_progress(&self.progress, None)?;

		let arc_state = Arc::clone(&self.state);
		let arc_res = Arc::clone(&self.last_result);
		let arc_progress = Arc::clone(&self.progress);
		let sender = self.sender.clone();
		let repo = self.repo.clone();

		thread::spawn(move || {
			let (progress_sender, receiver) = unbounded();

			let handle = RemoteProgress::spawn_receiver_thread(
				AsyncGitNotification::PushTags,
				sender.clone(),
				receiver,
				arc_progress,
			);

			let res = push_tags(
				&repo,
				params.remote.as_str(),
				params.basic_credential.clone(),
				Some(progress_sender),
			);

			handle.join().expect("joining thread failed");

			Self::set_result(&arc_res, res).expect("result error");

			Self::clear_request(&arc_state).expect("clear error");

			sender
				.send(AsyncGitNotification::PushTags)
				.expect("error sending push");
		});

		Ok(())
	}

	fn set_request(&self, _params: &PushTagsRequest) -> Result<()> {
		let mut state = self.state.lock()?;

		if state.is_some() {
			return Err(Error::Generic("pending request".into()));
		}

		*state = Some(PushState {});

		Ok(())
	}

	fn clear_request(
		state: &Arc<Mutex<Option<PushState>>>,
	) -> Result<()> {
		let mut state = state.lock()?;

		*state = None;

		Ok(())
	}

	fn set_result(
		arc_result: &Arc<Mutex<Option<String>>>,
		res: Result<()>,
	) -> Result<()> {
		let mut last_res = arc_result.lock()?;

		*last_res = match res {
			Ok(()) => None,
			Err(e) => {
				log::error!("push error: {e}");
				Some(e.to_string())
			}
		};

		Ok(())
	}
}
