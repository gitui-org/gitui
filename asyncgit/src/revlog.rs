use crate::{
	error::Result,
	sync::{
		repo, CommitId, LogWalker, LogWalkerWithoutFilter, RepoPath,
		SharedCommitFilterFn,
	},
	AsyncGitNotification, Error,
};
use crossbeam_channel::Sender;
use scopetime::scope_time;
use std::{
	cell::RefCell,
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
///
pub struct AsyncLog {
	current: Arc<Mutex<AsyncLogResult>>,
	current_head: Arc<Mutex<Option<CommitId>>>,
	sender: Sender<AsyncGitNotification>,
	pending: Arc<AtomicBool>,
	background: Arc<AtomicBool>,
	filter: Option<SharedCommitFilterFn>,
	partial_extract: AtomicBool,
	repo: RepoPath,
}

static LIMIT_COUNT: usize = 3000;
static SLEEP_FOREGROUND: Duration = Duration::from_millis(2);
static SLEEP_BACKGROUND: Duration = Duration::from_millis(1000);

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
		}
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
		filter: Option<SharedCommitFilterFn>,
	) -> Result<()> {
		filter.map_or_else(
			|| {
				Self::fetch_helper_without_filter(
					repo_path,
					arc_current,
					arc_background,
					sender,
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

	fn fetch_helper_with_filter(
		repo_path: &RepoPath,
		arc_current: &Arc<Mutex<AsyncLogResult>>,
		arc_background: &Arc<AtomicBool>,
		sender: &Sender<AsyncGitNotification>,
		filter: SharedCommitFilterFn,
	) -> Result<()> {
		let start_time = Instant::now();

		let entries =
			RefCell::new(vec![CommitId::default(); LIMIT_COUNT]);
		entries.borrow_mut().resize(0, CommitId::default());

		let r = repo(repo_path)?;
		let mut walker = LogWalker::new(&r, Some(LIMIT_COUNT))?
			.filter(Some(filter));

		loop {
			entries.borrow_mut().clear();
			let read = walker.read(Some(&entries))?;

			let mut current = arc_current.lock()?;
			current.commits.extend(entries.borrow().iter());
			current.duration = start_time.elapsed();

			if read == 0 {
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

		log::trace!("revlog visited: {}", walker.visited());

		Ok(())
	}

	fn fetch_helper_without_filter(
		repo_path: &RepoPath,
		arc_current: &Arc<Mutex<AsyncLogResult>>,
		arc_background: &Arc<AtomicBool>,
		sender: &Sender<AsyncGitNotification>,
	) -> Result<()> {
		let start_time = Instant::now();

		let mut entries = vec![CommitId::default(); LIMIT_COUNT];
		entries.resize(0, CommitId::default());

		let mut repo: gix::Repository =
				gix::ThreadSafeRepository::discover_with_environment_overrides(repo_path.gitpath())
						.map(Into::into)?;
		let mut walker =
			LogWalkerWithoutFilter::new(&mut repo, LIMIT_COUNT)?;

		loop {
			entries.clear();
			let read = walker.read(&mut entries)?;

			let mut current = arc_current.lock()?;
			current.commits.extend(entries.iter());
			current.duration = start_time.elapsed();

			if read == 0 {
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

		log::trace!("revlog visited: {}", walker.visited());

		Ok(())
	}

	fn clear(&self) -> Result<()> {
		self.current.lock()?.commits.clear();
		*self.current_head.lock()? = None;
		self.partial_extract.store(false, Ordering::Relaxed);
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

		let result = AsyncLog::fetch_helper_without_filter(
			&subdir_path,
			&arc_current,
			&arc_background,
			&tx_git,
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

		std::env::set_var("GIT_DIR", git_dir);

		let result = AsyncLog::fetch_helper_without_filter(
			// We pass an empty path, thus testing whether `GIT_DIR`, set above, is taken into account.
			&empty_path,
			&arc_current,
			&arc_background,
			&tx_git,
		);

		std::env::remove_var("GIT_DIR");

		assert_eq!(result.unwrap(), ());
	}
}
