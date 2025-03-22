//! merging from upstream

use super::BranchType;
use crate::{
	error::{Error, Result},
	sync::{repository::repo, RepoPath},
};
use scopetime::scope_time;

///
pub fn branch_merge_upstream_fastforward(
	repo_path: &RepoPath,
	branch: &str,
) -> Result<()> {
	scope_time!("branch_merge_upstream");

	let repo = repo(repo_path)?;

	let branch = repo.find_branch(branch, BranchType::Local)?;
	let upstream = branch.upstream()?;

	let upstream_commit =
		upstream.into_reference().peel_to_commit()?;

	let annotated =
		repo.find_annotated_commit(upstream_commit.id())?;

	let (analysis, pref) = repo.merge_analysis(&[&annotated])?;

	if !analysis.is_fast_forward() {
		return Err(Error::Generic(
			"fast forward merge not possible".into(),
		));
	}

	if pref.is_no_fast_forward() {
		return Err(Error::Generic("fast forward not wanted".into()));
	}

	//TODO: support merge on unborn
	if analysis.is_unborn() {
		return Err(Error::Generic("head is unborn".into()));
	}

	repo.checkout_tree(upstream_commit.as_object(), None)?;

	repo.head()?.set_target(annotated.id(), "")?;

	Ok(())
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::sync::{
		remotes::{fetch, push::push_branch},
		tests::{
			debug_cmd_print, get_commit_ids, repo_clone,
			repo_init_bare, write_commit_file,
		},
	};

	#[test]
	fn test_merge_fastforward() {
		let (r1_dir, _repo) = repo_init_bare().unwrap();

		let (clone1_dir, clone1) =
			repo_clone(r1_dir.path().to_str().unwrap()).unwrap();

		let (clone2_dir, clone2) =
			repo_clone(r1_dir.path().to_str().unwrap()).unwrap();

		// clone1

		let commit1 =
			write_commit_file(&clone1, "test.txt", "test", "commit1");

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

		// clone2
		debug_cmd_print(
			&clone2_dir.path().to_str().unwrap().into(),
			"git pull --ff",
		);

		let commit2 = write_commit_file(
			&clone2,
			"test2.txt",
			"test",
			"commit2",
		);

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

		// clone1 again

		let bytes = fetch(
			&clone1_dir.path().to_str().unwrap().into(),
			"master",
			None,
			None,
		)
		.unwrap();
		assert!(bytes > 0);

		let bytes = fetch(
			&clone1_dir.path().to_str().unwrap().into(),
			"master",
			None,
			None,
		)
		.unwrap();
		assert_eq!(bytes, 0);

		branch_merge_upstream_fastforward(
			&clone1_dir.path().to_str().unwrap().into(),
			"master",
		)
		.unwrap();

		let commits = get_commit_ids(&clone1, 10);
		assert_eq!(commits.len(), 2);
		assert_eq!(commits[1], commit1);
		assert_eq!(commits[0], commit2);
	}
}
