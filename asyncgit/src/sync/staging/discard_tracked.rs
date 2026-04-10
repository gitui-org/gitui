use super::{apply_selection, load_file};
use crate::{
	error::Result,
	sync::{
		diff::DiffLinePosition, patches::get_file_diff_patch,
		patches::patch_get_hunklines, repository::repo,
		utils::repo_write_file, RepoPath,
	},
};
use scopetime::scope_time;

/// discards specific lines in an unstaged hunk of a diff
pub fn discard_lines(
	repo_path: &RepoPath,
	file_path: &str,
	lines: &[DiffLinePosition],
) -> Result<()> {
	scope_time!("discard_lines");

	if lines.is_empty() {
		return Ok(());
	}

	let repo = repo(repo_path)?;
	repo.index()?.read(true)?;

	//TODO: check that file is not new (status modified)

	let new_content = {
		let patch =
			get_file_diff_patch(&repo, file_path, false, false)?;
		let hunks = patch_get_hunklines(&patch)?;

		let working_content = load_file(&repo, file_path)?;
		let old_lines = working_content.lines().collect::<Vec<_>>();

		apply_selection(lines, &hunks, &old_lines, false, true)?
	};

	repo_write_file(&repo, file_path, new_content.as_str())?;

	Ok(())
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::sync::tests::{repo_init, write_commit_file};

	#[test]
	fn test_discard() {
		static FILE_1: &str = r"0
1
2
3
4
";

		static FILE_2: &str = r"0


3
4
";

		static FILE_3: &str = r"0
2

3
4
";

		let (path, repo) = repo_init().unwrap();
		let path: &RepoPath = &path.path().to_str().unwrap().into();

		write_commit_file(&repo, "test.txt", FILE_1, "c1");

		repo_write_file(&repo, "test.txt", FILE_2).unwrap();

		discard_lines(
			path,
			"test.txt",
			&[
				DiffLinePosition {
					old_lineno: Some(3),
					new_lineno: None,
				},
				DiffLinePosition {
					old_lineno: None,
					new_lineno: Some(2),
				},
			],
		)
		.unwrap();

		let result_file = load_file(&repo, "test.txt").unwrap();

		assert_eq!(result_file.as_str(), FILE_3);
	}

	#[test]
	fn test_discard2() {
		static FILE_1: &str = r"start
end
";

		static FILE_2: &str = r"start
1
2
end
";

		static FILE_3: &str = r"start
1
end
";

		let (path, repo) = repo_init().unwrap();
		let path: &RepoPath = &path.path().to_str().unwrap().into();

		write_commit_file(&repo, "test.txt", FILE_1, "c1");

		repo_write_file(&repo, "test.txt", FILE_2).unwrap();

		discard_lines(
			path,
			"test.txt",
			&[DiffLinePosition {
				old_lineno: None,
				new_lineno: Some(3),
			}],
		)
		.unwrap();

		let result_file = load_file(&repo, "test.txt").unwrap();

		assert_eq!(result_file.as_str(), FILE_3);
	}

	#[test]
	fn test_discard3() {
		static FILE_1: &str = r"start
1
end
";

		static FILE_2: &str = r"start
2
end
";

		static FILE_3: &str = r"start
1
end
";

		let (path, repo) = repo_init().unwrap();
		let path: &RepoPath = &path.path().to_str().unwrap().into();

		write_commit_file(&repo, "test.txt", FILE_1, "c1");

		repo_write_file(&repo, "test.txt", FILE_2).unwrap();

		discard_lines(
			path,
			"test.txt",
			&[
				DiffLinePosition {
					old_lineno: Some(2),
					new_lineno: None,
				},
				DiffLinePosition {
					old_lineno: None,
					new_lineno: Some(2),
				},
			],
		)
		.unwrap();

		let result_file = load_file(&repo, "test.txt").unwrap();

		assert_eq!(result_file.as_str(), FILE_3);
	}

	#[test]
	fn test_discard4() {
		static FILE_1: &str = r"start
mid
end
";

		static FILE_2: &str = r"start
1
mid
2
end
";

		static FILE_3: &str = r"start
mid
end
";

		let (path, repo) = repo_init().unwrap();
		let path: &RepoPath = &path.path().to_str().unwrap().into();

		write_commit_file(&repo, "test.txt", FILE_1, "c1");

		repo_write_file(&repo, "test.txt", FILE_2).unwrap();

		discard_lines(
			path,
			"test.txt",
			&[
				DiffLinePosition {
					old_lineno: None,
					new_lineno: Some(2),
				},
				DiffLinePosition {
					old_lineno: None,
					new_lineno: Some(4),
				},
			],
		)
		.unwrap();

		let result_file = load_file(&repo, "test.txt").unwrap();

		assert_eq!(result_file.as_str(), FILE_3);
	}

	#[test]
	fn test_discard_if_first_selected_line_is_not_in_any_hunk() {
		static FILE_1: &str = r"start
end
";

		static FILE_2: &str = r"start
1
end
";

		static FILE_3: &str = r"start
end
";

		let (path, repo) = repo_init().unwrap();
		let path: &RepoPath = &path.path().to_str().unwrap().into();

		write_commit_file(&repo, "test.txt", FILE_1, "c1");

		repo_write_file(&repo, "test.txt", FILE_2).unwrap();

		discard_lines(
			path,
			"test.txt",
			&[
				DiffLinePosition {
					old_lineno: None,
					new_lineno: Some(1),
				},
				DiffLinePosition {
					old_lineno: None,
					new_lineno: Some(2),
				},
			],
		)
		.unwrap();

		let result_file = load_file(&repo, "test.txt").unwrap();

		assert_eq!(result_file.as_str(), FILE_3);
	}

	//this test shows that we require at least a diff context around add/removes of 1
	#[test]
	fn test_discard_deletions_filestart_breaking_with_zero_context() {
		static FILE_1: &str = r"start
mid
end
";

		static FILE_2: &str = r"start
end
";

		static FILE_3: &str = r"start
mid
end
";

		let (path, repo) = repo_init().unwrap();
		let path: &RepoPath = &path.path().to_str().unwrap().into();

		write_commit_file(&repo, "test.txt", FILE_1, "c1");

		repo_write_file(&repo, "test.txt", FILE_2).unwrap();

		discard_lines(
			path,
			"test.txt",
			&[DiffLinePosition {
				old_lineno: Some(2),
				new_lineno: None,
			}],
		)
		.unwrap();

		let result_file = load_file(&repo, "test.txt").unwrap();

		assert_eq!(result_file.as_str(), FILE_3);
	}

	#[test]
	fn test_discard5() {
		static FILE_1: &str = r"start
";

		static FILE_2: &str = r"start
1";

		static FILE_3: &str = r"start
";

		let (path, repo) = repo_init().unwrap();
		let path: &RepoPath = &path.path().to_str().unwrap().into();

		write_commit_file(&repo, "test.txt", FILE_1, "c1");

		repo_write_file(&repo, "test.txt", FILE_2).unwrap();

		discard_lines(
			path,
			"test.txt",
			&[DiffLinePosition {
				old_lineno: None,
				new_lineno: Some(2),
			}],
		)
		.unwrap();

		let result_file = load_file(&repo, "test.txt").unwrap();

		assert_eq!(result_file.as_str(), FILE_3);
	}
}
