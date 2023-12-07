use super::{repository::repo, RepoPath};
use crate::error::Result;
use scopetime::scope_time;

pub use git2_hooks::HookResult;

/// this hook is documented here <https://git-scm.com/docs/githooks#_commit_msg>
/// we use the same convention as other git clients to create a temp file containing
/// the commit message at `<.git|hooksPath>/COMMIT_EDITMSG` and pass it's relative path as the only
/// parameter to the hook script.
pub fn hooks_commit_msg(
	repo_path: &RepoPath,
	msg: &mut String,
) -> Result<HookResult> {
	scope_time!("hooks_commit_msg");

	let repo = repo(repo_path)?;

	Ok(git2_hooks::hooks_commit_msg(&repo, msg)?)
}

/// this hook is documented here <https://git-scm.com/docs/githooks#_pre_commit>
///
pub fn hooks_pre_commit(
	repo_path: &RepoPath,
) -> Result<git2_hooks::HookResult> {
	scope_time!("hooks_pre_commit");

	let repo = repo(repo_path)?;

	Ok(git2_hooks::hooks_pre_commit(&repo)?)
}

///
pub fn hooks_post_commit(
	repo_path: &RepoPath,
) -> Result<git2_hooks::HookResult> {
	scope_time!("hooks_post_commit");

	let repo = repo(repo_path)?;

	Ok(git2_hooks::hooks_post_commit(&repo)?)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::sync::tests::repo_init;

	#[test]
	fn test_post_commit_hook_reject_in_subfolder() {
		let (_td, repo) = repo_init().unwrap();
		let root = repo.path().parent().unwrap();

		let hook = b"#!/bin/sh
	echo 'rejected'
	exit 1
	        ";

		git2_hooks::create_hook(
			&repo,
			git2_hooks::HOOK_POST_COMMIT,
			hook,
		);

		let subfolder = root.join("foo/");
		std::fs::create_dir_all(&subfolder).unwrap();

		let res =
			hooks_post_commit(&subfolder.to_str().unwrap().into())
				.unwrap();

		assert_eq!(
			res,
			HookResult::NotOk(String::from("rejected\n"))
		);
	}

	// make sure we run the hooks with the correct pwd.
	// for non-bare repos this is the dir of the worktree
	// unfortunately does not work on windows
	#[test]
	#[cfg(unix)]
	fn test_pre_commit_workdir() {
		let (_td, repo) = repo_init().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();
		let workdir =
			crate::sync::utils::repo_work_dir(repo_path).unwrap();

		let hook = b"#!/bin/sh
	echo $(pwd)
	exit 1
	        ";

		git2_hooks::create_hook(
			&repo,
			git2_hooks::HOOK_PRE_COMMIT,
			hook,
		);
		let res = hooks_pre_commit(repo_path).unwrap();
		if let HookResult::NotOk(res) = res {
			assert_eq!(
				std::path::Path::new(res.trim_end()),
				std::path::Path::new(&workdir)
			);
		} else {
			assert!(false);
		}
	}
}
