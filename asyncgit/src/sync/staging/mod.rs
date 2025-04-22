mod discard_tracked;
mod stage_tracked;

pub use discard_tracked::discard_lines;
pub use stage_tracked::stage_lines;

use super::{
	diff::DiffLinePosition, patches::HunkLines, utils::work_dir,
};
use crate::error::Result;
use git2::{DiffLine, DiffLineType, Repository};
use std::{collections::HashSet, fs::File, io::Read};

const NEWLINE: char = '\n';

#[derive(Default)]
struct NewFromOldContent {
	lines: Vec<String>,
	old_index: usize,
}

impl NewFromOldContent {
	fn add_from_hunk(&mut self, line: &DiffLine) -> Result<()> {
		let line = String::from_utf8(line.content().into())?;

		let line = if line.ends_with(NEWLINE) {
			line[0..line.len() - 1].to_string()
		} else {
			line
		};

		self.lines.push(line);

		Ok(())
	}

	fn skip_old_line(&mut self) {
		self.old_index += 1;
	}

	fn add_old_line(&mut self, old_lines: &[&str]) {
		self.lines.push(old_lines[self.old_index].to_string());
		self.old_index += 1;
	}

	fn catchup_to_hunkstart(
		&mut self,
		hunk_start: usize,
		old_lines: &[&str],
	) {
		while hunk_start > self.old_index + 1 {
			self.add_old_line(old_lines);
		}
	}

	fn finish(mut self, old_lines: &[&str]) -> String {
		for line in old_lines.iter().skip(self.old_index) {
			self.lines.push((*line).to_string());
		}
		let lines = self.lines.join("\n");
		if lines.ends_with(NEWLINE) {
			lines
		} else {
			let mut lines = lines;
			lines.push(NEWLINE);
			lines
		}
	}
}

// this is the heart of the per line discard,stage,unstage. heavily inspired by the great work in
// nodegit: https://github.com/nodegit/nodegit
pub fn apply_selection(
	lines: &[DiffLinePosition],
	hunks: &[HunkLines],
	old_lines: &[&str],
	is_staged: bool,
	reverse: bool,
) -> Result<String> {
	let mut new_content = NewFromOldContent::default();
	let lines = lines.iter().collect::<HashSet<_>>();

	let added = if reverse {
		DiffLineType::Deletion
	} else {
		DiffLineType::Addition
	};
	let deleted = if reverse {
		DiffLineType::Addition
	} else {
		DiffLineType::Deletion
	};

	let mut first_hunk_encountered = false;
	for hunk in hunks {
		let hunk_start = if is_staged || reverse {
			usize::try_from(hunk.hunk.new_start)?
		} else {
			usize::try_from(hunk.hunk.old_start)?
		};

		if !first_hunk_encountered {
			let any_selection_in_hunk =
				hunk.lines.iter().any(|line| {
					let line: DiffLinePosition = line.into();
					lines.contains(&line)
				});

			first_hunk_encountered = any_selection_in_hunk;
		}

		if first_hunk_encountered {
			new_content.catchup_to_hunkstart(hunk_start, old_lines);

			for hunk_line in &hunk.lines {
				let hunk_line_pos: DiffLinePosition =
					hunk_line.into();
				let selected_line = lines.contains(&hunk_line_pos);

				log::debug!(
					// println!(
					"{} line: {} [{:?} old, {:?} new] -> {}",
					if selected_line { "*" } else { " " },
					hunk_line.origin(),
					hunk_line.old_lineno(),
					hunk_line.new_lineno(),
					String::from_utf8_lossy(hunk_line.content())
						.trim()
				);

				if hunk_line.origin_value()
					== DiffLineType::DeleteEOFNL
					|| hunk_line.origin_value()
						== DiffLineType::AddEOFNL
				{
					break;
				}

				if (is_staged && !selected_line)
					|| (!is_staged && selected_line)
				{
					if hunk_line.origin_value() == added {
						new_content.add_from_hunk(hunk_line)?;
						if is_staged {
							new_content.skip_old_line();
						}
					} else if hunk_line.origin_value() == deleted {
						if !is_staged {
							new_content.skip_old_line();
						}
					} else {
						new_content.add_old_line(old_lines);
					}
				} else {
					if hunk_line.origin_value() != added {
						new_content.add_from_hunk(hunk_line)?;
					}

					if (is_staged
						&& hunk_line.origin_value() != deleted)
						|| (!is_staged
							&& hunk_line.origin_value() != added)
					{
						new_content.skip_old_line();
					}
				}
			}
		}
	}

	Ok(new_content.finish(old_lines))
}

pub fn load_file(
	repo: &Repository,
	file_path: &str,
) -> Result<String> {
	let repo_path = work_dir(repo)?;
	let mut file = File::open(repo_path.join(file_path).as_path())?;
	let mut res = String::new();
	file.read_to_string(&mut res)?;

	Ok(res)
}
