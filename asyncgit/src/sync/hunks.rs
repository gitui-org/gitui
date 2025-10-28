use super::{
	diff::{get_diff_raw, DiffOptions, HunkHeader},
	RepoPath,
};
use crate::{
	error::{Error, Result},
	hash,
	sync::repository::repo,
};
use git2::{ApplyLocation, ApplyOptions, Diff};
use scopetime::scope_time;

///
pub fn stage_hunk(
	repo_path: &RepoPath,
	file_path: &str,
	hunk_hash: u64,
	options: Option<DiffOptions>,
) -> Result<()> {
	scope_time!("stage_hunk");

	let repo = repo(repo_path)?;

	let diff = get_diff_raw(&repo, file_path, false, false, options)?;

	let mut opt = ApplyOptions::new();
	opt.hunk_callback(|hunk| {
		hunk.is_some_and(|hunk| {
			let header = HunkHeader::from(hunk);
			hash(&header) == hunk_hash
		})
	});

	repo.apply(&diff, ApplyLocation::Index, Some(&mut opt))?;

	Ok(())
}

/// this will fail for an all untracked file
pub fn reset_hunk(
	repo_path: &RepoPath,
	file_path: &str,
	hunk_hash: u64,
	options: Option<DiffOptions>,
) -> Result<()> {
	scope_time!("reset_hunk");

	let repo = repo(repo_path)?;

	let diff = get_diff_raw(&repo, file_path, false, false, options)?;

	let hunk_index = find_hunk_index(&diff, hunk_hash);
	if let Some(hunk_index) = hunk_index {
		let mut hunk_idx = 0;
		let mut opt = ApplyOptions::new();
		opt.hunk_callback(|_hunk| {
			let res = hunk_idx == hunk_index;
			hunk_idx += 1;
			res
		});

		let diff = get_diff_raw(&repo, file_path, false, true, None)?;

		repo.apply(&diff, ApplyLocation::WorkDir, Some(&mut opt))?;

		Ok(())
	} else {
		Err(Error::Generic("hunk not found".to_string()))
	}
}

fn find_hunk_index(diff: &Diff, hunk_hash: u64) -> Option<usize> {
	let mut result = None;

	let mut hunk_count = 0;

	let foreach_result = diff.foreach(
		&mut |_, _| true,
		None,
		Some(&mut |_, hunk| {
			let header = HunkHeader::from(hunk);
			if hash(&header) == hunk_hash {
				result = Some(hunk_count);
			}
			hunk_count += 1;
			true
		}),
		None,
	);

	if foreach_result.is_ok() {
		result
	} else {
		None
	}
}

///
pub fn unstage_hunk(
	repo_path: &RepoPath,
	file_path: &str,
	hunk_hash: u64,
	options: Option<DiffOptions>,
) -> Result<bool> {
	scope_time!("revert_hunk");

	let repo = repo(repo_path)?;

	let diff = get_diff_raw(&repo, file_path, true, false, options)?;
	let diff_count_positive = diff.deltas().len();

	let hunk_index = find_hunk_index(&diff, hunk_hash);
	let hunk_index = hunk_index.map_or_else(
		|| Err(Error::Generic("hunk not found".to_string())),
		Ok,
	)?;

	let diff = get_diff_raw(&repo, file_path, true, true, options)?;

	if diff.deltas().len() != diff_count_positive {
		return Err(Error::Generic(format!(
			"hunk error: {}!={}",
			diff.deltas().len(),
			diff_count_positive
		)));
	}

	let mut count = 0;
	{
		let mut hunk_idx = 0;
		let mut opt = ApplyOptions::new();
		opt.hunk_callback(|_hunk| {
			let res = if hunk_idx == hunk_index {
				count += 1;
				true
			} else {
				false
			};

			hunk_idx += 1;

			res
		});

		repo.apply(&diff, ApplyLocation::Index, Some(&mut opt))?;
	}

	Ok(count == 1)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		error::Result,
		sync::{diff::get_diff, tests::repo_init_empty},
	};
	use std::{
		fs::{self, File},
		io::Write,
		path::Path,
	};

	#[test]
	fn reset_untracked_file_which_will_not_find_hunk() -> Result<()> {
		let file_path = Path::new("foo/foo.txt");
		let (_td, repo) = repo_init_empty()?;
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();
		let sub_path = root.join("foo/");

		fs::create_dir_all(&sub_path)?;
		File::create(root.join(file_path))?.write_all(b"test")?;

		let sub_path: &RepoPath = &sub_path.to_str().unwrap().into();
		let diff = get_diff(
			sub_path,
			file_path.to_str().unwrap(),
			false,
			None,
		)?;

		assert!(reset_hunk(
			repo_path,
			file_path.to_str().unwrap(),
			diff.hunks[0].header_hash,
			None,
		)
		.is_err());

		Ok(())
	}
}
