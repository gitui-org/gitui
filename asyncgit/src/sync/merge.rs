use crate::{
	error::{Error, Result},
	sync::{
		branch::merge_commit::commit_merge_with_head,
		rebase::{
			abort_rebase, continue_rebase, get_rebase_progress,
		},
		repository::repo,
		reset_stage, reset_workdir, CommitId,
	},
};
use git2::{BranchType, Commit, MergeOptions, Repository};
use scopetime::scope_time;

use super::{
	rebase::{RebaseProgress, RebaseState},
	RepoPath,
};

///
pub fn mergehead_ids(repo_path: &RepoPath) -> Result<Vec<CommitId>> {
	scope_time!("mergehead_ids");

	let mut repo = repo(repo_path)?;

	let mut ids: Vec<CommitId> = Vec::new();
	repo.mergehead_foreach(|id| {
		ids.push(CommitId::from(*id));
		true
	})?;

	Ok(ids)
}

/// does these steps:
/// * reset all staged changes,
/// * revert all changes in workdir
/// * cleanup repo merge state
pub fn abort_pending_state(repo_path: &RepoPath) -> Result<()> {
	scope_time!("abort_pending_state");

	let repo = repo(repo_path)?;

	reset_stage(repo_path, "*")?;
	reset_workdir(repo_path, "*")?;

	repo.cleanup_state()?;

	Ok(())
}

///
pub fn merge_branch(
	repo_path: &RepoPath,
	branch: &str,
	branch_type: BranchType,
) -> Result<()> {
	scope_time!("merge_branch");

	let repo = repo(repo_path)?;

	merge_branch_repo(&repo, branch, branch_type)?;

	Ok(())
}

///
pub fn rebase_progress(
	repo_path: &RepoPath,
) -> Result<RebaseProgress> {
	scope_time!("rebase_progress");

	let repo = repo(repo_path)?;

	get_rebase_progress(&repo)
}

///
pub fn continue_pending_rebase(
	repo_path: &RepoPath,
) -> Result<RebaseState> {
	scope_time!("continue_pending_rebase");

	let repo = repo(repo_path)?;

	continue_rebase(&repo)
}

///
pub fn abort_pending_rebase(repo_path: &RepoPath) -> Result<()> {
	scope_time!("abort_pending_rebase");

	let repo = repo(repo_path)?;

	abort_rebase(&repo)
}

///
pub fn merge_branch_repo(
	repo: &Repository,
	branch: &str,
	branch_type: BranchType,
) -> Result<()> {
	let branch = repo.find_branch(branch, branch_type)?;

	let annotated =
		repo.reference_to_annotated_commit(&branch.into_reference())?;

	let (analysis, _) = repo.merge_analysis(&[&annotated])?;

	//TODO: support merge on unborn
	if analysis.is_unborn() {
		return Err(Error::Generic("head is unborn".into()));
	}

	let mut opt = MergeOptions::default();

	repo.merge(&[&annotated], Some(&mut opt), None)?;

	Ok(())
}

///
pub fn merge_msg(repo_path: &RepoPath) -> Result<String> {
	scope_time!("merge_msg");

	let repo = repo(repo_path)?;
	let content = repo.message()?;

	Ok(content)
}

///
pub fn merge_commit(
	repo_path: &RepoPath,
	msg: &str,
	ids: &[CommitId],
) -> Result<CommitId> {
	scope_time!("merge_commit");

	let repo = repo(repo_path)?;

	let mut commits: Vec<Commit> = Vec::new();

	for id in ids {
		commits.push(repo.find_commit((*id).into())?);
	}

	let id = commit_merge_with_head(&repo, &commits, msg)?;

	Ok(id)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::sync::{
		create_branch,
		tests::{repo_init, write_commit_file},
		RepoPath,
	};
	use pretty_assertions::assert_eq;

	#[test]
	fn test_smoke() {
		let (_td, repo) = repo_init().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();

		let c1 =
			write_commit_file(&repo, "test.txt", "test", "commit1");

		create_branch(repo_path, "foo").unwrap();

		write_commit_file(&repo, "test.txt", "test2", "commit2");

		merge_branch(repo_path, "master", BranchType::Local).unwrap();

		let msg = merge_msg(repo_path).unwrap();

		assert_eq!(&msg[0..12], "Merge branch");

		let mergeheads = mergehead_ids(repo_path).unwrap();

		assert_eq!(mergeheads[0], c1);
	}
}
