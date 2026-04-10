use super::{utils::get_head_repo, CommitId, RepoPath};
use crate::{error::Result, sync::repository::repo};
use git2::{build::CheckoutBuilder, ObjectType, ResetType};
use scopetime::scope_time;

///
pub fn reset_stage(repo_path: &RepoPath, path: &str) -> Result<()> {
	scope_time!("reset_stage");

	let repo = repo(repo_path)?;

	if let Ok(id) = get_head_repo(&repo) {
		let obj =
			repo.find_object(id.into(), Some(ObjectType::Commit))?;

		repo.reset_default(Some(&obj), [path])?;
	} else {
		repo.reset_default(None, [path])?;
	}

	Ok(())
}

///
pub fn reset_workdir(repo_path: &RepoPath, path: &str) -> Result<()> {
	scope_time!("reset_workdir");

	let repo = repo(repo_path)?;

	let mut checkout_opts = CheckoutBuilder::new();
	checkout_opts
		.update_index(true) // windows: needs this to be true WTF?!
		.remove_untracked(true)
		.force()
		.path(path);

	repo.checkout_index(None, Some(&mut checkout_opts))?;
	Ok(())
}

///
pub fn reset_repo(
	repo_path: &RepoPath,
	commit: CommitId,
	kind: ResetType,
) -> Result<()> {
	scope_time!("reset_repo");

	let repo = repo(repo_path)?;

	let c = repo.find_commit(commit.into())?;

	repo.reset(c.as_object(), kind, None)?;

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::{reset_stage, reset_workdir};
	use crate::error::Result;
	use crate::sync::{
		commit,
		status::{get_status, StatusType},
		tests::{
			debug_cmd_print, get_statuses, repo_init, repo_init_empty,
		},
		utils::{stage_add_all, stage_add_file},
		RepoPath,
	};
	use std::{
		fs::{self, File},
		io::Write,
		path::Path,
	};

	static HUNK_A: &str = r"
1   start
2
3
4
5
6   middle
7
8
9
0
1   end";

	static HUNK_B: &str = r"
1   start
2   newa
3
4
5
6   middle
7
8
9
0   newb
1   end";

	#[test]
	fn test_reset_only_unstaged() {
		let (_td, repo) = repo_init().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();

		let res = get_status(repo_path, StatusType::WorkingDir, None)
			.unwrap();
		assert_eq!(res.len(), 0);

		let file_path = root.join("bar.txt");

		{
			File::create(&file_path)
				.unwrap()
				.write_all(HUNK_A.as_bytes())
				.unwrap();
		}

		debug_cmd_print(repo_path, "git status");

		stage_add_file(repo_path, Path::new("bar.txt")).unwrap();

		debug_cmd_print(repo_path, "git status");

		// overwrite with next content
		{
			File::create(&file_path)
				.unwrap()
				.write_all(HUNK_B.as_bytes())
				.unwrap();
		}

		debug_cmd_print(repo_path, "git status");

		assert_eq!(get_statuses(repo_path), (1, 1));

		reset_workdir(repo_path, "bar.txt").unwrap();

		debug_cmd_print(repo_path, "git status");

		assert_eq!(get_statuses(repo_path), (0, 1));
	}

	#[test]
	fn test_reset_untracked_in_subdir() {
		let (_td, repo) = repo_init().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();

		{
			fs::create_dir(root.join("foo")).unwrap();
			File::create(root.join("foo/bar.txt"))
				.unwrap()
				.write_all(b"test\nfoo")
				.unwrap();
		}

		debug_cmd_print(repo_path, "git status");

		assert_eq!(get_statuses(repo_path), (1, 0));

		reset_workdir(repo_path, "foo/bar.txt").unwrap();

		debug_cmd_print(repo_path, "git status");

		assert_eq!(get_statuses(repo_path), (0, 0));
	}

	#[test]
	fn test_reset_folder() -> Result<()> {
		let (_td, repo) = repo_init().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();

		{
			fs::create_dir(root.join("foo"))?;
			File::create(root.join("foo/file1.txt"))?
				.write_all(b"file1")?;
			File::create(root.join("foo/file2.txt"))?
				.write_all(b"file1")?;
			File::create(root.join("file3.txt"))?
				.write_all(b"file3")?;
		}

		stage_add_all(repo_path, "*", None).unwrap();
		commit(repo_path, "msg").unwrap();

		{
			File::create(root.join("foo/file1.txt"))?
				.write_all(b"file1\nadded line")?;
			fs::remove_file(root.join("foo/file2.txt"))?;
			File::create(root.join("foo/file4.txt"))?
				.write_all(b"file4")?;
			File::create(root.join("foo/file5.txt"))?
				.write_all(b"file5")?;
			File::create(root.join("file3.txt"))?
				.write_all(b"file3\nadded line")?;
		}

		assert_eq!(get_statuses(repo_path), (5, 0));

		stage_add_file(repo_path, Path::new("foo/file5.txt"))
			.unwrap();

		assert_eq!(get_statuses(repo_path), (4, 1));

		reset_workdir(repo_path, "foo").unwrap();

		assert_eq!(get_statuses(repo_path), (1, 1));

		Ok(())
	}

	#[test]
	fn test_reset_untracked_in_subdir_and_index() {
		let (_td, repo) = repo_init().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();
		let file = "foo/bar.txt";

		{
			fs::create_dir(root.join("foo")).unwrap();
			File::create(root.join(file))
				.unwrap()
				.write_all(b"test\nfoo")
				.unwrap();
		}

		debug_cmd_print(repo_path, "git status");

		debug_cmd_print(repo_path, "git add .");

		debug_cmd_print(repo_path, "git status");

		{
			File::create(root.join(file))
				.unwrap()
				.write_all(b"test\nfoo\nnewend")
				.unwrap();
		}

		debug_cmd_print(repo_path, "git status");

		assert_eq!(get_statuses(repo_path), (1, 1));

		reset_workdir(repo_path, file).unwrap();

		debug_cmd_print(repo_path, "git status");

		assert_eq!(get_statuses(repo_path), (0, 1));
	}

	#[test]
	fn unstage_in_empty_repo() {
		let file_path = Path::new("foo.txt");
		let (_td, repo) = repo_init_empty().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();

		File::create(root.join(file_path))
			.unwrap()
			.write_all(b"test\nfoo")
			.unwrap();

		assert_eq!(get_statuses(repo_path), (1, 0));

		stage_add_file(repo_path, file_path).unwrap();

		assert_eq!(get_statuses(repo_path), (0, 1));

		reset_stage(repo_path, file_path.to_str().unwrap()).unwrap();

		assert_eq!(get_statuses(repo_path), (1, 0));
	}

	#[test]
	fn test_reset_untracked_in_subdir_with_cwd_in_subdir() {
		let (_td, repo) = repo_init().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();

		{
			fs::create_dir(root.join("foo")).unwrap();
			File::create(root.join("foo/bar.txt"))
				.unwrap()
				.write_all(b"test\nfoo")
				.unwrap();
		}

		debug_cmd_print(repo_path, "git status");

		assert_eq!(get_statuses(repo_path), (1, 0));

		reset_workdir(
			&root.join("foo").as_os_str().to_str().unwrap().into(),
			"foo/bar.txt",
		)
		.unwrap();

		debug_cmd_print(repo_path, "git status");

		assert_eq!(get_statuses(repo_path), (0, 0));
	}

	#[test]
	fn test_reset_untracked_subdir() {
		let (_td, repo) = repo_init().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();

		{
			fs::create_dir_all(root.join("foo/bar")).unwrap();
			File::create(root.join("foo/bar/baz.txt"))
				.unwrap()
				.write_all(b"test\nfoo")
				.unwrap();
		}

		debug_cmd_print(repo_path, "git status");

		assert_eq!(get_statuses(repo_path), (1, 0));

		reset_workdir(repo_path, "foo/bar").unwrap();

		debug_cmd_print(repo_path, "git status");

		assert_eq!(get_statuses(repo_path), (0, 0));
	}
}
