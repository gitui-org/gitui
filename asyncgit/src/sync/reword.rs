use git2::{Oid, RebaseOptions, Repository};

use super::{
	commit::signature_allow_undefined_name,
	repo,
	sign::{create_signed_commit, SignBuilder},
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

	let sign = config.get_bool("commit.gpgsign").unwrap_or(false);

	if sign {
		let head = get_head_repo(&repo)?;
		if head == commit {
			// HACK: we undo the last commit and create a new one
			use crate::sync::utils::undo_last_commit;

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
	}

	let cur_branch_ref = get_head_refname(&repo)?;

	let result = if sign {
		reword_signed(&repo, commit.get_oid(), message)
	} else {
		reword_unsigned(&repo, commit.get_oid(), message)
			.map(Into::into)
	};

	match result {
		Ok(id) => Ok(id),
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
fn reword_unsigned(
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

/// Reword a non-HEAD commit while honoring `commit.gpgsign`.
///
/// `git2`'s rebase API cannot sign commits, so we rebuild the chain
/// from `target` up to HEAD manually and sign each rewritten commit with `commit_signed`.
/// A pure reword does not alter trees, so the original tree of every commit in the chain is reused,
/// only parents, committer and (for `target`) the message are rewritten.
fn reword_signed(
	repo: &Repository,
	target: Oid,
	new_message: &str,
) -> Result<CommitId> {
	let config = repo.config()?;
	let signer = SignBuilder::from_gitconfig(repo, &config)?;
	let committer = signature_allow_undefined_name(repo)?;
	let signer = signer.as_ref();

	let cur_branch_ref = get_head_refname(repo)?;

	// collect commits from HEAD down to (and including) target.
	let mut chain = Vec::new();
	let mut cur = repo.head()?.peel_to_commit()?;
	loop {
		let id = cur.id();
		let parent_count = cur.parent_count();
		chain.push(cur);
		if id == target {
			break;
		}
		if parent_count != 1 {
			return Err(Error::Generic(
				"reword across merge commits is not supported"
					.to_string(),
			));
		}
		cur = repo.find_commit(id)?.parent(0)?;
	}

	// target first, then its descendants up to HEAD.
	chain.reverse();

	let (target_commit, descendants) =
		chain.split_first().ok_or_else(|| {
			Error::Generic("empty reword chain".to_string())
		})?;

	let target_parent =
		target_commit.parent(0).map_err(|_| Error::NoParent)?;
	let parents = [&target_parent];
	let new_target_oid = create_signed_commit(
		repo,
		signer,
		&target_commit.author(),
		&committer,
		new_message,
		&target_commit.tree()?,
		&parents,
	)?;

	let mut last_new_oid = new_target_oid;
	for original in descendants {
		let new_parent = repo.find_commit(last_new_oid)?;
		let parents = [&new_parent];
		let msg = original.message_raw().unwrap_or("");
		last_new_oid = create_signed_commit(
			repo,
			signer,
			&original.author(),
			&committer,
			msg,
			&original.tree()?,
			&parents,
		)?;
	}

	// move the branch to the rewritten tip and refresh the worktree.
	let mut branch_ref = repo.find_reference(&cur_branch_ref)?;
	branch_ref.set_target(last_new_oid, "gitui: reword (signed)")?;
	repo.set_head(&cur_branch_ref)?;
	repo.checkout_head(None)?;

	Ok(new_target_oid.into())
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

	#[cfg(unix)]
	#[test]
	fn test_reword_signed_non_head() {
		use std::os::unix::fs::PermissionsExt;

		let (td, repo) = repo_init_empty().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();

		// fake gpg program: drain stdin, emit a fixed PGP block on stdout and the GNUPG status line gitui's signer expects on stderr.
		let script_path = td.path().join("fake_gpg.sh");
		let script = "#!/bin/sh\n\
			cat > /dev/null\n\
			printf -- '-----BEGIN PGP SIGNATURE-----\\n\\nfake\\n-----END PGP SIGNATURE-----\\n'\n\
			printf '[GNUPG:] BEGIN_SIGNING\\n[GNUPG:] SIG_CREATED fake\\n' 1>&2\n";
		std::fs::write(&script_path, script).unwrap();
		std::fs::set_permissions(
			&script_path,
			std::fs::Permissions::from_mode(0o755),
		)
		.unwrap();

		{
			let mut cfg = repo.config().unwrap();
			cfg.set_bool("commit.gpgsign", true).unwrap();
			cfg.set_str("gpg.format", "openpgp").unwrap();
			cfg.set_str("gpg.program", script_path.to_str().unwrap())
				.unwrap();
			cfg.set_str("user.signingKey", "TEST_KEY").unwrap();
		}

		// build a 3-commit history with signing disabled so we can use
		// `write_commit_file`, then enable signing for the reword.
		repo.config()
			.unwrap()
			.set_bool("commit.gpgsign", false)
			.unwrap();
		write_commit_file(&repo, "foo", "a", "commit1");
		let oid2 = write_commit_file(&repo, "foo", "ab", "commit2");
		write_commit_file(&repo, "foo", "abc", "commit3");
		repo.config()
			.unwrap()
			.set_bool("commit.gpgsign", true)
			.unwrap();

		let reworded =
			reword(repo_path, oid2, "RewordedMiddle").unwrap();

		// reworded commit carries the new message.
		assert_eq!(
			get_commit_info(repo_path, &reworded).unwrap().message,
			"RewordedMiddle"
		);

		// HEAD still points to the descendant ("commit3") and the chain length is unchanged.
		let head = repo.head().unwrap().peel_to_commit().unwrap();
		assert_eq!(head.message().unwrap().trim_end(), "commit3");
		assert_eq!(head.parent_count(), 1);
		let middle = head.parent(0).unwrap();
		assert_eq!(middle.id(), reworded.get_oid());
		let first = middle.parent(0).unwrap();
		assert_eq!(first.message().unwrap().trim_end(), "commit1");
		assert!(first.parent(0).is_err());

		// both rewritten commits carry the fake PGP signature.
		let (sig_middle, _) =
			repo.extract_signature(&middle.id(), None).unwrap();
		assert!(sig_middle
			.as_str()
			.unwrap()
			.contains("BEGIN PGP SIGNATURE"));
		let (sig_head, _) =
			repo.extract_signature(&head.id(), None).unwrap();
		assert!(sig_head
			.as_str()
			.unwrap()
			.contains("BEGIN PGP SIGNATURE"));
	}
}
