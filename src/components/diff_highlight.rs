//! Off-thread syntax highlighting for the unified diff view.
//!
//! The heavy syntect pass is wrapped in an [`AsyncDiffHighlightJob`]
//! so it can run on the shared threadpool instead of blocking the UI
//! thread during file navigation.

use crate::ui::SyntaxText;
use crate::{AsyncAppNotification, SyntaxHighlightProgress};
use asyncgit::{
	asyncjob::{AsyncJob, RunParams},
	DiffLineType, ProgressPercent,
};
use ratatui::{style::Style, text::Text};
use std::{
	ops::Range,
	path::Path,
	sync::{Arc, Mutex},
};

/// Per-line highlight: byte range within the (tab-expanded) line
/// content paired with the syntect-derived style for that token.
pub type LineHighlight = Vec<(Range<usize>, Style)>;

/// Which reconstructed side a diff line was highlighted from.
#[derive(Clone, Copy)]
enum Side {
	New(usize),
	Old(usize),
	Skip,
}

fn push_expanded_line(buf: &mut String, content: &str) {
	let expanded =
		crate::string_utils::tabs_to_spaces(content.to_string());
	buf.push_str(expanded.trim_end_matches(['\n', '\r']));
	buf.push('\n');
}

/// Highlights one reconstructed side (new or old) of the diff,
/// returning per-line token ranges + styles.
fn highlight_side(
	text: &str,
	path: &str,
	syntax_theme: &str,
) -> Vec<LineHighlight> {
	if text.is_empty() {
		return Vec::new();
	}
	let Ok(styled) = SyntaxText::new_sync(
		text.to_string(),
		Path::new(path),
		syntax_theme,
	) else {
		return Vec::new();
	};
	let rendered: Text = (&styled).into();
	rendered
		.lines
		.into_iter()
		.map(|line| {
			let mut offset = 0_usize;
			line.spans
				.into_iter()
				.map(|span| {
					let len = span.content.len();
					let range = offset..offset + len;
					offset += len;
					(range, span.style)
				})
				.collect()
		})
		.collect()
}

/// Highlights a diff given its flat, in-order per-line
/// `(content, line_type)` list (same order the diff renders in).
///
/// Reconstructs the new-side text (`None`/`Add` lines) and old-side
/// text (`None`/`Delete` lines), highlights each independently, then
/// maps every flat line back to its highlight (or `None` for headers
/// and lines that could not be highlighted).
pub fn build_highlight(
	lines: &[(String, DiffLineType)],
	path: &str,
	theme: &str,
) -> Vec<Option<LineHighlight>> {
	let mut new_text = String::new();
	let mut old_text = String::new();
	let mut sides: Vec<Side> = Vec::with_capacity(lines.len());
	let (mut ni, mut oi) = (0_usize, 0_usize);

	for (content, line_type) in lines {
		match line_type {
			DiffLineType::Header => sides.push(Side::Skip),
			DiffLineType::Add => {
				push_expanded_line(&mut new_text, content);
				sides.push(Side::New(ni));
				ni += 1;
			}
			DiffLineType::Delete => {
				push_expanded_line(&mut old_text, content);
				sides.push(Side::Old(oi));
				oi += 1;
			}
			DiffLineType::None => {
				push_expanded_line(&mut new_text, content);
				push_expanded_line(&mut old_text, content);
				sides.push(Side::New(ni));
				ni += 1;
				oi += 1;
			}
		}
	}

	let new_hl = highlight_side(&new_text, path, theme);
	let old_hl = highlight_side(&old_text, path, theme);

	sides
		.into_iter()
		.map(|side| match side {
			Side::New(i) => new_hl.get(i).cloned(),
			Side::Old(i) => old_hl.get(i).cloned(),
			Side::Skip => None,
		})
		.collect()
}

enum JobState {
	Request {
		hash: u64,
		lines: Vec<(String, DiffLineType)>,
		path: String,
		theme: String,
	},
	Response {
		hash: u64,
		result: Vec<Option<LineHighlight>>,
	},
}

/// Async job that runs [`build_highlight`] off the UI thread.
#[derive(Clone, Default)]
pub struct AsyncDiffHighlightJob {
	state: Arc<Mutex<Option<JobState>>>,
}

impl AsyncDiffHighlightJob {
	/// Creates a job for the given diff. `hash` identifies the diff
	/// so the caller can discard results that arrive after the user
	/// has navigated to a different file.
	pub fn new(
		hash: u64,
		lines: Vec<(String, DiffLineType)>,
		path: String,
		theme: String,
	) -> Self {
		Self {
			state: Arc::new(Mutex::new(Some(JobState::Request {
				hash,
				lines,
				path,
				theme,
			}))),
		}
	}

	/// Returns the finished highlight together with the diff `hash`
	/// it was computed for, or `None` if the job has not run yet.
	pub fn result(
		&self,
	) -> Option<(u64, Vec<Option<LineHighlight>>)> {
		if let Ok(mut state) = self.state.lock() {
			if let Some(state) = state.take() {
				return match state {
					JobState::Request { .. } => None,
					JobState::Response { hash, result } => {
						Some((hash, result))
					}
				};
			}
		}

		None
	}
}

impl AsyncJob for AsyncDiffHighlightJob {
	type Notification = AsyncAppNotification;
	type Progress = ProgressPercent;

	fn run(
		&mut self,
		_params: RunParams<Self::Notification, Self::Progress>,
	) -> asyncgit::Result<Self::Notification> {
		let mut state_mutex = self.state.lock()?;

		if let Some(state) = state_mutex.take() {
			*state_mutex = Some(match state {
				JobState::Request {
					hash,
					lines,
					path,
					theme,
				} => {
					let result =
						build_highlight(&lines, &path, &theme);
					JobState::Response { hash, result }
				}
				JobState::Response { hash, result } => {
					JobState::Response { hash, result }
				}
			});
		}

		Ok(AsyncAppNotification::SyntaxHighlighting(
			SyntaxHighlightProgress::Done,
		))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::ui::style::Theme;

	#[test]
	fn test_highlight_side_tokenizes_rust_code() {
		let theme = Theme::default();

		let result = highlight_side(
			"let x = 42;\n",
			"f.rs",
			&theme.get_syntax(),
		);

		assert!(!result.is_empty());
		assert!(result[0].len() > 1);
	}

	#[test]
	fn test_build_highlight_maps_add_line() {
		let theme = Theme::default();
		let lines = vec![
			("@@ hunk @@".to_string(), DiffLineType::Header),
			("let x = 42;".to_string(), DiffLineType::Add),
		];

		let result =
			build_highlight(&lines, "f.rs", &theme.get_syntax());

		assert_eq!(result.len(), 2);
		assert!(result[0].is_none());
		assert!(result[1].as_ref().is_some_and(|v| v.len() > 1));
	}
}
