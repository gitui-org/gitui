use anyhow::Result;
use asyncgit::sync::{
	diff::DiffOptions, repo_dir, RepoPathRef,
	ShowUntrackedFilesConfig,
};
use ron::{
	de::from_bytes,
	ser::{to_string_pretty, PrettyConfig},
};
use serde::{Deserialize, Serialize};
use std::{
	cell::RefCell,
	fs::File,
	io::{Read, Write},
	path::PathBuf,
	rc::Rc,
};

#[derive(Default, Clone, Serialize, Deserialize)]
struct OptionsData {
	pub tab: usize,
	pub diff: DiffOptions,
	pub status_show_untracked: Option<ShowUntrackedFilesConfig>,
	pub commit_msgs: Vec<String>,
	#[serde(default)]
	pub log_show_diff_stats: bool,
}

const COMMIT_MSG_HISTORY_LENGTH: usize = 20;

#[derive(Clone)]
pub struct Options {
	repo: RepoPathRef,
	data: OptionsData,
}

#[cfg(test)]
impl Options {
	pub fn test_env() -> Self {
		use asyncgit::sync::RepoPath;
		Self {
			repo: RefCell::new(RepoPath::Path(Default::default())),
			data: Default::default(),
		}
	}
}

pub type SharedOptions = Rc<RefCell<Options>>;

impl Options {
	pub fn new(repo: RepoPathRef) -> SharedOptions {
		Rc::new(RefCell::new(Self {
			data: Self::read(&repo).unwrap_or_default(),
			repo,
		}))
	}

	pub fn set_current_tab(&mut self, tab: usize) {
		self.data.tab = tab;
		self.save();
	}

	pub const fn current_tab(&self) -> usize {
		self.data.tab
	}

	pub const fn diff_options(&self) -> DiffOptions {
		self.data.diff
	}

	pub const fn status_show_untracked(
		&self,
	) -> Option<ShowUntrackedFilesConfig> {
		self.data.status_show_untracked
	}

	pub fn set_status_show_untracked(
		&mut self,
		value: Option<ShowUntrackedFilesConfig>,
	) {
		self.data.status_show_untracked = value;
		self.save();
	}

	pub const fn log_show_diff_stats(&self) -> bool {
		self.data.log_show_diff_stats
	}

	pub fn set_log_show_diff_stats(&mut self, show: bool) {
		self.data.log_show_diff_stats = show;
		self.save();
	}

	pub fn diff_context_change(&mut self, increase: bool) {
		self.data.diff.context = if increase {
			self.data.diff.context.saturating_add(1)
		} else {
			self.data.diff.context.saturating_sub(1)
		};

		self.save();
	}

	pub fn diff_hunk_lines_change(&mut self, increase: bool) {
		self.data.diff.interhunk_lines = if increase {
			self.data.diff.interhunk_lines.saturating_add(1)
		} else {
			self.data.diff.interhunk_lines.saturating_sub(1)
		};

		self.save();
	}

	pub fn diff_toggle_whitespace(&mut self) {
		self.data.diff.ignore_whitespace =
			!self.data.diff.ignore_whitespace;

		self.save();
	}

	pub fn add_commit_msg(&mut self, msg: &str) {
		self.data.commit_msgs.push(msg.to_owned());
		while self.data.commit_msgs.len() > COMMIT_MSG_HISTORY_LENGTH
		{
			self.data.commit_msgs.remove(0);
		}
		self.save();
	}

	pub const fn has_commit_msg_history(&self) -> bool {
		!self.data.commit_msgs.is_empty()
	}

	pub fn commit_msg(&self, idx: usize) -> Option<String> {
		if self.data.commit_msgs.is_empty() {
			None
		} else {
			let entries = self.data.commit_msgs.len();
			let mut index = idx;

			while index >= entries {
				index -= entries;
			}

			index = entries.saturating_sub(1) - index;

			Some(self.data.commit_msgs[index].clone())
		}
	}

	fn save(&self) {
		if let Err(e) = self.save_failable() {
			log::error!("options save error: {e}");
		}
	}

	fn read(repo: &RepoPathRef) -> Result<OptionsData> {
		let dir = Self::options_file(repo)?;

		let mut f = File::open(dir)?;
		let mut buffer = Vec::new();
		f.read_to_end(&mut buffer)?;
		Ok(from_bytes(&buffer)?)
	}

	fn save_failable(&self) -> Result<()> {
		let dir = Self::options_file(&self.repo)?;

		let mut file = File::create(dir)?;
		let data =
			to_string_pretty(&self.data, PrettyConfig::default())?;
		file.write_all(data.as_bytes())?;

		Ok(())
	}

	fn options_file(repo: &RepoPathRef) -> Result<PathBuf> {
		let dir = repo_dir(&repo.borrow())?;
		let dir = dir.join("gitui");
		Ok(dir)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn diff_stats_default_off_and_legacy_options() {
		assert!(!Options::test_env().log_show_diff_stats());
		let old = br#"(tab: 2, diff: (context: 7, interhunk_lines: 2, ignore_whitespace: true), status_show_untracked: None, commit_msgs: ["saved message"])"#;
		let data: OptionsData = from_bytes(old).unwrap();
		assert!(!data.log_show_diff_stats);
		assert_eq!(data.tab, 2);
		assert_eq!(data.diff.context, 7);
		assert_eq!(data.commit_msgs, ["saved message"]);
	}

	#[test]
	fn diff_stats_persist_per_repository() {
		let (first, _repo) = git2_testing::repo_init();
		let (second, _other) = git2_testing::repo_init();
		let path = || RefCell::new(first.path().to_path_buf().into());
		let options = Options::new(path());
		assert!(!options.borrow().log_show_diff_stats());
		options.borrow_mut().set_log_show_diff_stats(true);
		assert!(Options::new(path()).borrow().log_show_diff_stats());
		assert!(!Options::new(RefCell::new(
			second.path().to_path_buf().into()
		))
		.borrow()
		.log_show_diff_stats());
		options.borrow_mut().set_log_show_diff_stats(false);
		assert!(!Options::new(path()).borrow().log_show_diff_stats());
	}
}
