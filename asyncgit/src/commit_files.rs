use crate::{
	error::Result,
	sync::{self, commit_files::OldNew, CommitId, RepoPath},
	AsyncGitNotification, StatusItem,
};
use crossbeam_channel::Sender;
use std::sync::{
	atomic::{AtomicUsize, Ordering},
	Arc, Mutex,
};

type ResultType = Vec<StatusItem>;
struct Request<R, A>(R, A);

///
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct CommitFilesParams {
	///
	pub id: CommitId,
	///
	pub other: Option<CommitId>,
}

impl From<CommitId> for CommitFilesParams {
	fn from(id: CommitId) -> Self {
		Self { id, other: None }
	}
}

impl From<(CommitId, CommitId)> for CommitFilesParams {
	fn from((id, other): (CommitId, CommitId)) -> Self {
		Self {
			id,
			other: Some(other),
		}
	}
}

impl From<OldNew<CommitId>> for CommitFilesParams {
	fn from(old_new: OldNew<CommitId>) -> Self {
		Self {
			id: old_new.new,
			other: Some(old_new.old),
		}
	}
}

///
pub struct AsyncCommitFiles {
	current:
		Arc<Mutex<Option<Request<CommitFilesParams, ResultType>>>>,
	sender: Sender<AsyncGitNotification>,
	pending: Arc<AtomicUsize>,
	repo: RepoPath,
}

impl AsyncCommitFiles {
	///
	pub fn new(
		repo: RepoPath,
		sender: &Sender<AsyncGitNotification>,
	) -> Self {
		Self {
			repo,
			current: Arc::new(Mutex::new(None)),
			sender: sender.clone(),
			pending: Arc::new(AtomicUsize::new(0)),
		}
	}

	///
	pub fn current(
		&self,
	) -> Result<Option<(CommitFilesParams, ResultType)>> {
		let c = self.current.lock()?;

		c.as_ref()
			.map_or(Ok(None), |c| Ok(Some((c.0, c.1.clone()))))
	}

	///
	pub fn is_pending(&self) -> bool {
		self.pending.load(Ordering::Relaxed) > 0
	}

	///
	pub fn fetch(&self, params: CommitFilesParams) -> Result<()> {
		if self.is_pending() {
			return Ok(());
		}

		log::trace!("request: {params:?}");

		{
			let current = self.current.lock()?;
			if let Some(c) = &*current {
				if c.0 == params {
					return Ok(());
				}
			}
		}

		let arc_current = Arc::clone(&self.current);
		let sender = self.sender.clone();
		let arc_pending = Arc::clone(&self.pending);
		let repo = self.repo.clone();

		self.pending.fetch_add(1, Ordering::Relaxed);

		rayon_core::spawn(move || {
			Self::fetch_helper(&repo, params, &arc_current)
				.expect("failed to fetch");

			arc_pending.fetch_sub(1, Ordering::Relaxed);

			sender
				.send(AsyncGitNotification::CommitFiles)
				.expect("error sending");
		});

		Ok(())
	}

	fn fetch_helper(
		repo_path: &RepoPath,
		params: CommitFilesParams,
		arc_current: &Arc<
			Mutex<Option<Request<CommitFilesParams, ResultType>>>,
		>,
	) -> Result<()> {
		let res = sync::get_commit_files(
			repo_path,
			params.id,
			params.other,
		)?;

		log::trace!("get_commit_files: {:?} ({})", params, res.len());

		{
			let mut current = arc_current.lock()?;
			*current = Some(Request(params, res));
		}

		Ok(())
	}
}
