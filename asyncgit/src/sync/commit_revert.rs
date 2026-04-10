use super::{CommitId, RepoPath};
use crate::{
	error::Result,
	sync::{repository::repo, utils::read_file},
};
use scopetime::scope_time;

const GIT_REVERT_HEAD_FILE: &str = "REVERT_HEAD";

///
pub fn revert_commit(
	repo_path: &RepoPath,
	commit: CommitId,
) -> Result<()> {
	scope_time!("revert");

	let repo = repo(repo_path)?;

	let commit = repo.find_commit(commit.into())?;

	repo.revert(&commit, None)?;

	Ok(())
}

///
pub fn revert_head(repo_path: &RepoPath) -> Result<CommitId> {
	scope_time!("revert_head");

	let path = repo(repo_path)?.path().join(GIT_REVERT_HEAD_FILE);

	let file_content = read_file(&path)?;

	let id = git2::Oid::from_str(file_content.trim())?;

	Ok(id.into())
}

///
pub fn commit_revert(
	repo_path: &RepoPath,
	msg: &str,
) -> Result<CommitId> {
	scope_time!("commit_revert");

	let id = crate::sync::commit(repo_path, msg)?;

	repo(repo_path)?.cleanup_state()?;

	Ok(id)
}
