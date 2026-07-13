//! read-only listing of git worktrees

use std::path::{Path, PathBuf};

use git2::{Repository, WorktreeLockStatus};
use scopetime::scope_time;

use super::{repo, RepoPath};
use crate::error::{Error, Result};

/// name reported for the primary working tree
const MAIN_WORKTREE_NAME: &str = "(main)";

/// Information about one git worktree (the primary tree or a linked one).
#[derive(Debug, Clone)]
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

	let worktree = repo.worktree(&name, &target, None)?;

	Ok(worktree.path().to_path_buf())
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
	use super::{create_worktree, get_worktrees, WorktreeInfo};
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
	fn test_create_worktree_rejects_pathless_name() {
		let (_td, repo) = repo_init().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: RepoPath = root.to_str().unwrap().into();

		// a path ending in ".." has no usable final component
		let res = create_worktree(&repo_path, "..");
		assert!(res.is_err());
	}
}
