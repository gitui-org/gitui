//! merging from upstream (rebase)

use crate::{
	error::{Error, Result},
	sync::{
		rebase::conflict_free_rebase, repository::repo, CommitId,
		RepoPath,
	},
};
use git2::{BranchType, ErrorCode, Oid, Repository, StashFlags};
use scopetime::scope_time;

/// tries merging current branch with its upstream using rebase
///
/// honors the `rebase.autoStash` git config: when it's enabled and the
/// working tree carries uncommitted changes to tracked files, those
/// changes are stashed before the rebase and re-applied afterwards, so a
/// pull doesn't fail just because the tree is dirty.
pub fn merge_upstream_rebase(
	repo_path: &RepoPath,
	branch_name: &str,
) -> Result<CommitId> {
	scope_time!("merge_upstream_rebase");

	let mut repo = repo(repo_path)?;
	if super::get_branch_name_repo(&repo)? != branch_name {
		return Err(Error::Generic(String::from(
			"can only rebase in head branch",
		)));
	}

	let upstream_id = {
		let branch =
			repo.find_branch(branch_name, BranchType::Local)?;
		let upstream = branch.upstream()?;
		upstream.get().peel_to_commit()?.id()
	};

	let autostash = if rebase_autostash_enabled(&repo)? {
		autostash_save(&mut repo)?
	} else {
		None
	};

	let rebase_result = {
		let annotated_upstream =
			repo.find_annotated_commit(upstream_id)?;
		conflict_free_rebase(&repo, &annotated_upstream)
	};

	if let Some(stash_id) = autostash {
		// always restore the autostash, whether the rebase finished or
		// failed. if the rebase aborted, its HEAD is back where it
		// started and the pop simply restores the dirty tree; if it
		// succeeded, the changes are re-applied on top of the new HEAD.
		let pop_result = autostash_pop(&mut repo, stash_id);
		// surface the rebase failure first, if any: the pop has already
		// put the user's changes back for them.
		let commit = rebase_result?;
		// a conflicting pop leaves the stash entry in place (git2 only
		// drops it on a clean apply), matching git's autostash behavior.
		pop_result?;
		Ok(commit)
	} else {
		rebase_result
	}
}

/// reads the `rebase.autoStash` bool git config (defaults to false)
fn rebase_autostash_enabled(repo: &Repository) -> Result<bool> {
	Ok(repo.config()?.get_bool("rebase.autoStash").unwrap_or(false))
}

/// stashes tracked changes ahead of an autostash rebase. returns `None`
/// when the tree is clean and nothing needed stashing. untracked files
/// are left alone, matching git's autostash.
fn autostash_save(repo: &mut Repository) -> Result<Option<Oid>> {
	let signature = repo.signature()?;

	match repo.stash_save2(
		&signature,
		Some("gitui: autostash before rebase"),
		Some(StashFlags::DEFAULT),
	) {
		Ok(id) => Ok(Some(id)),
		// nothing to stash: the tree was clean
		Err(e) if e.code() == ErrorCode::NotFound => Ok(None),
		Err(e) => Err(e.into()),
	}
}

/// pops the autostash entry identified by `stash_id`
fn autostash_pop(repo: &mut Repository, stash_id: Oid) -> Result<()> {
	let mut index = None;
	repo.stash_foreach(|i, _msg, id| {
		if *id == stash_id {
			index = Some(i);
			false
		} else {
			true
		}
	})?;

	let index = index.ok_or_else(|| {
		Error::Generic(String::from("autostash entry not found"))
	})?;

	repo.stash_pop(index, None)?;

	Ok(())
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::sync::{
		branch_compare_upstream, get_commits_info, get_stashes,
		remotes::{fetch, push::push_branch},
		tests::{
			debug_cmd_print, get_commit_ids, repo_clone,
			repo_init_bare, write_commit_file, write_commit_file_at,
		},
		RepoState,
	};
	use git2::{Repository, Time};
	use std::fs;

	fn get_commit_msgs(r: &Repository) -> Vec<String> {
		let commits = get_commit_ids(r, 10);
		get_commits_info(
			&r.workdir().unwrap().to_str().unwrap().into(),
			&commits,
			10,
		)
		.unwrap()
		.into_iter()
		.map(|c| c.message)
		.collect()
	}

	#[test]
	fn test_merge_normal() {
		let (r1_dir, _repo) = repo_init_bare().unwrap();

		let (clone1_dir, clone1) =
			repo_clone(r1_dir.path().to_str().unwrap()).unwrap();

		let clone1_dir = clone1_dir.path().to_str().unwrap();

		// clone1

		let _commit1 = write_commit_file_at(
			&clone1,
			"test.txt",
			"test",
			"commit1",
			git2::Time::new(0, 0),
		);

		assert!(!clone1.head_detached().unwrap());

		push_branch(
			&clone1_dir.into(),
			"origin",
			"master",
			false,
			false,
			None,
			None,
		)
		.unwrap();

		assert!(!clone1.head_detached().unwrap());

		// clone2

		let (clone2_dir, clone2) =
			repo_clone(r1_dir.path().to_str().unwrap()).unwrap();

		let clone2_dir = clone2_dir.path().to_str().unwrap();

		let _commit2 = write_commit_file_at(
			&clone2,
			"test2.txt",
			"test",
			"commit2",
			git2::Time::new(1, 0),
		);

		assert!(!clone2.head_detached().unwrap());

		push_branch(
			&clone2_dir.into(),
			"origin",
			"master",
			false,
			false,
			None,
			None,
		)
		.unwrap();

		assert!(!clone2.head_detached().unwrap());

		// clone1

		let _commit3 = write_commit_file_at(
			&clone1,
			"test3.txt",
			"test",
			"commit3",
			git2::Time::new(2, 0),
		);

		assert!(!clone1.head_detached().unwrap());

		//lets fetch from origin
		let bytes =
			fetch(&clone1_dir.into(), "master", None, None).unwrap();
		assert!(bytes > 0);

		//we should be one commit behind
		assert_eq!(
			branch_compare_upstream(&clone1_dir.into(), "master")
				.unwrap()
				.behind,
			1
		);

		// debug_cmd_print(clone1_dir, "git status");

		assert!(!clone1.head_detached().unwrap());

		merge_upstream_rebase(&clone1_dir.into(), "master").unwrap();

		debug_cmd_print(&clone1_dir.into(), "git log");

		let state =
			crate::sync::repo_state(&clone1_dir.into()).unwrap();
		assert_eq!(state, RepoState::Clean);

		let commits = get_commit_msgs(&clone1);
		assert_eq!(
			commits,
			vec![
				String::from("commit3"),
				String::from("commit2"),
				String::from("commit1")
			]
		);

		assert!(!clone1.head_detached().unwrap());
	}

	#[test]
	fn test_merge_multiple() {
		let (r1_dir, _repo) = repo_init_bare().unwrap();

		let (clone1_dir, clone1) =
			repo_clone(r1_dir.path().to_str().unwrap()).unwrap();

		let clone1_dir = clone1_dir.path().to_str().unwrap();

		// clone1

		write_commit_file_at(
			&clone1,
			"test.txt",
			"test",
			"commit1",
			Time::new(0, 0),
		);

		push_branch(
			&clone1_dir.into(),
			"origin",
			"master",
			false,
			false,
			None,
			None,
		)
		.unwrap();

		// clone2

		let (clone2_dir, clone2) =
			repo_clone(r1_dir.path().to_str().unwrap()).unwrap();

		let clone2_dir = clone2_dir.path().to_str().unwrap();

		write_commit_file_at(
			&clone2,
			"test2.txt",
			"test",
			"commit2",
			Time::new(1, 0),
		);

		push_branch(
			&clone2_dir.into(),
			"origin",
			"master",
			false,
			false,
			None,
			None,
		)
		.unwrap();

		// clone1

		write_commit_file_at(
			&clone1,
			"test3.txt",
			"test",
			"commit3",
			Time::new(2, 0),
		);
		write_commit_file_at(
			&clone1,
			"test4.txt",
			"test",
			"commit4",
			Time::new(3, 0),
		);

		//lets fetch from origin

		fetch(&clone1_dir.into(), "master", None, None).unwrap();

		merge_upstream_rebase(&clone1_dir.into(), "master").unwrap();

		debug_cmd_print(&clone1_dir.into(), "git log");

		let state =
			crate::sync::repo_state(&clone1_dir.into()).unwrap();
		assert_eq!(state, RepoState::Clean);

		let commits = get_commit_msgs(&clone1);
		assert_eq!(
			commits,
			vec![
				String::from("commit4"),
				String::from("commit3"),
				String::from("commit2"),
				String::from("commit1")
			]
		);

		assert!(!clone1.head_detached().unwrap());
	}

	#[test]
	fn test_merge_conflict() {
		let (r1_dir, _repo) = repo_init_bare().unwrap();

		let (clone1_dir, clone1) =
			repo_clone(r1_dir.path().to_str().unwrap()).unwrap();

		let clone1_dir = clone1_dir.path().to_str().unwrap();

		// clone1

		let _commit1 =
			write_commit_file(&clone1, "test.txt", "test", "commit1");

		push_branch(
			&clone1_dir.into(),
			"origin",
			"master",
			false,
			false,
			None,
			None,
		)
		.unwrap();

		// clone2

		let (clone2_dir, clone2) =
			repo_clone(r1_dir.path().to_str().unwrap()).unwrap();

		let clone2_dir = clone2_dir.path().to_str().unwrap();

		let _commit2 = write_commit_file(
			&clone2,
			"test2.txt",
			"test",
			"commit2",
		);

		push_branch(
			&clone2_dir.into(),
			"origin",
			"master",
			false,
			false,
			None,
			None,
		)
		.unwrap();

		// clone1

		let _commit3 =
			write_commit_file(&clone1, "test2.txt", "foo", "commit3");

		let bytes =
			fetch(&clone1_dir.into(), "master", None, None).unwrap();
		assert!(bytes > 0);

		assert_eq!(
			branch_compare_upstream(&clone1_dir.into(), "master")
				.unwrap()
				.behind,
			1
		);

		let res = merge_upstream_rebase(&clone1_dir.into(), "master");
		assert!(res.is_err());

		let state =
			crate::sync::repo_state(&clone1_dir.into()).unwrap();

		assert_eq!(state, RepoState::Clean);

		let commits = get_commit_msgs(&clone1);
		assert_eq!(
			commits,
			vec![String::from("commit3"), String::from("commit1")]
		);
	}

	// sets up a bare origin plus a clone that sits one commit ahead of
	// and one commit behind its upstream (so a rebase actually has a
	// local commit to replay), and returns the clone dir + repo.
	// `upstream_file` is the file the behind-by-one upstream commit adds.
	fn setup_diverged_clone(
		upstream_file: &str,
	) -> (tempfile::TempDir, tempfile::TempDir, Repository) {
		let (r1_dir, _repo) = repo_init_bare().unwrap();
		let origin = r1_dir.path().to_str().unwrap();

		let (clone1_dir, clone1) = repo_clone(origin).unwrap();
		write_commit_file(&clone1, "test.txt", "base", "commit1");
		push_branch(
			&clone1_dir.path().to_str().unwrap().into(),
			"origin",
			"master",
			false,
			false,
			None,
			None,
		)
		.unwrap();

		let (clone2_dir, clone2) = repo_clone(origin).unwrap();
		write_commit_file(&clone2, upstream_file, "up", "commit2");
		push_branch(
			&clone2_dir.path().to_str().unwrap().into(),
			"origin",
			"master",
			false,
			false,
			None,
			None,
		)
		.unwrap();

		// local commit that isn't pushed yet: this is what gets rebased
		write_commit_file(&clone1, "local.txt", "local", "commit3");

		let clone1_path = clone1_dir.path().to_str().unwrap();
		fetch(&clone1_path.into(), "master", None, None).unwrap();
		let cmp =
			branch_compare_upstream(&clone1_path.into(), "master")
				.unwrap();
		assert_eq!(cmp.behind, 1);
		assert_eq!(cmp.ahead, 1);

		(clone1_dir, clone2_dir, clone1)
	}

	#[test]
	fn test_autostash_restores_dirty_tree() {
		let (clone1_dir, _clone2_dir, clone1) =
			setup_diverged_clone("upstream.txt");
		let clone1_path = clone1_dir.path().to_str().unwrap();

		clone1
			.config()
			.unwrap()
			.set_bool("rebase.autoStash", true)
			.unwrap();

		// leave an uncommitted change on a tracked file
		let dirty = clone1.workdir().unwrap().join("test.txt");
		fs::write(&dirty, "dirty").unwrap();

		merge_upstream_rebase(&clone1_path.into(), "master").unwrap();

		// upstream commit got rebased in and the tree is clean again
		assert_eq!(
			crate::sync::repo_state(&clone1_path.into()).unwrap(),
			RepoState::Clean
		);
		assert_eq!(
			get_commit_msgs(&clone1),
			vec![
				String::from("commit3"),
				String::from("commit2"),
				String::from("commit1")
			]
		);

		// the dirty change was popped back and no stash entry lingers
		assert_eq!(fs::read_to_string(&dirty).unwrap(), "dirty");
		assert!(get_stashes(&clone1_path.into()).unwrap().is_empty());
	}

	#[test]
	fn test_autostash_noop_on_clean_tree() {
		let (clone1_dir, _clone2_dir, clone1) =
			setup_diverged_clone("upstream.txt");
		let clone1_path = clone1_dir.path().to_str().unwrap();

		clone1
			.config()
			.unwrap()
			.set_bool("rebase.autoStash", true)
			.unwrap();

		// tree is clean, so nothing should be stashed
		merge_upstream_rebase(&clone1_path.into(), "master").unwrap();

		assert_eq!(
			crate::sync::repo_state(&clone1_path.into()).unwrap(),
			RepoState::Clean
		);
		assert_eq!(
			get_commit_msgs(&clone1),
			vec![
				String::from("commit3"),
				String::from("commit2"),
				String::from("commit1")
			]
		);
		assert!(get_stashes(&clone1_path.into()).unwrap().is_empty());
	}
}
