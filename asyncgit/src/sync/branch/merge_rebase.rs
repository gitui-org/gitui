//! merging from upstream (rebase)

use crate::{
	error::{Error, Result},
	sync::{
		rebase::conflict_free_rebase, repository::repo, CommitId,
		RepoPath,
	},
};
use git2::BranchType;
use scopetime::scope_time;

/// tries merging current branch with its upstream using rebase
pub fn merge_upstream_rebase(
	repo_path: &RepoPath,
	branch_name: &str,
) -> Result<CommitId> {
	scope_time!("merge_upstream_rebase");

	let repo = repo(repo_path)?;
	if super::get_branch_name_repo(&repo)? != branch_name {
		return Err(Error::Generic(String::from(
			"can only rebase in head branch",
		)));
	}

	let branch = repo.find_branch(branch_name, BranchType::Local)?;
	let upstream = branch.upstream()?;
	let upstream_commit = upstream.get().peel_to_commit()?;
	let annotated_upstream =
		repo.find_annotated_commit(upstream_commit.id())?;

	conflict_free_rebase(&repo, &annotated_upstream)
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::sync::{
		branch_compare_upstream, get_commits_info,
		remotes::{fetch, push::push_branch},
		tests::{
			debug_cmd_print, get_commit_ids, repo_clone,
			repo_init_bare, write_commit_file, write_commit_file_at,
		},
		RepoState,
	};
	use git2::{Repository, Time};

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
}
