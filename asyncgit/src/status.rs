use crate::{
	error::Result,
	hash,
	sync::{
		self, status::StatusType, RepoPath, ShowUntrackedFilesConfig,
	},
	AsyncGitNotification, StatusItem,
};
use crossbeam_channel::Sender;
use std::{
	hash::Hash,
	sync::{
		atomic::{AtomicU64, AtomicUsize, Ordering},
		Arc, Mutex,
	},
};

#[derive(Default, Hash, Clone)]
pub struct Status {
	pub items: Vec<StatusItem>,
}

///
#[derive(Default, Hash, Copy, Clone, PartialEq, Eq)]
pub struct StatusParams {
	status_type: StatusType,
	config: Option<ShowUntrackedFilesConfig>,
}

impl StatusParams {
	///
	pub const fn new(
		status_type: StatusType,
		config: Option<ShowUntrackedFilesConfig>,
	) -> Self {
		Self {
			status_type,
			config,
		}
	}
}

struct Request<R, A>(R, Option<A>);

///
pub struct AsyncStatus {
	current: Arc<Mutex<Request<u64, Status>>>,
	last: Arc<Mutex<Status>>,
	sender: Sender<AsyncGitNotification>,
	pending: Arc<AtomicUsize>,
	repo: RepoPath,
	/// Counter that increments after each completed fetch.
	generation: Arc<AtomicU64>,
}

impl AsyncStatus {
	///
	pub fn new(
		repo: RepoPath,
		sender: Sender<AsyncGitNotification>,
	) -> Self {
		Self {
			repo,
			current: Arc::new(Mutex::new(Request(0, None))),
			last: Arc::new(Mutex::new(Status::default())),
			sender,
			pending: Arc::new(AtomicUsize::new(0)),
			generation: Arc::new(AtomicU64::new(0)),
		}
	}

	///
	pub fn last(&self) -> Result<Status> {
		let last = self.last.lock()?;
		Ok(last.clone())
	}

	///
	pub fn is_pending(&self) -> bool {
		self.pending.load(Ordering::Relaxed) > 0
	}

	///
	pub fn fetch(
		&self,
		params: &StatusParams,
	) -> Result<Option<Status>> {
		if self.is_pending() {
			log::trace!("request blocked, still pending");
			return Ok(None);
		}

		let generation = self.generation.load(Ordering::Relaxed);
		let hash_request = hash(&(params, generation));

		log::trace!(
			"request: [hash: {}] (type: {:?}, gen: {})",
			hash_request,
			params.status_type,
			generation,
		);

		{
			let mut current = self.current.lock()?;

			if current.0 == hash_request {
				return Ok(current.1.clone());
			}

			current.0 = hash_request;
			current.1 = None;
		}

		let arc_current = Arc::clone(&self.current);
		let arc_last = Arc::clone(&self.last);
		let arc_generation = Arc::clone(&self.generation);
		let sender = self.sender.clone();
		let arc_pending = Arc::clone(&self.pending);
		let status_type = params.status_type;
		let config = params.config;
		let repo = self.repo.clone();

		self.pending.fetch_add(1, Ordering::Relaxed);

		rayon_core::spawn(move || {
			if let Err(e) = Self::fetch_helper(
				&repo,
				status_type,
				config,
				hash_request,
				&arc_current,
				&arc_last,
			) {
				log::error!("fetch_helper: {e}");
			}

			// Increment generation to invalidate cache for next request
			arc_generation.fetch_add(1, Ordering::Relaxed);
			arc_pending.fetch_sub(1, Ordering::Relaxed);

			sender
				.send(AsyncGitNotification::Status)
				.expect("error sending status");
		});

		Ok(None)
	}

	fn fetch_helper(
		repo: &RepoPath,
		status_type: StatusType,
		config: Option<ShowUntrackedFilesConfig>,
		hash_request: u64,
		arc_current: &Arc<Mutex<Request<u64, Status>>>,
		arc_last: &Arc<Mutex<Status>>,
	) -> Result<()> {
		let res = Self::get_status(repo, status_type, config)?;
		log::trace!(
			"status fetched: {hash_request} (type: {status_type:?})",
		);

		{
			let mut current = arc_current.lock()?;
			if current.0 == hash_request {
				current.1 = Some(res.clone());
			}
		}

		{
			let mut last = arc_last.lock()?;
			*last = res;
		}

		Ok(())
	}

	fn get_status(
		repo: &RepoPath,
		status_type: StatusType,
		config: Option<ShowUntrackedFilesConfig>,
	) -> Result<Status> {
		Ok(Status {
			items: sync::status::get_status(
				repo,
				status_type,
				config,
			)?,
		})
	}
}
