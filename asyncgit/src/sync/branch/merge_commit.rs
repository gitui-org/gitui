//! merging from upstream

use super::BranchType;
use crate::{
	error::{Error, Result},
	sync::{merge_msg, repository::repo, CommitId, RepoPath},
};
use git2::Commit;
use scopetime::scope_time;

/// merge upstream using a merge commit if we did not create conflicts.
/// if we did not create conflicts we create a merge commit and return the commit id.
/// Otherwise we return `None`
pub fn merge_upstream_commit(
	repo_path: &RepoPath,
	branch_name: &str,
) -> Result<Option<CommitId>> {
	scope_time!("merge_upstream_commit");

	let repo = repo(repo_path)?;

	let branch = repo.find_branch(branch_name, BranchType::Local)?;
	let upstream = branch.upstream()?;

	let upstream_commit = upstream.get().peel_to_commit()?;

	let annotated_upstream = repo
		.reference_to_annotated_commit(&upstream.into_reference())?;

	let (analysis, pref) =
		repo.merge_analysis(&[&annotated_upstream])?;

	if !analysis.is_normal() {
		return Err(Error::Generic(
			"normal merge not possible".into(),
		));
	}

	if analysis.is_fast_forward() && pref.is_fastforward_only() {
		return Err(Error::Generic(
			"ff merge would be possible".into(),
		));
	}

	//TODO: support merge on unborn?
	if analysis.is_unborn() {
		return Err(Error::Generic("head is unborn".into()));
	}

	repo.merge(&[&annotated_upstream], None, None)?;

	if !repo.index()?.has_conflicts() {
		let msg = merge_msg(repo_path)?;

		let commit_id =
			commit_merge_with_head(&repo, &[upstream_commit], &msg)?;

		return Ok(Some(commit_id));
	}

	Ok(None)
}

pub(crate) fn commit_merge_with_head(
	repo: &git2::Repository,
	commits: &[Commit],
	msg: &str,
) -> Result<CommitId> {
	let signature =
		crate::sync::commit::signature_allow_undefined_name(repo)?;
	let mut index = repo.index()?;
	let tree_id = index.write_tree()?;
	let tree = repo.find_tree(tree_id)?;
	let head_commit = repo.find_commit(
		crate::sync::utils::get_head_repo(repo)?.into(),
	)?;

	let mut parents = vec![&head_commit];
	parents.extend(commits);

	let commit_id = repo
		.commit(
			Some("HEAD"),
			&signature,
			&signature,
			msg,
			&tree,
			parents.as_slice(),
		)?
		.into();
	repo.cleanup_state()?;
	Ok(commit_id)
}

#[cfg(test)]
mod test {
	use git2::Time;

	use super::*;
	use crate::sync::{
		branch_compare_upstream,
		remotes::{fetch, push::push_branch},
		tests::{
			debug_cmd_print, get_commit_ids, repo_clone, repo_init,
			repo_init_bare, write_commit_file, write_commit_file_at,
		},
		RepoState,
	};

	#[test]
	fn test_merge_normal() {
		let (r1_dir, _repo) = repo_init_bare().unwrap();

		let (clone1_dir, clone1) =
			repo_clone(r1_dir.path().to_str().unwrap()).unwrap();

		let (clone2_dir, clone2) =
			repo_clone(r1_dir.path().to_str().unwrap()).unwrap();

		let clone2_dir = clone2_dir.path().to_str().unwrap();

		// clone1

		let commit1 = write_commit_file_at(
			&clone1,
			"test.txt",
			"test",
			"commit1",
			Time::new(1, 0),
		);

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

		let commit2 = write_commit_file_at(
			&clone2,
			"test2.txt",
			"test",
			"commit2",
			Time::new(2, 0),
		);

		//push should fail since origin diverged
		assert!(push_branch(
			&clone2_dir.into(),
			"origin",
			"master",
			false,
			false,
			None,
			None,
		)
		.is_err());

		//lets fetch from origin
		let bytes =
			fetch(&clone2_dir.into(), "master", None, None).unwrap();
		assert!(bytes > 0);

		//we should be one commit behind
		assert_eq!(
			branch_compare_upstream(&clone2_dir.into(), "master")
				.unwrap()
				.behind,
			1
		);

		let merge_commit =
			merge_upstream_commit(&clone2_dir.into(), "master")
				.unwrap()
				.unwrap();

		let state =
			crate::sync::repo_state(&clone2_dir.into()).unwrap();
		assert_eq!(state, RepoState::Clean);

		assert!(!clone2.head_detached().unwrap());

		let commits = get_commit_ids(&clone2, 10);
		assert_eq!(commits.len(), 3);
		assert_eq!(commits[0], merge_commit);
		assert_eq!(commits[1], commit2);
		assert_eq!(commits[2], commit1);

		//verify commit msg
		let details = crate::sync::get_commit_details(
			&clone2_dir.into(),
			merge_commit,
		)
		.unwrap();
		assert_eq!(
            details.message.unwrap().combine(),
            String::from("Merge remote-tracking branch 'refs/remotes/origin/master'")
        );
	}

	#[test]
	fn test_merge_normal_non_ff() {
		let (r1_dir, _repo) = repo_init_bare().unwrap();

		let (clone1_dir, clone1) =
			repo_clone(r1_dir.path().to_str().unwrap()).unwrap();

		let (clone2_dir, clone2) =
			repo_clone(r1_dir.path().to_str().unwrap()).unwrap();

		// clone1

		write_commit_file(
			&clone1,
			"test.bin",
			"test\nfooo",
			"commit1",
		);

		debug_cmd_print(
			&clone2_dir.path().to_str().unwrap().into(),
			"git status",
		);

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

		write_commit_file(
			&clone2,
			"test.bin",
			"foobar\ntest",
			"commit2",
		);

		let bytes = fetch(
			&clone2_dir.path().to_str().unwrap().into(),
			"master",
			None,
			None,
		)
		.unwrap();
		assert!(bytes > 0);

		let res = merge_upstream_commit(
			&clone2_dir.path().to_str().unwrap().into(),
			"master",
		)
		.unwrap();

		//this should not have committed cause we left conflicts behind
		assert_eq!(res, None);

		let state = crate::sync::repo_state(
			&clone2_dir.path().to_str().unwrap().into(),
		)
		.unwrap();

		//validate the repo is in a merge state now
		assert_eq!(state, RepoState::Merge);

		//check that we still only have the first commit
		let commits = get_commit_ids(&clone1, 10);
		assert_eq!(commits.len(), 1);
	}

	/// Verify that the walker preserves topology order. This means that
	/// no parent is visited before any of its children.
	#[test]
	fn test_topology_order() {
		/* Test case
			a diamon shaped graph, where the common ancestor is younger than
			one of its children.

			GP--P1--M
			  \    /
			   --P2

		*/

		let (_repo_dir, repo) = repo_init().unwrap();

		// 1. Grandparent (GP) - Newest
		let gp = write_commit_file_at(
			&repo,
			"gp.txt",
			"gp",
			"gp",
			Time::new(1000, 0),
		);

		// 2. Parent 1 (P1) - Older
		let p1 = write_commit_file_at(
			&repo,
			"p1.txt",
			"p1",
			"p1",
			Time::new(500, 0),
		);

		// 3. Parent 2 (P2) - Older (diverging from GP)
		// Reset HEAD to GP so P2 becomes a child of GP
		repo.reset(
			repo.find_object(gp.into(), None)
				.unwrap()
				.as_commit()
				.unwrap()
				.as_object(),
			git2::ResetType::Hard,
			None,
		)
		.unwrap();
		let p2 = write_commit_file_at(
			&repo,
			"p2.txt",
			"p2",
			"p2",
			Time::new(400, 0),
		);

		// 4. Merge commit (M) - The starting point of our walk
		// The heap now contains [p1, p2].
		// If we pop p1, we add gp. The heap is [gp, p2].
		// Because gp(1000) > p2(400), the walker returns gp before p2.
		// This is a violation: p2 is a child of gp and must be visited first.
		let p1_commit = repo.find_commit(p1.into()).unwrap();
		let p2_commit = repo.find_commit(p2.into()).unwrap();
		let tree = repo
			.find_tree(repo.index().unwrap().write_tree().unwrap())
			.unwrap();
		let sig = repo.signature().unwrap();
		let m = repo
			.commit(
				Some("HEAD"),
				&sig,
				&sig,
				"Merge p1 into p2",
				&tree,
				&[&p2_commit, &p1_commit],
			)
			.unwrap();
		let m = CommitId::new(m);

		// Expected Topological Order: [M, P1, P2, GP] or [M, P2, P1, GP]
		// Actual Defective Order: [M, P1, GP, P2]
		// (GP jumps ahead of P2 because 1000 > 400)

		let commits = get_commit_ids(&repo, 14);
		for (i, id) in commits.iter().enumerate() {
			println!("DEBUG: commits[{}] = {:?}", i, id);
			// Print the message of the commit to identify it
			let repo_path = &repo.path().to_path_buf().into();
			let details =
				crate::sync::get_commit_details(repo_path, *id)
					.unwrap();
			println!(
				"DEBUG:    Message: {:?}",
				details.message.map(|m| m.combine())
			);
		}
		println!("DEBUG: Expected M is {:?}", m);
		println!("DEBUG:   P1 is {:?}", &p1);
		println!("DEBUG:   P2 is {:?}", &p2);
		println!("DEBUG:   GP is {:?}", &gp);
		assert_eq!(commits[0], m);
		assert!(commits.contains(&p1));
		assert!(commits.contains(&p2));
		assert_eq!(
			commits[3], gp,
			"Violation: Grandparent must be the last commit"
		);
	}
}
