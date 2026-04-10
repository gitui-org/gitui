use super::diff::{get_diff_raw, DiffOptions, HunkHeader};
use crate::error::{Error, Result};
use git2::{Diff, DiffLine, Patch, Repository};

pub struct HunkLines<'a> {
	pub hunk: HunkHeader,
	pub lines: Vec<DiffLine<'a>>,
}

pub fn get_file_diff_patch<'a>(
	repo: &'a Repository,
	file: &str,
	is_staged: bool,
	reverse: bool,
) -> Result<Patch<'a>> {
	let diff = get_diff_raw(
		repo,
		file,
		is_staged,
		reverse,
		Some(DiffOptions {
			context: 1,
			..DiffOptions::default()
		}),
	)?;
	let patches = get_patches(&diff)?;
	if patches.len() > 1 {
		return Err(Error::Generic(String::from("patch error")));
	}

	let patch = patches.into_iter().next().ok_or_else(|| {
		Error::Generic(String::from("no patch found"))
	})?;

	Ok(patch)
}

//
pub fn patch_get_hunklines<'a>(
	patch: &'a Patch<'a>,
) -> Result<Vec<HunkLines<'a>>> {
	let count_hunks = patch.num_hunks();
	let mut res = Vec::with_capacity(count_hunks);
	for hunk_idx in 0..count_hunks {
		let (hunk, _) = patch.hunk(hunk_idx)?;

		let count_lines = patch.num_lines_in_hunk(hunk_idx)?;

		let mut hunk = HunkLines {
			hunk: HunkHeader::from(hunk),
			lines: Vec::with_capacity(count_lines),
		};

		for line_idx in 0..count_lines {
			let line = patch.line_in_hunk(hunk_idx, line_idx)?;
			hunk.lines.push(line);
		}

		res.push(hunk);
	}

	Ok(res)
}

//
fn get_patches<'a>(diff: &Diff<'a>) -> Result<Vec<Patch<'a>>> {
	let count = diff.deltas().len();

	let mut res = Vec::with_capacity(count);
	for idx in 0..count {
		let p = Patch::from_diff(diff, idx)?;
		if let Some(p) = p {
			res.push(p);
		}
	}

	Ok(res)
}
