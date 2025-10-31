use crate::{
	error::Result,
	hash,
	sync::{self, CommitId, FileBlame, RepoPath},
	AsyncGitNotification,
};
use crossbeam_channel::Sender;
use std::{
	hash::Hash,
	sync::{
		atomic::{AtomicUsize, Ordering},
		Arc, Mutex,
	},
};

///
#[derive(Hash, Clone, PartialEq, Eq)]
pub struct BlameParams {
	/// path to the file to blame
	pub file_path: String,
	/// blame at a specific revision
	pub commit_id: Option<CommitId>,
}

struct Request<R, A>(R, Option<A>);

#[derive(Default, Clone)]
struct LastResult<P, R> {
	params: P,
	result: R,
}

///
pub struct AsyncBlame {
	current: Arc<Mutex<Request<u64, FileBlame>>>,
	last: Arc<Mutex<Option<LastResult<BlameParams, FileBlame>>>>,
	sender: Sender<AsyncGitNotification>,
	pending: Arc<AtomicUsize>,
	repo: RepoPath,
}

impl AsyncBlame {
	///
	pub fn new(
		repo: RepoPath,
		sender: &Sender<AsyncGitNotification>,
	) -> Self {
		Self {
			repo,
			current: Arc::new(Mutex::new(Request(0, None))),
			last: Arc::new(Mutex::new(None)),
			sender: sender.clone(),
			pending: Arc::new(AtomicUsize::new(0)),
		}
	}

	///
	pub fn last(&self) -> Result<Option<(BlameParams, FileBlame)>> {
		let last = self.last.lock()?;

		Ok(last.clone().map(|last_result| {
			(last_result.params, last_result.result)
		}))
	}

	///
	pub fn refresh(&self) -> Result<()> {
		if let Ok(Some(param)) = self.get_last_param() {
			self.clear_current()?;
			self.request(param)?;
		}
		Ok(())
	}

	///
	pub fn is_pending(&self) -> bool {
		self.pending.load(Ordering::Relaxed) > 0
	}

	///
	pub fn request(
		&self,
		params: BlameParams,
	) -> Result<Option<FileBlame>> {
		log::trace!("request");

		let hash = hash(&params);

		{
			let mut current = self.current.lock()?;

			if current.0 == hash {
				return Ok(current.1.clone());
			}

			current.0 = hash;
			current.1 = None;
		}

		let arc_current = Arc::clone(&self.current);
		let arc_last = Arc::clone(&self.last);
		let sender = self.sender.clone();
		let arc_pending = Arc::clone(&self.pending);
		let repo = self.repo.clone();

		self.pending.fetch_add(1, Ordering::Relaxed);

		rayon_core::spawn(move || {
			let notify = Self::get_blame_helper(
				&repo,
				params,
				&arc_last,
				&arc_current,
				hash,
			);

			let notify = match notify {
				Err(err) => {
					log::error!("get_blame_helper error: {err}");
					true
				}
				Ok(notify) => notify,
			};

			arc_pending.fetch_sub(1, Ordering::Relaxed);

			sender
				.send(if notify {
					AsyncGitNotification::Blame
				} else {
					AsyncGitNotification::FinishUnchanged
				})
				.expect("error sending blame");
		});

		Ok(None)
	}

	fn get_blame_helper(
		repo_path: &RepoPath,
		params: BlameParams,
		arc_last: &Arc<
			Mutex<Option<LastResult<BlameParams, FileBlame>>>,
		>,
		arc_current: &Arc<Mutex<Request<u64, FileBlame>>>,
		hash: u64,
	) -> Result<bool> {
		let file_blame = sync::blame::blame_file(
			repo_path,
			&params.file_path,
			params.commit_id,
		)?;

		let mut notify = false;
		{
			let mut current = arc_current.lock()?;
			if current.0 == hash {
				current.1 = Some(file_blame.clone());
				notify = true;
			}
		}

		{
			let mut last = arc_last.lock()?;
			*last = Some(LastResult {
				result: file_blame,
				params,
			});
		}

		Ok(notify)
	}

	fn get_last_param(&self) -> Result<Option<BlameParams>> {
		Ok(self
			.last
			.lock()?
			.clone()
			.map(|last_result| last_result.params))
	}

	fn clear_current(&self) -> Result<()> {
		let mut current = self.current.lock()?;
		current.0 = 0;
		current.1 = None;
		Ok(())
	}
}
