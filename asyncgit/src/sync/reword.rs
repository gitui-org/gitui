use git2::{Oid, RebaseOptions, Repository};

use super::{
	commit::signature_allow_undefined_name,
	repo,
	utils::{bytes2string, get_head_refname, get_head_repo},
	CommitId, RepoPath,
};
use crate::error::{Error, Result};

/// This is the same as reword, but will abort and fix the repo if something goes wrong
pub fn reword(
	repo_path: &RepoPath,
	commit: CommitId,
	message: &str,
) -> Result<CommitId> {
	let repo = repo(repo_path)?;
	let config = repo.config()?;

	if config.get_bool("commit.gpgsign").unwrap_or(false) {
		// HACK: we undo the last commit and create a new one
		use crate::sync::utils::undo_last_commit;

		let head = get_head_repo(&repo)?;
		if head == commit {
			// Check if there are any staged changes
			let parent = repo.find_commit(head.into())?;
			let tree = parent.tree()?;
			if repo
				.diff_tree_to_index(Some(&tree), None, None)?
				.deltas()
				.len() == 0
			{
				undo_last_commit(repo_path)?;
				return super::commit(repo_path, message);
			}

			return Err(Error::SignRewordLastCommitStaged);
		}

		return Err(Error::SignRewordNonLastCommit);
	}

	let cur_branch_ref = get_head_refname(&repo)?;

	match reword_internal(&repo, commit.get_oid(), message) {
		Ok(id) => Ok(id.into()),
		// Something went wrong, checkout the previous branch then error
		Err(e) => {
			if let Ok(mut rebase) = repo.open_rebase(None) {
				rebase.abort()?;
				repo.set_head(&cur_branch_ref)?;
				repo.checkout_head(None)?;
			}
			Err(e)
		}
	}
}

/// Gets the current branch the user is on.
/// Returns none if they are not on a branch
/// and Err if there was a problem finding the branch
fn get_current_branch(
	repo: &Repository,
) -> Result<Option<git2::Branch<'_>>> {
	for b in repo.branches(None)? {
		let branch = b?.0;
		if branch.is_head() {
			return Ok(Some(branch));
		}
	}
	Ok(None)
}

/// Changes the commit message of a commit with a specified oid
///
/// While this function is most commonly associated with doing a
/// reword operation in an interactive rebase, that is not how it
/// is implemented in git2rs
///
/// This is dangerous if it errors, as the head will be detached so this should
/// always be wrapped by another function which aborts the rebase if something goes wrong
fn reword_internal(
	repo: &Repository,
	commit: Oid,
	message: &str,
) -> Result<Oid> {
	let sig = signature_allow_undefined_name(repo)?;

	let parent_commit_oid = repo
		.find_commit(commit)?
		.parent(0)
		.map_or(None, |parent_commit| Some(parent_commit.id()));

	let commit_to_change = if let Some(pc_oid) = parent_commit_oid {
		// Need to start at one previous to the commit, so
		// first rebase.next() points to the actual commit we want to change
		repo.find_annotated_commit(pc_oid)?
	} else {
		return Err(Error::NoParent);
	};

	// If we are on a branch
	if let Ok(Some(branch)) = get_current_branch(repo) {
		let cur_branch_ref = bytes2string(branch.get().name_bytes())?;
		let cur_branch_name = bytes2string(branch.name_bytes()?)?;
		let top_branch_commit = repo.find_annotated_commit(
			branch.get().peel_to_commit()?.id(),
		)?;

		let mut rebase = repo.rebase(
			Some(&top_branch_commit),
			Some(&commit_to_change),
			None,
			Some(&mut RebaseOptions::default()),
		)?;

		let mut target;

		rebase.next();
		if parent_commit_oid.is_none() {
			return Err(Error::NoParent);
		}
		target = rebase.commit(None, &sig, Some(message))?;
		let reworded_commit = target;

		// Set target to top commit, don't know when the rebase will end
		// so have to loop till end
		while rebase.next().is_some() {
			target = rebase.commit(None, &sig, None)?;
		}
		rebase.finish(None)?;

		// Now override the previous branch
		repo.branch(
			&cur_branch_name,
			&repo.find_commit(target)?,
			true,
		)?;

		// Reset the head back to the branch then checkout head
		repo.set_head(&cur_branch_ref)?;
		repo.checkout_head(None)?;
		return Ok(reworded_commit);
	}
	// Repo is not on a branch, possibly detached head
	Err(Error::NoBranch)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::sync::{
		get_commit_info,
		tests::{repo_init_empty, write_commit_file},
	};
	use pretty_assertions::assert_eq;

	#[test]
	fn test_reword() {
		let (_td, repo) = repo_init_empty().unwrap();
		let root = repo.path().parent().unwrap();

		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();

		write_commit_file(&repo, "foo", "a", "commit1");

		let oid2 = write_commit_file(&repo, "foo", "ab", "commit2");

		let branch =
			repo.branches(None).unwrap().next().unwrap().unwrap().0;
		let branch_ref = branch.get();
		let commit_ref = branch_ref.peel_to_commit().unwrap();
		let message = commit_ref.message().unwrap();

		assert_eq!(message, "commit2");

		let reworded =
			reword(repo_path, oid2, "NewCommitMessage").unwrap();

		// Need to get the branch again as top oid has changed
		let branch =
			repo.branches(None).unwrap().next().unwrap().unwrap().0;
		let branch_ref = branch.get();
		let commit_ref_new = branch_ref.peel_to_commit().unwrap();
		let message_new = commit_ref_new.message().unwrap();
		assert_eq!(message_new, "NewCommitMessage");

		assert_eq!(
			message_new,
			get_commit_info(repo_path, &reworded).unwrap().message
		);
	}
}
