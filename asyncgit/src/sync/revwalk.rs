//! git revwalk utils
use super::{repo, CommitId, RepoPath};
use crate::Result;
use git2::Oid;
use std::ops::ControlFlow;

/// Checks if `commits` range is topologically continuous
///
/// Supports only linear paths - presence of merge commits results in `false`.
pub fn is_continuous(
	repo_path: &RepoPath,
	commits: &[CommitId],
) -> Result<bool> {
	match commits {
		[] | [_] => Ok(true),
		commits => {
			let repo = repo(repo_path)?;
			let mut revwalk = repo.revwalk()?;
			revwalk.set_sorting(git2::Sort::TOPOLOGICAL)?;
			revwalk.push(commits[0].get_oid())?;
			let revwalked: Vec<Oid> =
				revwalk
					.take(commits.len())
					.collect::<std::result::Result<Vec<_>, _>>()?;

			if revwalked.len() != commits.len() {
				return Ok(false);
			}

			match revwalked.iter().zip(commits).try_fold(
				Ok(true),
				|acc, (r, c)| match acc
					.map(|acc| acc && (&(CommitId::from(*r)) == c))
				{
					ok @ Ok(true) => ControlFlow::Continue(ok),
					otherwise => ControlFlow::Break(otherwise),
				},
			) {
				ControlFlow::Continue(v) | ControlFlow::Break(v) => v,
			}
		}
	}
}
#[cfg(test)]
mod tests_is_continuous {
	use crate::sync::{
		checkout_commit, commit, merge_commit,
		revwalk::is_continuous, tests::repo_init_empty, RepoPath,
	};

	#[test]
	fn test_linear_commits_are_continuous() {
		// * c3 (HEAD)
		// * c2
		// * c1
		// * c0 (root)

		let (_td, repo) = repo_init_empty().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();
		let _c0 = commit(repo_path, "commit 0").unwrap();
		let c1 = commit(repo_path, "commit 1").unwrap();
		let c2 = commit(repo_path, "commit 2").unwrap();
		let c3 = commit(repo_path, "commit 3").unwrap();

		let result = is_continuous(repo_path, &[c3, c2, c1]).unwrap();
		assert!(result, "linear commits should be continuous");

		let result = is_continuous(repo_path, &[c2]).unwrap();
		assert!(result, "single commit should be continuous");

		let result = is_continuous(repo_path, &[]).unwrap();
		assert!(result, "empty range should be continuous");
	}

	#[test]
	fn test_merge_commits_break_continuity() {
		// *   c4 (HEAD)
		// |\
		// | * c3
		// * | c2
		// |/
		// * c1
		// * c0 (root)

		let (_td, repo) = repo_init_empty().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();

		let c0 = commit(repo_path, "commit 0").unwrap();
		let c1 = commit(repo_path, "commit 1").unwrap();
		let c2 = commit(repo_path, "commit 2").unwrap();

		checkout_commit(repo_path, c1).unwrap();
		let c3 = commit(repo_path, "commit 3").unwrap();

		let c4 =
			merge_commit(repo_path, "commit 4", &[c2, c3]).unwrap();

		let result = is_continuous(repo_path, &[c4, c2, c1]).unwrap();
		assert!(
			!result,
			"range including merge should not be continuous"
		);

		let result = is_continuous(repo_path, &[c4, c3, c1]).unwrap();
		assert!(
			!result,
			"range including merge should not be continuous (following second parent commit)"
		);

		let result = is_continuous(repo_path, &[c2, c1, c0]).unwrap();
		assert!(
			result,
			"linear range before merge should be continuous"
		);
	}

	#[test]
	fn test_non_continuous_commits() {
		// * c4 (HEAD)
		// * c3
		// * c2
		// * c1
		// * c0 (root)

		let (_td, repo) = repo_init_empty().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();

		let _c0 = commit(repo_path, "commit 0").unwrap();
		let c1 = commit(repo_path, "commit 1").unwrap();
		let c2 = commit(repo_path, "commit 2").unwrap();
		let c3 = commit(repo_path, "commit 3").unwrap();
		let c4 = commit(repo_path, "commit 4").unwrap();

		let result = is_continuous(repo_path, &[c4, c3, c1]).unwrap();
		assert!(!result, "commit range with gap should return false");

		let result = is_continuous(repo_path, &[c1, c2, c3]).unwrap();
		assert!(!result, "wrong order should return false");
	}
}
