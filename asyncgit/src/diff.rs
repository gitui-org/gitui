use crate::{
	error::Result,
	hash,
	sync::{
		self, commit_files::OldNew, diff::DiffOptions, CommitId,
		RepoPath,
	},
	AsyncGitNotification, FileDiff,
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
#[derive(Debug, Hash, Clone, PartialEq, Eq)]
pub enum DiffType {
	/// diff two commits
	Commits(OldNew<CommitId>),
	/// diff in a given commit
	Commit(CommitId),
	/// diff against staged file
	Stage,
	/// diff against file in workdir
	WorkDir,
}

///
#[derive(Debug, Hash, Clone, PartialEq, Eq)]
pub struct DiffParams {
	/// path to the file to diff
	pub path: String,
	/// what kind of diff
	pub diff_type: DiffType,
	/// diff options
	pub options: DiffOptions,
}

struct Request<R, A>(R, Option<A>);

#[derive(Default, Clone)]
struct LastResult<P, R> {
	params: P,
	result: R,
}

///
pub struct AsyncDiff {
	current: Arc<Mutex<Request<u64, FileDiff>>>,
	last: Arc<Mutex<Option<LastResult<DiffParams, FileDiff>>>>,
	sender: Sender<AsyncGitNotification>,
	pending: Arc<AtomicUsize>,
	repo: RepoPath,
}

impl AsyncDiff {
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
	pub fn last(&self) -> Result<Option<(DiffParams, FileDiff)>> {
		let last = self.last.lock()?;

		Ok(last.clone().map(|res| (res.params, res.result)))
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
		params: DiffParams,
	) -> Result<Option<FileDiff>> {
		log::trace!("request {params:?}");

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
			let notify = Self::get_diff_helper(
				&repo,
				params,
				&arc_last,
				&arc_current,
				hash,
			);

			let notify = match notify {
				Err(e) => {
					log::error!("get_diff_helper error: {e}");
					true
				}
				Ok(notify) => notify,
			};

			arc_pending.fetch_sub(1, Ordering::Relaxed);

			sender
				.send(if notify {
					AsyncGitNotification::Diff
				} else {
					AsyncGitNotification::FinishUnchanged
				})
				.expect("error sending diff");
		});

		Ok(None)
	}

	fn get_diff_helper(
		repo_path: &RepoPath,
		params: DiffParams,
		arc_last: &Arc<
			Mutex<Option<LastResult<DiffParams, FileDiff>>>,
		>,
		arc_current: &Arc<Mutex<Request<u64, FileDiff>>>,
		hash: u64,
	) -> Result<bool> {
		let res = match params.diff_type {
			DiffType::Stage => sync::diff::get_diff(
				repo_path,
				&params.path,
				true,
				Some(params.options),
			)?,
			DiffType::WorkDir => sync::diff::get_diff(
				repo_path,
				&params.path,
				false,
				Some(params.options),
			)?,
			DiffType::Commit(id) => sync::diff::get_diff_commit(
				repo_path,
				id,
				params.path.clone(),
				Some(params.options),
			)?,
			DiffType::Commits(ids) => sync::diff::get_diff_commits(
				repo_path,
				ids,
				params.path.clone(),
				Some(params.options),
			)?,
		};

		let mut notify = false;
		{
			let mut current = arc_current.lock()?;
			if current.0 == hash {
				current.1 = Some(res.clone());
				notify = true;
			}
		}

		{
			let mut last = arc_last.lock()?;
			*last = Some(LastResult {
				result: res,
				params,
			});
		}

		Ok(notify)
	}

	fn get_last_param(&self) -> Result<Option<DiffParams>> {
		Ok(self.last.lock()?.clone().map(|e| e.params))
	}

	fn clear_current(&self) -> Result<()> {
		let mut current = self.current.lock()?;
		current.0 = 0;
		current.1 = None;
		Ok(())
	}
}
