//! Sync git API for fetching a file blame

use super::{utils, CommitId, RepoPath};
use crate::{
	error::Result,
	sync::{get_commits_info, gix_repo},
};
use gix::blame::Options;
use scopetime::scope_time;
use std::collections::{HashMap, HashSet};

/// A `BlameHunk` contains all the information that will be shown to the user.
#[derive(Clone, Hash, Debug, PartialEq, Eq)]
pub struct BlameHunk {
	///
	pub commit_id: CommitId,
	///
	pub author: String,
	///
	pub time: i64,
	/// `git2::BlameHunk::final_start_line` returns 1-based indices, but
	/// `start_line` is 0-based because the `Vec` storing the lines starts at
	/// index 0.
	pub start_line: usize,
	///
	pub end_line: usize,
}

/// A `BlameFile` represents a collection of lines. This is targeted at how the
/// data will be used by the UI.
#[derive(Clone, Debug)]
pub struct FileBlame {
	///
	pub commit_id: CommitId,
	///
	pub path: String,
	///
	pub lines: Vec<(Option<BlameHunk>, String)>,
}

///
pub fn blame_file(
	repo_path: &RepoPath,
	file_path: &str,
	commit_id: Option<CommitId>,
) -> Result<FileBlame> {
	scope_time!("blame_file");

	let file_path: &gix::bstr::BStr = file_path.into();
	let file_path =
		gix::path::to_unix_separators_on_windows(file_path);

	let repo: gix::Repository = gix_repo(repo_path)?;
	let tip: gix::ObjectId = match commit_id {
		Some(commit_id) => gix::ObjectId::from_bytes_or_panic(
			commit_id.get_oid().as_bytes(),
		),
		_ => repo.head()?.peel_to_commit()?.id,
	};

	let diff_algorithm = repo.diff_algorithm()?;

	let options: Options = Options {
		diff_algorithm,
		..Options::default()
	};

	// TODO: `blame_file` does not take `diff_algorithm` into account. Instead, it relies on
	// `#[default]` which, as of 2025-10-30, is `Histogram`.
	let outcome = repo.blame_file(&file_path, tip, options)?;

	let commit_id = if let Some(commit_id) = commit_id {
		commit_id
	} else {
		let repo = crate::sync::repo(repo_path)?;

		utils::get_head_repo(&repo)?
	};

	let unique_commit_ids: HashSet<CommitId> = outcome
		.entries
		.iter()
		.map(|entry| entry.commit_id.into())
		.collect();
	let mut commit_ids = Vec::with_capacity(unique_commit_ids.len());
	commit_ids.extend(unique_commit_ids);

	let commit_infos = get_commits_info(repo_path, &commit_ids, 0)?;
	let unique_commit_infos: HashMap<_, _> = commit_infos
		.iter()
		.map(|commit_info| (commit_info.id, commit_info))
		.collect();

	// TODO
	// The shape of data as returned by `entries_with_lines` is preferable to the one chosen here
	// because the former is much closer to what the UI is going to need in the end.
	let lines: Vec<(Option<BlameHunk>, String)> = outcome
		.entries_with_lines()
		.flat_map(|(entry, lines)| {
			let commit_id = entry.commit_id.into();
			let start_in_blamed_file =
				entry.start_in_blamed_file as usize;

			lines
				.iter()
				.enumerate()
				.map(|(i, line)| {
					// TODO
					let trimmed_line =
						line.to_string().trim_end().to_string();

					if let Some(commit_info) =
						unique_commit_infos.get(&commit_id)
					{
						return (
							Some(BlameHunk {
								commit_id,
								author: commit_info.author.clone(),
								time: commit_info.time,
								start_line: start_in_blamed_file + i,
								end_line: start_in_blamed_file
									+ i + 1,
							}),
							trimmed_line,
						);
					}

					(None, trimmed_line)
				})
				.collect::<Vec<_>>()
		})
		.collect();

	let file_blame = FileBlame {
		commit_id,
		path: file_path.to_string(),
		lines,
	};

	Ok(file_blame)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		error::Result,
		sync::{commit, stage_add_file, tests::repo_init_empty},
	};
	use std::{
		fs::{File, OpenOptions},
		io::Write,
		path::Path,
	};

	#[test]
	fn test_blame() -> Result<()> {
		let file_path = Path::new("foo");
		let (_td, repo) = repo_init_empty()?;
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();

		assert!(blame_file(repo_path, "foo", None).is_err());

		File::create(root.join(file_path))?.write_all(b"line 1\n")?;

		stage_add_file(repo_path, file_path)?;
		commit(repo_path, "first commit")?;

		let blame = blame_file(repo_path, "foo", None)?;

		assert!(matches!(
			blame.lines.as_slice(),
			[(
				Some(BlameHunk {
					author,
					start_line: 0,
					end_line: 1,
					..
				}),
				line
			)] if author == "name" && line == "line 1"
		));

		let mut file = OpenOptions::new()
			.append(true)
			.open(root.join(file_path))?;

		file.write(b"line 2\n")?;

		stage_add_file(repo_path, file_path)?;
		commit(repo_path, "second commit")?;

		let blame = blame_file(repo_path, "foo", None)?;

		assert!(matches!(
			blame.lines.as_slice(),
			[
				(
					Some(BlameHunk {
						start_line: 0,
						end_line: 1,
						..
					}),
					first_line
				),
				(
					Some(BlameHunk {
						author,
						start_line: 1,
						end_line: 2,
						..
					}),
					second_line
				)
			] if author == "name" && first_line == "line 1" && second_line == "line 2"
		));

		file.write(b"line 3\n")?;

		let blame = blame_file(repo_path, "foo", None)?;

		assert_eq!(blame.lines.len(), 2);

		stage_add_file(repo_path, file_path)?;
		commit(repo_path, "third commit")?;

		let blame = blame_file(repo_path, "foo", None)?;

		assert_eq!(blame.lines.len(), 3);

		Ok(())
	}

	#[test]
	fn test_blame_windows_path_dividers() {
		let file_path = Path::new("bar\\foo");
		let (_td, repo) = repo_init_empty().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();

		std::fs::create_dir(root.join("bar")).unwrap();

		File::create(root.join(file_path))
			.unwrap()
			.write_all(b"line 1\n")
			.unwrap();

		stage_add_file(repo_path, file_path).unwrap();
		commit(repo_path, "first commit").unwrap();

		assert!(blame_file(repo_path, "bar\\foo", None).is_ok());
	}
}
