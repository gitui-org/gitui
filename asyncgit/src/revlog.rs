use crate::{
	error::Result,
	graph::{GraphRow, GraphWalker},
	sync::{
		gix_repo, repo, CommitId, LogWalker, LogWalkerWithoutFilter,
		RepoPath, SharedCommitFilterFn,
	},
	AsyncGitNotification, Error,
};
use crossbeam_channel::Sender;
use scopetime::scope_time;
use std::{
	collections::HashSet,
	sync::{
		atomic::{AtomicBool, Ordering},
		Arc, Mutex,
	},
	thread,
	time::{Duration, Instant},
};

///
#[derive(PartialEq, Eq, Debug)]
pub enum FetchStatus {
	/// previous fetch still running
	Pending,
	/// no change expected
	NoChange,
	/// new walk was started
	Started,
}

///
pub struct AsyncLogResult {
	///
	pub commits: Vec<CommitId>,
	///
	pub duration: Duration,
}
/// Drives the background commit-log walker and exposes graph rows.
pub struct AsyncLog {
	current: Arc<Mutex<AsyncLogResult>>,
	current_head: Arc<Mutex<Option<CommitId>>>,
	sender: Sender<AsyncGitNotification>,
	pending: Arc<AtomicBool>,
	background: Arc<AtomicBool>,
	filter: Option<SharedCommitFilterFn>,
	partial_extract: AtomicBool,
	repo: RepoPath,
	/// All commit ids collected by the background thread, in walk order.
	/// The graph walker reads these lazily, only as far as the viewport
	/// requires, looking up each commit's parents on demand.
	walk_entries: Arc<Mutex<Vec<CommitId>>>,
	graph_walker: Arc<Mutex<GraphWalker>>,
}

static LIMIT_COUNT: usize = 3000;
static SLEEP_FOREGROUND: Duration = Duration::from_millis(2);
static SLEEP_BACKGROUND: Duration = Duration::from_secs(1);

impl AsyncLog {
	///
	pub fn new(
		repo: RepoPath,
		sender: &Sender<AsyncGitNotification>,
		filter: Option<SharedCommitFilterFn>,
	) -> Self {
		Self {
			repo,
			current: Arc::new(Mutex::new(AsyncLogResult {
				commits: Vec::new(),
				duration: Duration::default(),
			})),
			current_head: Arc::new(Mutex::new(None)),
			sender: sender.clone(),
			pending: Arc::new(AtomicBool::new(false)),
			background: Arc::new(AtomicBool::new(false)),
			filter,
			partial_extract: AtomicBool::new(false),
			walk_entries: Arc::new(Mutex::new(Vec::new())),
			graph_walker: Arc::new(Mutex::new(GraphWalker::new())),
		}
	}

	/// Computes graph rows for `commit_slice` starting at `global_start`.
	///
	/// Driven lazily. Processes only as many walked commits as the
	/// viewport requires, resolving their parents on demand.
	///
	/// Returns `None` when the background walk hasn't reached
	/// `global_start + commit_slice.len()` yet.
	pub fn get_graph_rows(
		&self,
		commit_slice: &[CommitId],
		global_start: usize,
		branch_tips: &HashSet<CommitId>,
		stashes: &HashSet<CommitId>,
		head_id: Option<&CommitId>,
	) -> Option<Vec<GraphRow>> {
		let needed_end = global_start + commit_slice.len();

		let mut walker = self.graph_walker.lock().ok()?;

		{
			let entries = self.walk_entries.lock().ok()?;
			if entries.len() < needed_end {
				return None;
			}

			// the walker may already be ahead of the requested range
			// (you know, scrolling up), so only feed it entries
			// we know it is yet to have seen
			let processed =
				walker.processed_commits().min(needed_end);

			// The graph only needs topology for the commits it is
			// about to fold in, so parents are looked up here on
			// demand instead of being carried along the whole walk.
			if processed < needed_end {
				let mut repo = gix_repo(&self.repo).ok()?;
				repo.object_cache_size_if_unset(2_usize.pow(14));

				for id in &entries[processed..needed_end] {
					let parents = Self::parents_of(&repo, *id).ok()?;
					walker.process(*id, &parents);
				}
			}
		}

		Some(walker.compute_rows(
			commit_slice,
			global_start,
			branch_tips,
			stashes,
			head_id,
		))
	}

	/// Looks up a commit's (up to two) parents on demand.
	///
	/// The graph caps support at two parents, ignoring octopus
	/// merges, so anything beyond the first two is dropped here.
	fn parents_of(
		repo: &gix::Repository,
		id: CommitId,
	) -> Result<Vec<CommitId>> {
		Ok(repo
			.find_commit(id)?
			.parent_ids()
			.take(2)
			.map(Into::into)
			.collect())
	}

	///
	pub fn count(&self) -> Result<usize> {
		Ok(self.current.lock()?.commits.len())
	}

	///
	pub fn get_slice(
		&self,
		start_index: usize,
		amount: usize,
	) -> Result<Vec<CommitId>> {
		if self.partial_extract.load(Ordering::Relaxed) {
			return Err(Error::Generic(String::from("Faulty usage of AsyncLog: Cannot partially extract items and rely on get_items slice to still work!")));
		}

		let list = &self.current.lock()?.commits;
		let list_len = list.len();
		let min = start_index.min(list_len);
		let max = min + amount;
		let max = max.min(list_len);
		Ok(list[min..max].to_vec())
	}

	///
	pub fn get_items(&self) -> Result<Vec<CommitId>> {
		if self.partial_extract.load(Ordering::Relaxed) {
			return Err(Error::Generic(String::from("Faulty usage of AsyncLog: Cannot partially extract items and rely on get_items slice to still work!")));
		}

		let list = &self.current.lock()?.commits;
		Ok(list.clone())
	}

	///
	pub fn extract_items(&self) -> Result<Vec<CommitId>> {
		self.partial_extract.store(true, Ordering::Relaxed);
		let list = &mut self.current.lock()?.commits;
		let result = list.clone();
		list.clear();
		Ok(result)
	}

	///
	pub fn get_last_duration(&self) -> Result<Duration> {
		Ok(self.current.lock()?.duration)
	}

	///
	pub fn is_pending(&self) -> bool {
		self.pending.load(Ordering::Relaxed)
	}

	///
	pub fn set_background(&self) {
		self.background.store(true, Ordering::Relaxed);
	}

	///
	fn current_head(&self) -> Result<Option<CommitId>> {
		Ok(*self.current_head.lock()?)
	}

	///
	fn head_changed(&self) -> Result<bool> {
		if let Ok(head) = repo(&self.repo)?.head() {
			return Ok(
				head.target() != self.current_head()?.map(Into::into)
			);
		}
		Ok(false)
	}

	///
	pub fn fetch(&self) -> Result<FetchStatus> {
		self.background.store(false, Ordering::Relaxed);

		if self.is_pending() {
			return Ok(FetchStatus::Pending);
		}

		if !self.head_changed()? {
			return Ok(FetchStatus::NoChange);
		}

		self.pending.store(true, Ordering::Relaxed);

		self.clear()?;

		let arc_current = Arc::clone(&self.current);
		let sender = self.sender.clone();
		let arc_pending = Arc::clone(&self.pending);
		let arc_background = Arc::clone(&self.background);
		let arc_walk_entries = Arc::clone(&self.walk_entries);
		let filter = self.filter.clone();
		let repo_path = self.repo.clone();

		if let Ok(head) = repo(&self.repo)?.head() {
			*self.current_head.lock()? =
				head.target().map(CommitId::new);
		}

		rayon_core::spawn(move || {
			scope_time!("async::revlog");

			Self::fetch_helper(
				&repo_path,
				&arc_current,
				&arc_background,
				&sender,
				&arc_walk_entries,
				filter,
			)
			.expect("failed to fetch");

			arc_pending.store(false, Ordering::Relaxed);

			Self::notify(&sender);
		});

		Ok(FetchStatus::Started)
	}

	fn fetch_helper(
		repo_path: &RepoPath,
		arc_current: &Arc<Mutex<AsyncLogResult>>,
		arc_background: &Arc<AtomicBool>,
		sender: &Sender<AsyncGitNotification>,
		arc_walk_entries: &Arc<Mutex<Vec<CommitId>>>,
		filter: Option<SharedCommitFilterFn>,
	) -> Result<()> {
		filter.map_or_else(
			|| {
				Self::fetch_helper_without_filter(
					repo_path,
					arc_current,
					arc_background,
					sender,
					arc_walk_entries,
				)
			},
			|filter| {
				Self::fetch_helper_with_filter(
					repo_path,
					arc_current,
					arc_background,
					sender,
					filter,
				)
			},
		)
	}

	/// A filtered walk yields a disconnected subset of the history,
	/// which the graph cannot represent, so no topology entries are
	/// collected here.
	fn fetch_helper_with_filter(
		repo_path: &RepoPath,
		arc_current: &Arc<Mutex<AsyncLogResult>>,
		arc_background: &Arc<AtomicBool>,
		sender: &Sender<AsyncGitNotification>,
		filter: SharedCommitFilterFn,
	) -> Result<()> {
		let r = repo(repo_path)?;
		let mut walker =
			LogWalker::new(&r, LIMIT_COUNT)?.filter(Some(filter));

		Self::walk_loop(
			|out| walker.read(out),
			arc_current,
			arc_background,
			sender,
			None,
		)?;

		log::trace!("revlog visited: {}", walker.visited());

		Ok(())
	}

	fn fetch_helper_without_filter(
		repo_path: &RepoPath,
		arc_current: &Arc<Mutex<AsyncLogResult>>,
		arc_background: &Arc<AtomicBool>,
		sender: &Sender<AsyncGitNotification>,
		arc_walk_entries: &Arc<Mutex<Vec<CommitId>>>,
	) -> Result<()> {
		let mut repo: gix::Repository = gix_repo(repo_path)?;
		let mut walker =
			LogWalkerWithoutFilter::new(&mut repo, LIMIT_COUNT)?;

		Self::walk_loop(
			|out| walker.read(out),
			arc_current,
			arc_background,
			sender,
			Some(arc_walk_entries),
		)?;

		log::trace!("revlog visited: {}", walker.visited());

		Ok(())
	}

	/// Drives `read` in batches, publishing every batch's commit ids
	/// to `arc_current` and (when given) moving the full entries into
	/// `walk_entries` for the graph.
	fn walk_loop(
		mut read: impl FnMut(&mut Vec<CommitId>) -> Result<usize>,
		arc_current: &Arc<Mutex<AsyncLogResult>>,
		arc_background: &Arc<AtomicBool>,
		sender: &Sender<AsyncGitNotification>,
		walk_entries: Option<&Mutex<Vec<CommitId>>>,
	) -> Result<()> {
		let start_time = Instant::now();

		let mut entries: Vec<CommitId> =
			Vec::with_capacity(LIMIT_COUNT);

		loop {
			let read_count = read(&mut entries)?;

			{
				let mut current = arc_current.lock()?;
				current.commits.extend(entries.iter().copied());
				current.duration = start_time.elapsed();
			}

			if let Some(walk_entries) = walk_entries {
				walk_entries.lock()?.append(&mut entries);
			} else {
				entries.clear();
			}

			if read_count == 0 {
				break;
			}
			Self::notify(sender);

			let sleep_duration =
				if arc_background.load(Ordering::Relaxed) {
					SLEEP_BACKGROUND
				} else {
					SLEEP_FOREGROUND
				};

			thread::sleep(sleep_duration);
		}

		Ok(())
	}

	fn clear(&self) -> Result<()> {
		self.current.lock()?.commits.clear();
		*self.current_head.lock()? = None;
		self.partial_extract.store(false, Ordering::Relaxed);
		self.walk_entries.lock()?.clear();
		*self.graph_walker.lock()? = GraphWalker::new();
		Ok(())
	}

	fn notify(sender: &Sender<AsyncGitNotification>) {
		sender
			.send(AsyncGitNotification::Log)
			.expect("error sending");
	}
}

#[cfg(test)]
mod tests {
	use std::sync::atomic::AtomicBool;
	use std::sync::{Arc, Mutex};
	use std::time::Duration;

	use crossbeam_channel::unbounded;
	use serial_test::serial;
	use tempfile::TempDir;

	use crate::sync::tests::{debug_cmd_print, repo_init};
	use crate::sync::RepoPath;
	use crate::AsyncLog;

	use super::AsyncLogResult;

	#[test]
	#[serial]
	fn test_smoke_in_subdir() {
		let (_td, repo) = repo_init().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: RepoPath =
			root.as_os_str().to_str().unwrap().into();

		let (tx_git, _rx_git) = unbounded();

		debug_cmd_print(&repo_path, "mkdir subdir");

		let subdir = repo.path().parent().unwrap().join("subdir");
		let subdir_path: RepoPath =
			subdir.as_os_str().to_str().unwrap().into();

		let arc_current = Arc::new(Mutex::new(AsyncLogResult {
			commits: Vec::new(),
			duration: Duration::default(),
		}));
		let arc_background = Arc::new(AtomicBool::new(false));
		let arc_walk_entries = Arc::new(Mutex::new(Vec::new()));

		let result = AsyncLog::fetch_helper_without_filter(
			&subdir_path,
			&arc_current,
			&arc_background,
			&tx_git,
			&arc_walk_entries,
		);

		assert_eq!(result.unwrap(), ());
	}

	#[test]
	#[serial]
	fn test_env_variables() {
		let (_td, repo) = repo_init().unwrap();
		let git_dir = repo.path();

		let (tx_git, _rx_git) = unbounded();

		let empty_dir = TempDir::new().unwrap();
		let empty_path: RepoPath =
			empty_dir.path().to_str().unwrap().into();

		let arc_current = Arc::new(Mutex::new(AsyncLogResult {
			commits: Vec::new(),
			duration: Duration::default(),
		}));
		let arc_background = Arc::new(AtomicBool::new(false));
		let arc_walk_entries = Arc::new(Mutex::new(Vec::new()));

		std::env::set_var("GIT_DIR", git_dir);

		let result = AsyncLog::fetch_helper_without_filter(
			// We pass an empty path, thus testing whether `GIT_DIR`, set above, is taken into account.
			&empty_path,
			&arc_current,
			&arc_background,
			&tx_git,
			&arc_walk_entries,
		);

		std::env::remove_var("GIT_DIR");

		assert_eq!(result.unwrap(), ());
	}
}
