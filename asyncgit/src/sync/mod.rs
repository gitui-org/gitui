//! sync git api

//TODO: remove once we have this activated on the toplevel
#![deny(clippy::expect_used)]

pub mod blame;
pub mod branch;
pub mod commit;
mod commit_details;
pub mod commit_files;
mod commit_filter;
mod commit_revert;
mod commits_info;
mod config;
pub mod cred;
pub mod diff;
mod hooks;
mod hunks;
mod ignore;
mod logwalker;
mod merge;
mod patches;
mod rebase;
pub mod remotes;
mod repository;
mod reset;
mod reword;
pub mod sign;
mod staging;
mod stash;
mod state;
pub mod status;
mod submodules;
mod tags;
mod tree;
pub mod utils;

pub use blame::{blame_file, BlameHunk, FileBlame};
pub use branch::{
	branch_compare_upstream, checkout_branch, checkout_commit,
	config_is_pull_rebase, create_branch, delete_branch,
	get_branch_remote, get_branch_upstream_merge, get_branches_info,
	merge_commit::merge_upstream_commit,
	merge_ff::branch_merge_upstream_fastforward,
	merge_rebase::merge_upstream_rebase, rename::rename_branch,
	validate_branch_name, BranchCompare, BranchDetails, BranchInfo,
};
pub use commit::{amend, commit, tag_commit};
pub use commit_details::{
	get_commit_details, CommitDetails, CommitMessage, CommitSignature,
};
pub use commit_files::get_commit_files;
pub use commit_filter::{
	diff_contains_file, filter_commit_by_search, LogFilterSearch,
	LogFilterSearchOptions, SearchFields, SearchOptions,
	SharedCommitFilterFn,
};
pub use commit_revert::{commit_revert, revert_commit, revert_head};
pub use commits_info::{
	get_commit_info, get_commits_info, CommitId, CommitInfo,
};
pub use config::{
	get_config_string, untracked_files_config,
	ShowUntrackedFilesConfig,
};
pub use diff::get_diff_commit;
pub use git2::BranchType;
pub use hooks::{
	hooks_commit_msg, hooks_post_commit, hooks_pre_commit,
	hooks_prepare_commit_msg, HookResult, PrepareCommitMsgSource,
};
pub use hunks::{reset_hunk, stage_hunk, unstage_hunk};
pub use ignore::add_to_ignore;
pub use logwalker::{LogWalker, LogWalkerWithoutFilter};
pub use merge::{
	abort_pending_rebase, abort_pending_state,
	continue_pending_rebase, merge_branch, merge_commit, merge_msg,
	mergehead_ids, rebase_progress,
};
pub use rebase::rebase_branch;
pub use remotes::{
	add_remote, delete_remote, get_default_remote,
	get_default_remote_for_fetch, get_default_remote_for_push,
	get_remote_url, get_remotes, push::AsyncProgress, rename_remote,
	tags::PushTagsProgress, update_remote_url, validate_remote_name,
};
pub(crate) use repository::repo;
pub use repository::{RepoPath, RepoPathRef};
pub use reset::{reset_repo, reset_stage, reset_workdir};
pub use reword::reword;
pub use staging::{discard_lines, stage_lines};
pub use stash::{
	get_stashes, stash_apply, stash_drop, stash_pop, stash_save,
};
pub use state::{repo_state, RepoState};
pub use status::is_workdir_clean;
pub use submodules::{
	get_submodules, submodule_parent_info, update_submodule,
	SubmoduleInfo, SubmoduleParentInfo, SubmoduleStatus,
};
pub use tags::{
	delete_tag, get_tags, get_tags_with_metadata, CommitTags, Tag,
	TagWithMetadata, Tags,
};
pub use tree::{tree_file_content, tree_files, TreeFile};
pub use utils::{
	get_head, get_head_tuple, repo_dir, repo_open_error,
	stage_add_all, stage_add_file, stage_addremoved, Head,
};

pub use git2::ResetType;

/// test utils
#[cfg(test)]
pub mod tests {
	use super::{
		commit,
		repository::repo,
		stage_add_file,
		status::{get_status, StatusType},
		utils::{get_head_repo, repo_write_file},
		CommitId, LogWalker, RepoPath,
	};
	use crate::error::Result;
	use git2::Repository;
	use std::{ffi::OsStr, path::Path, process::Command};
	use tempfile::TempDir;

	///
	pub fn repo_init_empty() -> Result<(TempDir, Repository)> {
		init_log();

		sandbox_config_files();

		let td = TempDir::new()?;
		let repo = Repository::init(td.path())?;
		{
			let mut config = repo.config()?;
			config.set_str("user.name", "name")?;
			config.set_str("user.email", "email")?;
		}
		Ok((td, repo))
	}

	///
	pub fn repo_init() -> Result<(TempDir, Repository)> {
		repo_init_with_prefix("gitui")
	}

	///
	#[inline]
	pub fn repo_init_with_prefix(
		prefix: impl AsRef<OsStr>,
	) -> Result<(TempDir, Repository)> {
		init_log();

		sandbox_config_files();

		let td = TempDir::with_prefix(prefix)?;
		let repo = Repository::init(td.path())?;
		{
			let mut config = repo.config()?;
			config.set_str("user.name", "name")?;
			config.set_str("user.email", "email")?;

			let mut index = repo.index()?;
			let id = index.write_tree()?;

			let tree = repo.find_tree(id)?;
			let sig = repo.signature()?;
			repo.commit(
				Some("HEAD"),
				&sig,
				&sig,
				"initial",
				&tree,
				&[],
			)?;
		}
		Ok((td, repo))
	}

	///
	pub fn repo_clone(p: &str) -> Result<(TempDir, Repository)> {
		sandbox_config_files();

		let td = TempDir::new()?;

		let td_path = td.path().as_os_str().to_str().unwrap();

		let repo = Repository::clone(p, td_path).unwrap();

		let mut config = repo.config()?;
		config.set_str("user.name", "name")?;
		config.set_str("user.email", "email")?;

		Ok((td, repo))
	}

	/// write, stage and commit a file
	pub fn write_commit_file(
		repo: &Repository,
		file: &str,
		content: &str,
		commit_name: &str,
	) -> CommitId {
		repo_write_file(repo, file, content).unwrap();

		stage_add_file(
			&repo.workdir().unwrap().to_str().unwrap().into(),
			Path::new(file),
		)
		.unwrap();

		commit(
			&repo.workdir().unwrap().to_str().unwrap().into(),
			commit_name,
		)
		.unwrap()
	}

	/// write, stage and commit a file giving the commit a specific timestamp
	pub fn write_commit_file_at(
		repo: &Repository,
		file: &str,
		content: &str,
		commit_name: &str,
		time: git2::Time,
	) -> CommitId {
		repo_write_file(repo, file, content).unwrap();

		let path: &RepoPath =
			&repo.workdir().unwrap().to_str().unwrap().into();

		stage_add_file(path, Path::new(file)).unwrap();

		commit_at(path, commit_name, time)
	}

	/// helper returning amount of files with changes in the (wd,stage)
	pub fn get_statuses(repo_path: &RepoPath) -> (usize, usize) {
		(
			get_status(repo_path, StatusType::WorkingDir, None)
				.unwrap()
				.len(),
			get_status(repo_path, StatusType::Stage, None)
				.unwrap()
				.len(),
		)
	}

	///
	pub fn debug_cmd_print(path: &RepoPath, cmd: &str) {
		let cmd = debug_cmd(path, cmd);
		eprintln!("\n----\n{cmd}");
	}

	/// helper to fetch commit details using log walker
	pub fn get_commit_ids(
		r: &Repository,
		max_count: usize,
	) -> Vec<CommitId> {
		let mut commit_ids = Vec::<CommitId>::new();
		LogWalker::new(r, max_count)
			.unwrap()
			.read(&mut commit_ids)
			.unwrap();

		commit_ids
	}

	/// Same as `repo_init`, but the repo is a bare repo (--bare)
	pub fn repo_init_bare() -> Result<(TempDir, Repository)> {
		init_log();

		let tmp_repo_dir = TempDir::new()?;
		let bare_repo = Repository::init_bare(tmp_repo_dir.path())?;
		Ok((tmp_repo_dir, bare_repo))
	}

	/// Calling `set_search_path` with an empty directory makes sure that there
	/// is no git config interfering with our tests (for example user-local
	/// `.gitconfig`).
	#[allow(unsafe_code)]
	fn sandbox_config_files() {
		use git2::{opts::set_search_path, ConfigLevel};
		use std::sync::Once;

		static INIT: Once = Once::new();

		// Adapted from https://github.com/rust-lang/cargo/pull/9035
		INIT.call_once(|| unsafe {
			let temp_dir = TempDir::new().unwrap();
			let path = temp_dir.path();

			set_search_path(ConfigLevel::System, path).unwrap();
			set_search_path(ConfigLevel::Global, path).unwrap();
			set_search_path(ConfigLevel::XDG, path).unwrap();
			set_search_path(ConfigLevel::ProgramData, path).unwrap();
		});
	}

	fn commit_at(
		repo_path: &RepoPath,
		msg: &str,
		time: git2::Time,
	) -> CommitId {
		let repo = repo(repo_path).unwrap();

		let signature =
			git2::Signature::new("name", "email", &time).unwrap();
		let mut index = repo.index().unwrap();
		let tree_id = index.write_tree().unwrap();
		let tree = repo.find_tree(tree_id).unwrap();

		let parents = if let Ok(id) = get_head_repo(&repo) {
			vec![repo.find_commit(id.into()).unwrap()]
		} else {
			Vec::new()
		};

		let parents = parents.iter().collect::<Vec<_>>();

		let commit = repo
			.commit(
				Some("HEAD"),
				&signature,
				&signature,
				msg,
				&tree,
				parents.as_slice(),
			)
			.unwrap()
			.into();

		commit
	}

	// init log
	fn init_log() {
		let _ = env_logger::builder()
			.is_test(true)
			.filter_level(log::LevelFilter::Trace)
			.try_init();
	}

	fn debug_cmd(path: &RepoPath, cmd: &str) -> String {
		let output = if cfg!(target_os = "windows") {
			Command::new("cmd")
				.args(["/C", cmd])
				.current_dir(path.gitpath())
				.output()
				.unwrap()
		} else {
			Command::new("sh")
				.arg("-c")
				.arg(cmd)
				.current_dir(path.gitpath())
				.output()
				.unwrap()
		};

		let stdout = String::from_utf8_lossy(&output.stdout);
		let stderr = String::from_utf8_lossy(&output.stderr);
		format!(
			"{}{}",
			if stdout.is_empty() {
				String::new()
			} else {
				format!("out:\n{stdout}")
			},
			if stderr.is_empty() {
				String::new()
			} else {
				format!("err:\n{stderr}")
			}
		)
	}
}
