//! read-only listing of git worktrees

use std::path::{Path, PathBuf};

use git2::{Repository, WorktreeLockStatus, WorktreePruneOptions};
use scopetime::scope_time;

use super::{is_workdir_clean, repo, RepoPath};
use crate::error::{Error, Result};

/// name reported for the primary working tree
const MAIN_WORKTREE_NAME: &str = "(main)";

/// Information about one git worktree (the primary tree or a linked one).
///
/// The four flag fields are independent (e.g. the primary tree can be
/// current or not, a linked tree can be locked or not, etc.), so they
/// don't collapse into a smaller enum; see `status_tree.rs` for the
/// same precedent in this codebase.
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct WorktreeInfo {
	/// Linked-worktree name; "(main)" for the primary working tree.
	pub name: String,
	/// Absolute path to the worktree's working directory.
	pub path: PathBuf,
	/// Short name of the checked-out branch (HEAD), or None if
	/// detached/unborn.
	pub branch: Option<String>,
	/// Whether the worktree's working directory is present/valid.
	pub is_valid: bool,
	/// Whether the worktree is locked.
	pub is_locked: bool,
	/// Whether this is the worktree gitui is currently operating in.
	pub is_current: bool,
	/// Whether this is the primary ("(main)") working tree, which
	/// cannot be removed or locked.
	pub is_main: bool,
}

/// Lists the worktrees of the repo at `repo_path`, returning the
/// primary working tree first followed by any linked worktrees.
pub fn get_worktrees(
	repo_path: &RepoPath,
) -> Result<Vec<WorktreeInfo>> {
	scope_time!("get_worktrees");

	let repo = repo(repo_path)?;

	// working dir gitui is currently operating in, used to flag the
	// matching entry as `is_current`.
	let current = repo.workdir();

	let mut worktrees = Vec::new();

	if let Some(main) = main_worktree_info(&repo, current) {
		worktrees.push(main);
	}

	// `iter()` yields `Result<Option<&str>, Error>`; the two
	// flattens drop unreadable/non-utf8 names, leaving `&str`.
	for name in repo.worktrees()?.iter().flatten().flatten() {
		worktrees.push(linked_worktree_info(&repo, name, current)?);
	}

	Ok(worktrees)
}

/// synthesizes the entry for the primary working tree, which is not
/// part of `Repository::worktrees`. returns `None` for bare repos
/// (no working tree) or when the primary workdir cannot be located.
fn main_worktree_info(
	repo: &Repository,
	current: Option<&Path>,
) -> Option<WorktreeInfo> {
	if repo.is_bare() {
		return None;
	}

	// primary working dir and the repo handle used to read its branch.
	let (main_workdir, branch) = if repo.is_worktree() {
		// gitui is operating inside a linked worktree; the primary
		// tree sits next to the shared common git dir.
		let workdir =
			repo.commondir().parent().map(Path::to_path_buf)?;

		let branch = Repository::open(&workdir)
			.ok()
			.and_then(|r| head_branch(&r));

		(workdir, branch)
	} else {
		let workdir = repo.workdir()?;

		(workdir.to_path_buf(), head_branch(repo))
	};

	Some(WorktreeInfo {
		is_current: same_workdir(&main_workdir, current),
		name: MAIN_WORKTREE_NAME.to_string(),
		path: main_workdir,
		branch,
		is_valid: true,
		is_locked: false,
		is_main: true,
	})
}

/// builds the entry for a single linked worktree.
fn linked_worktree_info(
	repo: &Repository,
	name: &str,
	current: Option<&Path>,
) -> Result<WorktreeInfo> {
	let wt = repo.find_worktree(name)?;

	let branch = Repository::open_from_worktree(&wt)
		.ok()
		.and_then(|r| head_branch(&r));

	let is_locked = wt.is_locked().is_ok_and(|status| {
		matches!(status, WorktreeLockStatus::Locked(_))
	});

	Ok(WorktreeInfo {
		is_current: same_workdir(wt.path(), current),
		name: name.to_string(),
		path: wt.path().to_path_buf(),
		branch,
		is_valid: wt.validate().is_ok(),
		is_locked,
		is_main: false,
	})
}

/// Creates a new linked worktree at `worktree_path`, checking out a
/// new branch named after the final path component.
///
/// `worktree_path` may be absolute or relative to the repository's
/// working directory. Returns the absolute path of the created
/// worktree.
pub fn create_worktree(
	repo_path: &RepoPath,
	worktree_path: &str,
) -> Result<PathBuf> {
	scope_time!("create_worktree");

	let repo = repo(repo_path)?;

	let requested = Path::new(worktree_path);

	let target = if requested.is_absolute() {
		requested.to_path_buf()
	} else {
		repo.workdir().ok_or(Error::NoWorkDir)?.join(requested)
	};

	let name = target
		.file_name()
		.and_then(|n| n.to_str())
		.ok_or_else(|| {
			Error::Generic(
				"invalid worktree path: no final component"
					.to_string(),
			)
		})?
		.to_string();

	// libgit2 only creates the leaf directory, not missing parents
	// (unlike `git worktree add`), so create the parent chain first
	// to support nested paths such as `worktrees/foo`.
	if let Some(parent) = target.parent() {
		std::fs::create_dir_all(parent)?;
	}

	let worktree = repo.worktree(&name, &target, None)?;

	Ok(worktree.path().to_path_buf())
}

/// Removes the linked worktree named `name`: deletes its working
/// directory and prunes its administrative files.
///
/// Refuses (returns an error, mirroring `git worktree remove` without
/// `--force`) when the worktree is the current one, is locked, or has
/// uncommitted/untracked changes — since removal deletes the working
/// directory irrecoverably.
pub fn remove_worktree(
	repo_path: &RepoPath,
	name: &str,
) -> Result<()> {
	scope_time!("remove_worktree");

	let repo = repo(repo_path)?;
	let worktree = repo.find_worktree(name)?;

	// never remove the worktree gitui is operating in.
	if same_workdir(worktree.path(), repo.workdir()) {
		return Err(Error::Generic(
			"cannot remove the current worktree".to_string(),
		));
	}

	// a locked worktree is deliberately protected; require unlock.
	if matches!(worktree.is_locked()?, WorktreeLockStatus::Locked(_))
	{
		return Err(Error::Generic(
			"worktree is locked; unlock it before removing"
				.to_string(),
		));
	}

	// refuse to delete a worktree with uncommitted or untracked
	// changes (libgit2 does not refuse this itself). Only checkable
	// when the working dir still exists.
	if worktree.validate().is_ok() {
		if let Some(path) = worktree.path().to_str() {
			let wt_path: RepoPath = path.into();
			if !is_workdir_clean(&wt_path, None)? {
				return Err(Error::Generic(
					"worktree has uncommitted changes; commit or discard them first"
						.to_string(),
				));
			}
		}
	}

	// valid(true): also prune worktrees whose dir still exists;
	// working_tree(true): delete the working directory too.
	let mut opts = WorktreePruneOptions::new();
	opts.valid(true).working_tree(true);
	worktree.prune(Some(&mut opts))?;

	Ok(())
}

/// Toggles the lock state of the linked worktree named `name`: locks
/// it (with no reason) when unlocked, unlocks it when locked.
pub fn toggle_worktree_lock(
	repo_path: &RepoPath,
	name: &str,
) -> Result<()> {
	scope_time!("toggle_worktree_lock");

	let repo = repo(repo_path)?;
	let worktree = repo.find_worktree(name)?;

	if matches!(worktree.is_locked()?, WorktreeLockStatus::Locked(_))
	{
		worktree.unlock()?;
	} else {
		worktree.lock(None)?;
	}

	Ok(())
}

/// short name of the branch a repo's HEAD points at, or `None` when
/// HEAD is unborn or detached.
fn head_branch(repo: &Repository) -> Option<String> {
	// a detached HEAD points at a commit, not a branch, and its
	// shorthand is "HEAD" rather than a branch name.
	if repo.head_detached().unwrap_or(false) {
		return None;
	}

	repo.head()
		.ok()
		.as_ref()
		.and_then(|head| head.shorthand().ok().map(String::from))
}

/// whether `path` and `current` refer to the same working directory,
/// comparing canonicalized paths and falling back to raw equality when
/// canonicalization fails.
fn same_workdir(path: &Path, current: Option<&Path>) -> bool {
	current.is_some_and(|current| {
		let canon = |p: &Path| std::fs::canonicalize(p).ok();
		match (canon(path), canon(current)) {
			(Some(a), Some(b)) => a == b,
			_ => path == current,
		}
	})
}

#[cfg(test)]
mod tests {
	use super::{
		create_worktree, get_worktrees, remove_worktree,
		toggle_worktree_lock, WorktreeInfo,
	};
	use crate::sync::{tests::repo_init, RepoPath};
	use pretty_assertions::assert_eq;

	fn find<'a>(
		list: &'a [WorktreeInfo],
		name: &str,
	) -> &'a WorktreeInfo {
		list.iter().find(|w| w.name == name).unwrap()
	}

	#[test]
	fn test_lists_primary_and_linked() {
		let (_td, repo) = repo_init().unwrap();

		let root = repo.path().parent().unwrap();
		let repo_path: RepoPath = root.to_str().unwrap().into();

		// linked worktree kept outside the main workdir to avoid
		// nesting; `wt_dir` must stay alive for the whole test.
		let wt_dir = tempfile::TempDir::new().unwrap();
		let wt_path = wt_dir.path().join("wt1");
		repo.worktree("wt1", &wt_path, None).unwrap();

		let list = get_worktrees(&repo_path).unwrap();

		assert_eq!(list.len(), 2);

		let linked = find(&list, "wt1");
		assert!(linked.path.is_absolute());
		assert!(linked.path.ends_with("wt1"));
		// git names the branch after the worktree by default.
		assert!(linked.branch.is_some());
		assert!(!linked.is_current);

		let primary = find(&list, "(main)");
		assert!(primary.is_current);
	}

	#[test]
	fn test_primary_only() {
		let (_td, repo) = repo_init().unwrap();

		let root = repo.path().parent().unwrap();
		let repo_path: RepoPath = root.to_str().unwrap().into();

		let list = get_worktrees(&repo_path).unwrap();

		assert_eq!(list.len(), 1);
		assert_eq!(list[0].name, "(main)");
		assert!(list[0].is_current);
		assert!(list[0].is_valid);
		assert!(!list[0].is_locked);
	}

	#[test]
	fn test_detached_head_has_no_branch() {
		let (_td, repo) = repo_init().unwrap();

		let root = repo.path().parent().unwrap();
		let repo_path: RepoPath = root.to_str().unwrap().into();

		let oid = repo.head().unwrap().target().unwrap();
		repo.set_head_detached(oid).unwrap();

		let list = get_worktrees(&repo_path).unwrap();

		assert_eq!(list.len(), 1);
		assert_eq!(list[0].name, "(main)");
		assert!(list[0].branch.is_none());
	}

	#[test]
	fn test_create_worktree_new_branch() {
		let (_td, repo) = repo_init().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: RepoPath = root.to_str().unwrap().into();

		// keep the linked worktree outside the main workdir; wt_dir
		// must stay alive for the whole test.
		let wt_dir = tempfile::TempDir::new().unwrap();
		let wt_path = wt_dir.path().join("feature-x");

		let created =
			create_worktree(&repo_path, wt_path.to_str().unwrap())
				.unwrap();

		assert!(created.ends_with("feature-x"));

		let list = get_worktrees(&repo_path).unwrap();
		let wt = find(&list, "feature-x");
		assert!(wt.path.is_absolute());
		// libgit2 names the new branch after the worktree.
		assert_eq!(wt.branch.as_deref(), Some("feature-x"));
		assert!(!wt.is_current);
	}

	#[test]
	fn test_create_worktree_creates_missing_parents() {
		let (_td, repo) = repo_init().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: RepoPath = root.to_str().unwrap().into();

		// nested path whose parent dirs do not exist yet; libgit2
		// would fail to mkdir the leaf without this being handled.
		let wt_dir = tempfile::TempDir::new().unwrap();
		let nested = wt_dir.path().join("a").join("b").join("wt");

		let created =
			create_worktree(&repo_path, nested.to_str().unwrap())
				.unwrap();

		assert!(created.ends_with("wt"));
		let list = get_worktrees(&repo_path).unwrap();
		let wt = find(&list, "wt");
		assert_eq!(wt.branch.as_deref(), Some("wt"));
	}

	#[test]
	fn test_create_worktree_rejects_pathless_name() {
		let (_td, repo) = repo_init().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: RepoPath = root.to_str().unwrap().into();

		// a path ending in ".." has no usable final component
		let res = create_worktree(&repo_path, "..");
		assert!(res.is_err());
	}

	#[test]
	fn test_remove_worktree() {
		let (_td, repo) = repo_init().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: RepoPath = root.to_str().unwrap().into();

		let wt_dir = tempfile::TempDir::new().unwrap();
		let wt_path = wt_dir.path().join("gone");
		create_worktree(&repo_path, wt_path.to_str().unwrap())
			.unwrap();
		assert_eq!(get_worktrees(&repo_path).unwrap().len(), 2);

		remove_worktree(&repo_path, "gone").unwrap();

		let list = get_worktrees(&repo_path).unwrap();
		assert_eq!(list.len(), 1);
		assert_eq!(list[0].name, "(main)");
		assert!(!wt_path.exists());
	}

	#[test]
	fn test_remove_worktree_refuses_dirty() {
		let (_td, repo) = repo_init().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: RepoPath = root.to_str().unwrap().into();

		let wt_dir = tempfile::TempDir::new().unwrap();
		let wt_path = wt_dir.path().join("dirty");
		create_worktree(&repo_path, wt_path.to_str().unwrap())
			.unwrap();

		// introduce an untracked file inside the worktree
		std::fs::write(wt_path.join("scratch.txt"), b"wip").unwrap();

		assert!(remove_worktree(&repo_path, "dirty").is_err());
		// still present because removal was refused
		assert_eq!(get_worktrees(&repo_path).unwrap().len(), 2);
	}

	#[test]
	fn test_toggle_worktree_lock() {
		let (_td, repo) = repo_init().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: RepoPath = root.to_str().unwrap().into();

		let wt_dir = tempfile::TempDir::new().unwrap();
		let wt_path = wt_dir.path().join("lockme");
		create_worktree(&repo_path, wt_path.to_str().unwrap())
			.unwrap();

		let locked = |p: &RepoPath| {
			get_worktrees(p)
				.unwrap()
				.into_iter()
				.find(|w| w.name == "lockme")
				.unwrap()
				.is_locked
		};

		assert!(!locked(&repo_path));
		toggle_worktree_lock(&repo_path, "lockme").unwrap();
		assert!(locked(&repo_path));
		toggle_worktree_lock(&repo_path, "lockme").unwrap();
		assert!(!locked(&repo_path));
	}
}
