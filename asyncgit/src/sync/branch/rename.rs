//! renaming of branches

use crate::{
	error::Result,
	sync::{repository::repo, RepoPath},
};
use scopetime::scope_time;

/// Rename the branch reference
pub fn rename_branch(
	repo_path: &RepoPath,
	branch_ref: &str,
	new_name: &str,
) -> Result<()> {
	scope_time!("rename_branch");

	let repo = repo(repo_path)?;
	let branch_as_ref = repo.find_reference(branch_ref)?;
	let mut branch = git2::Branch::wrap(branch_as_ref);
	branch.rename(new_name, true)?;

	Ok(())
}

#[cfg(test)]
mod test {
	use super::super::{checkout_branch, create_branch, RepoPath};
	use super::rename_branch;
	use crate::sync::tests::repo_init;

	#[test]
	fn test_rename_branch() {
		let (_td, repo) = repo_init().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();

		create_branch(repo_path, "branch1").unwrap();

		checkout_branch(repo_path, "branch1").unwrap();

		assert_eq!(
			repo.branches(None)
				.unwrap()
				.next()
				.unwrap()
				.unwrap()
				.0
				.name()
				.unwrap()
				.unwrap(),
			"branch1"
		);

		rename_branch(repo_path, "refs/heads/branch1", "AnotherName")
			.unwrap();

		assert_eq!(
			repo.branches(None)
				.unwrap()
				.next()
				.unwrap()
				.unwrap()
				.0
				.name()
				.unwrap()
				.unwrap(),
			"AnotherName"
		);
	}
}
