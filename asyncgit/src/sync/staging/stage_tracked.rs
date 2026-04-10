use super::apply_selection;
use crate::{
	error::{Error, Result},
	sync::{
		diff::DiffLinePosition, patches::get_file_diff_patch,
		patches::patch_get_hunklines, repository::repo, RepoPath,
	},
};
use easy_cast::Conv;
use scopetime::scope_time;
use std::path::Path;

///
pub fn stage_lines(
	repo_path: &RepoPath,
	file_path: &str,
	is_stage: bool,
	lines: &[DiffLinePosition],
) -> Result<()> {
	scope_time!("stage_lines");

	if lines.is_empty() {
		return Ok(());
	}

	let repo = repo(repo_path)?;
	// log::debug!("stage_lines: {:?}", lines);

	let mut index = repo.index()?;
	index.read(true)?;
	let mut idx =
		index.get_path(Path::new(file_path), 0).ok_or_else(|| {
			Error::Generic(String::from(
				"only non new files supported",
			))
		})?;
	let blob = repo.find_blob(idx.id)?;
	let indexed_content = String::from_utf8(blob.content().into())?;

	let new_content = {
		let patch =
			get_file_diff_patch(&repo, file_path, is_stage, false)?;
		let hunks = patch_get_hunklines(&patch)?;

		let old_lines = indexed_content.lines().collect::<Vec<_>>();

		apply_selection(lines, &hunks, &old_lines, is_stage, false)?
	};

	let blob_id = repo.blob(new_content.as_bytes())?;

	idx.id = blob_id;
	idx.file_size = u32::try_conv(new_content.len())?;
	index.add(&idx)?;

	index.write()?;
	index.read(true)?;

	Ok(())
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::sync::{
		diff::get_diff,
		tests::{get_statuses, repo_init, write_commit_file},
		utils::{repo_write_file, stage_add_file},
	};

	#[test]
	fn test_stage() {
		static FILE_1: &str = r"0
";

		static FILE_2: &str = r"0
1
2
3
";

		let (path, repo) = repo_init().unwrap();
		let path: &RepoPath = &path.path().to_str().unwrap().into();

		write_commit_file(&repo, "test.txt", FILE_1, "c1");

		repo_write_file(&repo, "test.txt", FILE_2).unwrap();

		stage_lines(
			path,
			"test.txt",
			false,
			&[DiffLinePosition {
				old_lineno: None,
				new_lineno: Some(2),
			}],
		)
		.unwrap();

		let diff = get_diff(path, "test.txt", true, None).unwrap();

		assert_eq!(diff.lines, 3);
		assert_eq!(&*diff.hunks[0].lines[0].content, "@@ -1 +1,2 @@");
	}

	#[test]
	fn test_panic_stage_no_newline() {
		static FILE_1: &str = r"a = 1
b = 2";

		static FILE_2: &str = r"a = 2
b = 3
c = 4";

		let (path, repo) = repo_init().unwrap();
		let path: &RepoPath = &path.path().to_str().unwrap().into();

		write_commit_file(&repo, "test.txt", FILE_1, "c1");

		repo_write_file(&repo, "test.txt", FILE_2).unwrap();

		stage_lines(
			path,
			"test.txt",
			false,
			&[
				DiffLinePosition {
					old_lineno: Some(1),
					new_lineno: None,
				},
				DiffLinePosition {
					old_lineno: Some(2),
					new_lineno: None,
				},
			],
		)
		.unwrap();

		let diff = get_diff(path, "test.txt", true, None).unwrap();

		assert_eq!(diff.lines, 5);
		assert_eq!(&*diff.hunks[0].lines[0].content, "@@ -1,2 +1 @@");
	}

	#[test]
	fn test_unstage() {
		static FILE_1: &str = r"0
";

		static FILE_2: &str = r"0
1
2
3
";

		let (path, repo) = repo_init().unwrap();
		let path: &RepoPath = &path.path().to_str().unwrap().into();

		write_commit_file(&repo, "test.txt", FILE_1, "c1");

		repo_write_file(&repo, "test.txt", FILE_2).unwrap();

		assert_eq!(get_statuses(path), (1, 0));

		stage_add_file(path, Path::new("test.txt")).unwrap();

		assert_eq!(get_statuses(path), (0, 1));

		let diff_before =
			get_diff(path, "test.txt", true, None).unwrap();

		assert_eq!(diff_before.lines, 5);

		stage_lines(
			path,
			"test.txt",
			true,
			&[DiffLinePosition {
				old_lineno: None,
				new_lineno: Some(2),
			}],
		)
		.unwrap();

		assert_eq!(get_statuses(path), (1, 1));

		let diff = get_diff(path, "test.txt", true, None).unwrap();

		assert_eq!(diff.lines, 4);
	}
}
