use crate::error::Result;
use crate::sync::repository::repo;
use scopetime::scope_time;

use super::RepoPath;

/// This should kinda represent a worktree
pub struct WorkTree {
	/// Worktree name (wich is also the folder i think)
	pub name: String,
	// Worktree branch name
	// pub branch: String,
}

// TODO: Optimize performance
// Maybe optimize somewhere else
/// Get all worktrees
pub fn worktrees(repo_path: &RepoPath) -> Result<Vec<WorkTree>> {
	scope_time!("worktrees");

	let repo_obj = repo(repo_path)?;

	Ok(repo_obj
		.worktrees()?
		.iter()
		.map(|s| WorkTree {
			name: s.unwrap().to_string(),
			// branch: worktree_branch(s.unwrap(), &repo_obj).unwrap(),
		})
		.collect())
}

/// Find a worktree
pub fn find_worktree(
	repo_path: &RepoPath,
	name: &str,
) -> Result<RepoPath> {
	scope_time!("find_worktree");

	let repo_obj = repo(repo_path)?;

	Ok(RepoPath::Path(
		repo_obj.find_worktree(name)?.path().to_path_buf(),
	))
}
