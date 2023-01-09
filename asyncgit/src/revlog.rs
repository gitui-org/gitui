use crate::{
	asyncjob::{AsyncJob, AsyncSingleJob, RunParams},
	error::Result,
	sync::{repo, CommitId, LogWalker, LogWalkerFilter, RepoPath},
	AsyncGitNotification,
};
use crossbeam_channel::Sender;
use scopetime::scope_time;
use std::{
	sync::{
		atomic::{AtomicBool, Ordering},
		Arc, Mutex,
	},
	thread,
	time::Duration,
};

///
#[derive(PartialEq, Eq)]
pub enum FetchStatus {
	/// previous fetch still running
	Pending,
	/// no change expected
	NoChange,
	/// new walk was started
	Started,
}

///
pub struct AsyncLog {
	current: Arc<Mutex<Vec<CommitId>>>,
	current_head: Arc<Mutex<Option<CommitId>>>,
	last: Option<()>,
	sender: Sender<AsyncGitNotification>,
	job: AsyncSingleJob<AsyncFileLogJob>,
	background: Arc<AtomicBool>,
	filter: Option<LogWalkerFilter>,
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
		filter: Option<LogWalkerFilter>,
	) -> Self {
		Self {
			repo,
			current: Arc::new(Mutex::new(Vec::new())),
			current_head: Arc::new(Mutex::new(None)),
			last: None,
			sender: sender.clone(),
			job: AsyncSingleJob::new(sender.clone()),
			background: Arc::new(AtomicBool::new(false)),
			filter,
		}
	}

	///
	pub fn count(&self) -> Result<usize> {
		Ok(self.current.lock()?.len())
	}

	///
	pub fn get_slice(
		&self,
		start_index: usize,
		amount: usize,
	) -> Result<Vec<CommitId>> {
		let list = self.current.lock()?;
		let list_len = list.len();
		let min = start_index.min(list_len);
		let max = min + amount;
		let max = max.min(list_len);
		Ok(list[min..max].to_vec())
	}

	///
	pub fn position(&self, id: CommitId) -> Result<Option<usize>> {
		let list = self.current.lock()?;
		let position = list.iter().position(|&x| x == id);

		Ok(position)
	}

	///
	pub fn is_pending(&self) -> bool {
		self.job.is_pending()
	}

	///
	pub fn set_background(&mut self) {
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
	pub fn fetch(&mut self) -> Result<FetchStatus> {
		self.background.store(false, Ordering::Relaxed);

		if self.is_pending() {
			return Ok(FetchStatus::Pending);
		}

		if !self.head_changed()? {
			return Ok(FetchStatus::NoChange);
		}

		self.clear()?;

		let arc_current = Arc::clone(&self.current);
		let sender = self.sender.clone();
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

			Self::notify(&sender);
		});

		Ok(FetchStatus::Started)
	}

	fn fetch_helper(
		repo_path: &RepoPath,
		arc_current: &Arc<Mutex<Vec<CommitId>>>,
		arc_background: &Arc<AtomicBool>,
		sender: &Sender<AsyncGitNotification>,
		filter: Option<LogWalkerFilter>,
	) -> Result<()> {
		let mut entries = Vec::with_capacity(LIMIT_COUNT);
		let r = repo(repo_path)?;
		let mut walker =
			LogWalker::new(&r, LIMIT_COUNT)?.filter(filter);
		loop {
			entries.clear();
			let res_is_err = walker.read(&mut entries).is_err();

			if !res_is_err {
				let mut current = arc_current.lock()?;
				current.extend(entries.iter());
			}

			if res_is_err || entries.len() <= 1 {
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

	fn clear(&mut self) -> Result<()> {
		self.current.lock()?.clear();
		*self.current_head.lock()? = None;
		Ok(())
	}

	fn notify(sender: &Sender<AsyncGitNotification>) {
		sender
			.send(AsyncGitNotification::Log)
			.expect("error sending");
	}

	///
	pub fn request(&mut self) -> Result<()> {
		self.job.spawn(AsyncFileLogJob::new(
			self.repo.clone(),
			self.filter.clone(),
		));

		if let Some(job) = self.job.take_last() {
			if let Some(Ok(result)) = job.result() {
				self.last = Some(result);
			}
		}

		Ok(())
	}
}

enum JobState {
	Request(RepoPath, Option<LogWalkerFilter>),
	Response(()),
}

///
#[derive(Clone, Default)]
pub struct AsyncFileLogJob {
	state: Arc<Mutex<Option<JobState>>>,
}

impl AsyncFileLogJob {
	///
	pub fn new(
		repo: RepoPath,
		filter: Option<LogWalkerFilter>,
	) -> Self {
		Self {
			state: Arc::new(Mutex::new(Some(JobState::Request(
				repo, filter,
			)))),
		}
	}

	///
	pub fn result(&self) -> Option<Result<()>> {
		if let Ok(mut state) = self.state.lock() {
			if let Some(state) = state.take() {
				return match state {
					JobState::Request(_, _) => None,
					JobState::Response(_) => None,
				};
			}
		}

		None
	}

	fn fetch_log(repo_path: &RepoPath) -> Result<()> {
		let mut all_entries: Vec<CommitId> = Vec::new();
		let mut current_entries: Vec<CommitId> =
			Vec::with_capacity(LIMIT_COUNT);
		let repo = repo(&repo_path)?;
		let mut walker =
			LogWalker::new(&repo, LIMIT_COUNT)?.filter(None);

		loop {
			current_entries.clear();
			let res_is_err =
				walker.read(&mut current_entries).is_err();

			if !res_is_err {
				all_entries.extend(current_entries.iter());
			}

			break;
		}

		Ok(())
	}
}

impl AsyncJob for AsyncFileLogJob {
	type Notification = AsyncGitNotification;
	type Progress = ();

	fn run(
		&mut self,
		_params: RunParams<Self::Notification, Self::Progress>,
	) -> Result<Self::Notification> {
		let mut notification = AsyncGitNotification::FinishUnchanged;

		if let Ok(mut state) = self.state.lock() {
			*state = state.take().map(|state| match state {
				JobState::Request(repo_path, _) => {
					Self::fetch_log(&repo_path)
						.expect("failed to fetch");

					notification = AsyncGitNotification::Log;

					JobState::Response(())
				}
				JobState::Response(result) => {
					JobState::Response(result)
				}
			});
		}

		Ok(notification)
	}
}
