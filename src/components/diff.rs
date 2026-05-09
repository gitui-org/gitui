use super::{
	utils::scroll_horizontal::HorizontalScroll,
	utils::scroll_vertical::VerticalScroll, CommandBlocking,
	Direction, DrawableComponent, HorizontalScrollType, ScrollType,
};
use crate::{
	app::Environment,
	components::{CommandInfo, Component, EventState},
	keys::{key_match, SharedKeyConfig},
	options::SharedOptions,
	queue::{Action, InternalEvent, NeedsUpdate, Queue, ResetItem},
	string_utils::tabs_to_spaces,
	string_utils::trim_offset,
	strings, try_or_popup,
	ui::style::SharedTheme,
};
use anyhow::Result;
use asyncgit::{
	hash,
	sync::{self, diff::DiffLinePosition, RepoPathRef},
	DiffLine, DiffLineType, FileDiff,
};
use bytesize::ByteSize;
use crossterm::event::Event;
use ratatui::{
	layout::{
		Constraint, Direction as RatatuiDirection, Layout, Rect,
	},
	symbols,
	text::{Line, Span},
	widgets::{Block, Borders, Paragraph},
	Frame,
};
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, cell::Cell, cmp, path::Path};

/// Diff display mode
#[derive(
	Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize,
)]
pub enum DiffMode {
	#[default]
	Unified,
	SideBySide,
}

#[derive(Default)]
struct Current {
	path: String,
	is_stage: bool,
	hash: u64,
}

///
#[derive(Clone, Copy)]
enum Selection {
	Single(usize),
	Multiple(usize, usize),
}

impl Selection {
	const fn get_start(&self) -> usize {
		match self {
			Self::Single(start) | Self::Multiple(start, _) => *start,
		}
	}

	const fn get_end(&self) -> usize {
		match self {
			Self::Single(end) | Self::Multiple(_, end) => *end,
		}
	}

	fn get_top(&self) -> usize {
		match self {
			Self::Single(start) => *start,
			Self::Multiple(start, end) => cmp::min(*start, *end),
		}
	}

	fn get_bottom(&self) -> usize {
		match self {
			Self::Single(start) => *start,
			Self::Multiple(start, end) => cmp::max(*start, *end),
		}
	}

	fn modify(&mut self, direction: Direction, max: usize) {
		let start = self.get_start();
		let old_end = self.get_end();

		*self = match direction {
			Direction::Up => {
				Self::Multiple(start, old_end.saturating_sub(1))
			}

			Direction::Down => {
				Self::Multiple(start, cmp::min(old_end + 1, max))
			}
		};
	}

	fn contains(&self, index: usize) -> bool {
		match self {
			Self::Single(start) => index == *start,
			Self::Multiple(start, end) => {
				if start <= end {
					*start <= index && index <= *end
				} else {
					*end <= index && index <= *start
				}
			}
		}
	}
}

/// A single line in side-by-side diff view
struct SideBySideLine {
	left_content: String,
	left_line_num: Option<u32>,
	right_content: String,
	right_line_num: Option<u32>,
	left_type: DiffLineType,
	right_type: DiffLineType,
	/// Global line index for selection tracking
	global_line_idx: usize,
	/// Index of the hunk this line belongs to
	hunk_idx: usize,
	/// Whether this is the first line of a hunk
	is_hunk_start: bool,
	/// Whether this is the last line of a hunk
	is_hunk_end: bool,
}

///
pub struct DiffComponent {
	repo: RepoPathRef,
	diff: Option<FileDiff>,
	longest_line: usize,
	pending: bool,
	selection: Selection,
	selected_hunk: Option<usize>,
	current_size: Cell<(u16, u16)>,
	focused: bool,
	current: Current,
	vertical_scroll: VerticalScroll,
	horizontal_scroll: HorizontalScroll,
	queue: Queue,
	theme: SharedTheme,
	key_config: SharedKeyConfig,
	is_immutable: bool,
	options: SharedOptions,
	diff_mode: DiffMode,
}

impl DiffComponent {
	///
	pub fn new(env: &Environment, is_immutable: bool) -> Self {
		Self {
			focused: false,
			queue: env.queue.clone(),
			current: Current::default(),
			pending: false,
			selected_hunk: None,
			diff: None,
			longest_line: 0,
			current_size: Cell::new((0, 0)),
			selection: Selection::Single(0),
			vertical_scroll: VerticalScroll::new(),
			horizontal_scroll: HorizontalScroll::new(),
			theme: env.theme.clone(),
			key_config: env.key_config.clone(),
			is_immutable,
			repo: env.repo.clone(),
			options: env.options.clone(),
			diff_mode: env.options.borrow().diff_mode(),
		}
	}
	///
	fn can_scroll(&self) -> bool {
		self.diff.as_ref().is_some_and(|diff| diff.lines > 1)
	}
	///
	pub fn current(&self) -> (String, bool) {
		(self.current.path.clone(), self.current.is_stage)
	}
	///
	const fn can_edit_file(&self) -> bool {
		!self.is_immutable && !self.current.path.is_empty()
	}
	///
	pub fn clear(&mut self, pending: bool) {
		self.current = Current::default();
		self.diff = None;
		self.longest_line = 0;
		self.vertical_scroll.reset();
		self.horizontal_scroll.reset();
		self.selection = Selection::Single(0);
		self.selected_hunk = None;
		self.pending = pending;
	}
	///
	pub fn update(
		&mut self,
		path: String,
		is_stage: bool,
		diff: FileDiff,
	) {
		self.pending = false;

		let hash = hash(&diff);

		if self.current.hash != hash {
			let reset_selection = self.current.path != path;

			self.current = Current {
				path,
				is_stage,
				hash,
			};

			self.diff = Some(diff);

			self.longest_line = self
				.diff
				.iter()
				.flat_map(|diff| diff.hunks.iter())
				.flat_map(|hunk| hunk.lines.iter())
				.map(|line| {
					let converted_content = tabs_to_spaces(
						line.content.as_ref().to_string(),
					);

					converted_content.len()
				})
				.max()
				.map_or(0, |len| {
					// Each hunk uses a 1-character wide vertical bar to its left to indicate
					// selection.
					len + 1
				});

			if reset_selection {
				self.vertical_scroll.reset();
				self.selection = Selection::Single(0);
				self.update_selection(0);
			} else {
				let old_selection = match self.selection {
					Selection::Single(line) => line,
					Selection::Multiple(start, _) => start,
				};
				self.update_selection(old_selection);
			}
		}
	}

	fn move_selection(&mut self, move_type: ScrollType) {
		if let Some(diff) = &self.diff {
			// In side-by-side mode, display lines differ from diff.lines
			// because Delete+Add pairs are shown as one line
			let max = if self.diff_mode == DiffMode::SideBySide {
				self.side_by_side_lines_count().saturating_sub(1)
			} else {
				diff.lines.saturating_sub(1)
			};

			let new_start = match move_type {
				ScrollType::Down => {
					let next =
						self.selection.get_bottom().saturating_add(1);
					cmp::min(next, max)
				}
				ScrollType::Up => {
					self.selection.get_top().saturating_sub(1)
				}
				ScrollType::Home => 0,
				ScrollType::End => max,
				ScrollType::PageDown => {
					let next =
						self.selection.get_bottom().saturating_add(
							self.current_size
								.get()
								.1
								.saturating_sub(1) as usize,
						);
					cmp::min(next, max)
				}
				ScrollType::PageUp => {
					self.selection.get_top().saturating_sub(
						self.current_size.get().1.saturating_sub(1)
							as usize,
					)
				}
			};

			self.update_selection(new_start);
		}
	}

	fn update_selection(&mut self, new_start: usize) {
		if let Some(diff) = &self.diff {
			// In side-by-side mode, display lines differ from diff.lines
			let max = if self.diff_mode == DiffMode::SideBySide {
				self.side_by_side_lines_count().saturating_sub(1)
			} else {
				diff.lines.saturating_sub(1)
			};
			let new_start = cmp::min(max, new_start);
			self.selection = Selection::Single(new_start);
			self.selected_hunk =
				Self::find_selected_hunk_for_display_line(
					diff,
					new_start,
					self.diff_mode,
				);
		}
	}

	fn lines_count(&self) -> usize {
		self.diff.as_ref().map_or(0, |diff| diff.lines)
	}

	/// Get the actual display line count for side-by-side mode
	/// In side-by-side mode, Delete+Add pairs are shown as one line
	fn side_by_side_lines_count(&self) -> usize {
		let Some(diff) = &self.diff else {
			return 0;
		};

		if diff.hunks.is_empty() {
			return 0;
		}

		let mut count = 0_usize;
		for hunk in &diff.hunks {
			let mut i = 0;
			while i < hunk.lines.len() {
				let line = &hunk.lines[i];
				if line.line_type == DiffLineType::Delete {
					// Check if next line is Add (they will be paired)
					if let Some(next) = hunk.lines.get(i + 1) {
						if next.line_type == DiffLineType::Add {
							i += 1; // Skip the Add line in counting
						}
					}
				}
				count += 1;
				i += 1;
			}
		}

		count
	}

	fn max_scroll_right(&self) -> usize {
		let available_width: usize = if self.diff_mode
			== DiffMode::SideBySide
		{
			// In side-by-side mode, each panel's content width:
			// chunks[0].width ≈ r.width / 2
			// content width = chunks[0].width - 7 (border + marker + line_num + space)
			// current_width = r.width - 2
			// So: r.width / 2 - 7 ≈ current_width / 2 - 6
			(self.current_size.get().0 / 2).saturating_sub(6).into()
		} else {
			self.current_size.get().0.into()
		};
		self.longest_line.saturating_sub(available_width)
	}

	fn modify_selection(&mut self, direction: Direction) {
		if self.diff.is_some() {
			self.selection.modify(direction, self.lines_count());
		}
	}

	fn copy_selection(&self) {
		if let Some(diff) = &self.diff {
			let lines_to_copy: Vec<&str> =
				diff.hunks
					.iter()
					.flat_map(|hunk| hunk.lines.iter())
					.enumerate()
					.filter_map(|(i, line)| {
						if self.selection.contains(i) {
							Some(line.content.trim_matches(|c| {
								c == '\n' || c == '\r'
							}))
						} else {
							None
						}
					})
					.collect();

			try_or_popup!(
				self,
				"copy to clipboard error:",
				crate::clipboard::copy_string(
					&lines_to_copy.join("\n")
				)
			);
		}
	}

	fn find_selected_hunk(
		diff: &FileDiff,
		line_selected: usize,
	) -> Option<usize> {
		let mut line_cursor = 0_usize;
		for (i, hunk) in diff.hunks.iter().enumerate() {
			let hunk_len = hunk.lines.len();
			let hunk_min = line_cursor;
			let hunk_max = line_cursor + hunk_len;

			let hunk_selected =
				hunk_min <= line_selected && hunk_max > line_selected;

			if hunk_selected {
				return Some(i);
			}

			line_cursor += hunk_len;
		}

		None
	}

	/// Find the hunk index for a display line (accounting for side-by-side pairing)
	fn find_selected_hunk_for_display_line(
		diff: &FileDiff,
		display_line_selected: usize,
		diff_mode: DiffMode,
	) -> Option<usize> {
		if diff_mode == DiffMode::Unified {
			return Self::find_selected_hunk(
				diff,
				display_line_selected,
			);
		}

		// For side-by-side mode, count display lines (where Delete+Add pairs count as 1)
		let mut display_cursor = 0_usize;
		for (i, hunk) in diff.hunks.iter().enumerate() {
			let mut j = 0;
			let hunk_start = display_cursor;
			while j < hunk.lines.len() {
				let line = &hunk.lines[j];
				if display_cursor == display_line_selected {
					return Some(i);
				}
				if line.line_type == DiffLineType::Delete {
					if let Some(next) = hunk.lines.get(j + 1) {
						if next.line_type == DiffLineType::Add {
							j += 1;
						}
					}
				}
				display_cursor += 1;
				j += 1;
			}
			// Check if this is the last line of the hunk
			if display_line_selected >= hunk_start
				&& display_line_selected < display_cursor
			{
				return Some(i);
			}
		}

		None
	}

	fn get_text(&self, width: u16, height: u16) -> Vec<Line<'_>> {
		if let Some(diff) = &self.diff {
			return if diff.hunks.is_empty() {
				self.get_text_binary(diff)
			} else {
				let mut res: Vec<Line> = Vec::new();

				let min = self.vertical_scroll.get_top();
				let max = min + height as usize;

				let mut line_cursor = 0_usize;
				let mut lines_added = 0_usize;

				for (i, hunk) in diff.hunks.iter().enumerate() {
					let hunk_selected = self.focused()
						&& self.selected_hunk.is_some_and(|s| s == i);

					if lines_added >= height as usize {
						break;
					}

					let hunk_len = hunk.lines.len();
					let hunk_min = line_cursor;
					let hunk_max = line_cursor + hunk_len;

					if Self::hunk_visible(
						hunk_min, hunk_max, min, max,
					) {
						for (i, line) in hunk.lines.iter().enumerate()
						{
							if line_cursor >= min
								&& line_cursor <= max
							{
								res.push(Self::get_line_to_add(
									width,
									line,
									self.focused()
										&& self
											.selection
											.contains(line_cursor),
									hunk_selected,
									i == hunk_len - 1,
									&self.theme,
									self.horizontal_scroll
										.get_right(),
								));
								lines_added += 1;
							}

							line_cursor += 1;
						}
					} else {
						line_cursor += hunk_len;
					}
				}

				res
			};
		}

		vec![]
	}

	fn get_text_binary(&self, diff: &FileDiff) -> Vec<Line<'_>> {
		let is_positive = diff.size_delta >= 0;
		let delta_byte_size =
			ByteSize::b(diff.size_delta.unsigned_abs());
		let sign = if is_positive { "+" } else { "-" };
		vec![Line::from(vec![
			Span::raw(Cow::from("size: ")),
			Span::styled(
				Cow::from(format!("{}", ByteSize::b(diff.sizes.0))),
				self.theme.text(false, false),
			),
			Span::raw(Cow::from(" -> ")),
			Span::styled(
				Cow::from(format!("{}", ByteSize::b(diff.sizes.1))),
				self.theme.text(false, false),
			),
			Span::raw(Cow::from(" (")),
			Span::styled(
				Cow::from(format!("{sign}{delta_byte_size:}")),
				self.theme.diff_line(
					if is_positive {
						DiffLineType::Add
					} else {
						DiffLineType::Delete
					},
					false,
				),
			),
			Span::raw(Cow::from(")")),
		])]
	}

	fn get_line_to_add<'a>(
		width: u16,
		line: &'a DiffLine,
		selected: bool,
		selected_hunk: bool,
		end_of_hunk: bool,
		theme: &SharedTheme,
		scrolled_right: usize,
	) -> Line<'a> {
		let style = theme.diff_hunk_marker(selected_hunk);

		let is_content_line =
			matches!(line.line_type, DiffLineType::None);

		let left_side_of_line = if end_of_hunk {
			Span::styled(Cow::from(symbols::line::BOTTOM_LEFT), style)
		} else {
			match line.line_type {
				DiffLineType::Header => Span::styled(
					Cow::from(symbols::line::TOP_LEFT),
					style,
				),
				_ => Span::styled(
					Cow::from(symbols::line::VERTICAL),
					style,
				),
			}
		};

		let content =
			if !is_content_line && line.content.as_ref().is_empty() {
				theme.line_break()
			} else {
				tabs_to_spaces(line.content.as_ref().to_string())
			};
		let content = trim_offset(&content, scrolled_right);

		let filled = if selected {
			// selected line
			format!("{content:w$}\n", w = width as usize)
		} else {
			// weird eof missing eol line
			format!("{content}\n")
		};

		Line::from(vec![
			left_side_of_line,
			Span::styled(
				Cow::from(filled),
				theme.diff_line(line.line_type, selected),
			),
		])
	}

	const fn hunk_visible(
		hunk_min: usize,
		hunk_max: usize,
		min: usize,
		max: usize,
	) -> bool {
		// full overlap
		if hunk_min <= min && hunk_max >= max {
			return true;
		}

		// partly overlap
		if (hunk_min >= min && hunk_min <= max)
			|| (hunk_max >= min && hunk_max <= max)
		{
			return true;
		}

		false
	}

	fn unstage_hunk(&self) -> Result<()> {
		if let Some(diff) = &self.diff {
			if let Some(hunk) = self.selected_hunk {
				let hash = diff.hunks[hunk].header_hash;
				sync::unstage_hunk(
					&self.repo.borrow(),
					&self.current.path,
					hash,
					Some(self.options.borrow().diff_options()),
				)?;
				self.queue_update();
			}
		}

		Ok(())
	}

	fn stage_hunk(&self) -> Result<()> {
		if let Some(diff) = &self.diff {
			if let Some(hunk) = self.selected_hunk {
				if diff.untracked {
					sync::stage_add_file(
						&self.repo.borrow(),
						Path::new(&self.current.path),
					)?;
				} else {
					let hash = diff.hunks[hunk].header_hash;
					sync::stage_hunk(
						&self.repo.borrow(),
						&self.current.path,
						hash,
						Some(self.options.borrow().diff_options()),
					)?;
				}

				self.queue_update();
			}
		}

		Ok(())
	}

	fn queue_update(&self) {
		self.queue.push(InternalEvent::Update(NeedsUpdate::ALL));
	}

	fn reset_hunk(&self) {
		if let Some(diff) = &self.diff {
			if let Some(hunk) = self.selected_hunk {
				let hash = diff.hunks[hunk].header_hash;

				self.queue.push(InternalEvent::ConfirmAction(
					Action::ResetHunk(
						self.current.path.clone(),
						hash,
					),
				));
			}
		}
	}

	fn reset_lines(&self) {
		self.queue.push(InternalEvent::ConfirmAction(
			Action::ResetLines(
				self.current.path.clone(),
				self.selected_lines(),
			),
		));
	}

	fn stage_lines(&self) {
		if let Some(diff) = &self.diff {
			//TODO: support untracked files as well
			if !diff.untracked {
				let selected_lines = self.selected_lines();

				try_or_popup!(
					self,
					"(un)stage lines:",
					sync::stage_lines(
						&self.repo.borrow(),
						&self.current.path,
						self.is_stage(),
						&selected_lines,
					)
				);

				self.queue_update();
			}
		}
	}

	fn selected_lines(&self) -> Vec<DiffLinePosition> {
		self.diff
			.as_ref()
			.map(|diff| {
				diff.hunks
					.iter()
					.flat_map(|hunk| hunk.lines.iter())
					.enumerate()
					.filter_map(|(i, line)| {
						let is_add_or_delete = line.line_type
							== DiffLineType::Add
							|| line.line_type == DiffLineType::Delete;
						if self.selection.contains(i)
							&& is_add_or_delete
						{
							Some(line.position)
						} else {
							None
						}
					})
					.collect()
			})
			.unwrap_or_default()
	}

	fn reset_untracked(&self) {
		self.queue.push(InternalEvent::ConfirmAction(Action::Reset(
			ResetItem {
				path: self.current.path.clone(),
			},
		)));
	}

	fn stage_unstage_hunk(&self) -> Result<()> {
		if self.current.is_stage {
			self.unstage_hunk()?;
		} else {
			self.stage_hunk()?;
		}

		Ok(())
	}

	fn calc_hunk_move_target(
		&self,
		direction: isize,
	) -> Option<usize> {
		let diff = self.diff.as_ref()?;
		if diff.hunks.is_empty() {
			return None;
		}
		let max = diff.hunks.len() - 1;
		let target_index = self.selected_hunk.map_or(0, |i| {
			let target = if direction >= 0 {
				i.saturating_add(direction.unsigned_abs())
			} else {
				i.saturating_sub(direction.unsigned_abs())
			};
			std::cmp::min(max, target)
		});
		Some(target_index)
	}

	fn diff_hunk_move_up_down(&mut self, direction: isize) {
		let Some(diff) = &self.diff else { return };
		let hunk_index = self.calc_hunk_move_target(direction);
		// return if selected_hunk not change
		if self.selected_hunk == hunk_index {
			return;
		}
		if let Some(hunk_index) = hunk_index {
			let line_index = diff
				.hunks
				.iter()
				.take(hunk_index)
				.fold(0, |sum, hunk| sum + hunk.lines.len());
			let hunk = &diff.hunks[hunk_index];
			self.selection = Selection::Single(line_index);
			self.selected_hunk = Some(hunk_index);
			self.vertical_scroll.move_area_to_visible(
				self.current_size.get().1 as usize,
				line_index,
				line_index.saturating_add(hunk.lines.len()),
			);
		}
	}

	/// Toggle between unified and side-by-side diff mode
	pub fn toggle_diff_mode(&mut self) {
		self.diff_mode = match self.diff_mode {
			DiffMode::Unified => DiffMode::SideBySide,
			DiffMode::SideBySide => DiffMode::Unified,
		};
		self.options.borrow_mut().set_diff_mode(self.diff_mode);
	}

	fn get_side_by_side_lines(
		&self,
		height: u16,
	) -> Vec<SideBySideLine> {
		let Some(diff) = &self.diff else {
			return Vec::new();
		};

		if diff.hunks.is_empty() {
			return Vec::new();
		}

		let min = self.vertical_scroll.get_top();
		let max = min + height as usize;

		let mut result = Vec::new();
		// Use display_cursor to track display line index (where Delete+Add pairs count as 1)
		let mut display_cursor = 0_usize;

		for (hunk_idx, hunk) in diff.hunks.iter().enumerate() {
			// Calculate display line range for this hunk
			let hunk_display_start = display_cursor;
			let mut hunk_display_len = 0_usize;
			{
				let mut j = 0;
				while j < hunk.lines.len() {
					let line = &hunk.lines[j];
					if line.line_type == DiffLineType::Delete {
						if let Some(next) = hunk.lines.get(j + 1) {
							if next.line_type == DiffLineType::Add {
								j += 1;
							}
						}
					}
					hunk_display_len += 1;
					j += 1;
				}
			}
			let hunk_display_end =
				hunk_display_start + hunk_display_len;

			if Self::hunk_visible(
				hunk_display_start,
				hunk_display_end,
				min,
				max,
			) {
				let mut i = 0;
				while i < hunk.lines.len() {
					let line = &hunk.lines[i];
					let global_display_idx = display_cursor;
					let is_hunk_start = i == 0;
					// Calculate if this is the last display line of the hunk
					let is_hunk_end = {
						let mut remaining = hunk.lines.len() - i;
						let next = hunk.lines.get(i + 1);
						if line.line_type == DiffLineType::Delete
							&& next.is_some_and(|n| {
								n.line_type == DiffLineType::Add
							}) {
							remaining -= 1;
						}
						remaining == 1
					};

					if global_display_idx >= min
						&& global_display_idx <= max
					{
						match line.line_type {
							DiffLineType::Delete => {
								// Look ahead for a matching add line
								let next_line = hunk.lines.get(i + 1);
								let (
									right_content,
									right_num,
									right_type,
								) = if let Some(next) = next_line {
									if next.line_type
										== DiffLineType::Add
									{
										i += 1;
										(
											tabs_to_spaces(
												next.content
													.as_ref()
													.to_string(),
											),
											next.position.new_lineno,
											DiffLineType::Add,
										)
									} else {
										(
											String::new(),
											None,
											DiffLineType::None,
										)
									}
								} else {
									(
										String::new(),
										None,
										DiffLineType::None,
									)
								};

								result.push(SideBySideLine {
									left_content: tabs_to_spaces(
										line.content
											.as_ref()
											.to_string(),
									),
									left_line_num: line
										.position
										.old_lineno,
									right_content,
									right_line_num: right_num,
									left_type: DiffLineType::Delete,
									right_type,
									global_line_idx:
										global_display_idx,
									hunk_idx,
									is_hunk_start,
									is_hunk_end,
								});
							}
							DiffLineType::Add => {
								// Add line not paired with a delete
								result.push(SideBySideLine {
									left_content: String::new(),
									left_line_num: None,
									right_content: tabs_to_spaces(
										line.content
											.as_ref()
											.to_string(),
									),
									right_line_num: line
										.position
										.new_lineno,
									left_type: DiffLineType::None,
									right_type: DiffLineType::Add,
									global_line_idx:
										global_display_idx,
									hunk_idx,
									is_hunk_start,
									is_hunk_end,
								});
							}
							DiffLineType::Header => {
								let header_content = tabs_to_spaces(
									line.content.as_ref().to_string(),
								);
								result.push(SideBySideLine {
									left_content: header_content,
									left_line_num: None,
									right_content: String::new(),
									right_line_num: None,
									left_type: DiffLineType::Header,
									right_type: DiffLineType::Header,
									global_line_idx:
										global_display_idx,
									hunk_idx,
									is_hunk_start,
									is_hunk_end,
								});
							}
							DiffLineType::None => {
								// Context line - appears in both columns
								result.push(SideBySideLine {
									left_content: tabs_to_spaces(
										line.content
											.as_ref()
											.to_string(),
									),
									left_line_num: line
										.position
										.old_lineno,
									right_content: tabs_to_spaces(
										line.content
											.as_ref()
											.to_string(),
									),
									right_line_num: line
										.position
										.new_lineno,
									left_type: DiffLineType::None,
									right_type: DiffLineType::None,
									global_line_idx:
										global_display_idx,
									hunk_idx,
									is_hunk_start,
									is_hunk_end,
								});
							}
						}
					}

					// Increment display cursor for each display line
					display_cursor += 1;
					i += 1;
				}
			} else {
				// Skip this hunk's display lines
				display_cursor += hunk_display_len;
			}
		}

		result
	}

	fn draw_side_by_side(
		&self,
		f: &mut Frame,
		r: Rect,
		title: &str,
		height: u16,
	) -> Result<()> {
		// Split area into left and right columns
		let chunks = Layout::default()
			.direction(RatatuiDirection::Horizontal)
			.constraints(
				[
					Constraint::Percentage(50),
					Constraint::Percentage(50),
				]
				.as_ref(),
			)
			.split(r);

		// Calculate available width for content (subtract borders, marker, line number, space)
		// Each panel has: 1 border + 1 marker + 4 line num + 1 space = 7 chars overhead
		let panel_width = chunks[0].width.saturating_sub(7) as usize;

		let lines = self.get_side_by_side_lines(height);
		let scrolled_right = self.horizontal_scroll.get_right();
		let selected_hunk = self.selected_hunk;

		// Get current selection index
		let current_selection = self.selection.get_end();

		// Build left column text with selection highlighting
		let left_txt: Vec<Line> = lines
			.iter()
			.map(|line| {
				let selected = self.focused()
					&& line.global_line_idx == current_selection;
				let hunk_selected = self.focused()
					&& selected_hunk
						.is_some_and(|h| h == line.hunk_idx);
				let left_content =
					trim_offset(&line.left_content, scrolled_right);
				let line_num_str = line
					.left_line_num
					.map_or(String::from("    "), |n| {
						format!("{n:4}")
					});

				// Get hunk marker style
				let marker_style =
					self.theme.diff_hunk_marker(hunk_selected);
				let marker = if line.is_hunk_end {
					symbols::line::BOTTOM_LEFT
				} else if line.is_hunk_start {
					symbols::line::TOP_LEFT
				} else {
					symbols::line::VERTICAL
				};

				// Pad content to fill width when selected
				let content = if selected {
					format!("{:w$}\n", left_content, w = panel_width)
				} else {
					format!("{left_content}\n")
				};

				// For lines where left side is empty (e.g., Add lines without Delete pair),
				// still apply selection highlight to maintain visual consistency.
				// Show line_break symbol (¶) for empty Add/Delete lines, same as unified mode.
				if line.left_content.is_empty() {
					// Show line_break symbol for empty Add/Delete lines
					let display_content =
						if line.left_type != DiffLineType::None {
							self.theme.line_break()
						} else {
							String::new()
						};
					let content = if selected {
						format!(
							"{:w$}\n",
							display_content,
							w = panel_width
						)
					} else {
						format!("{display_content}\n")
					};
					Line::from(vec![
						Span::styled(Cow::from(marker), marker_style),
						Span::styled(
							Cow::from(line_num_str),
							self.theme.text(false, false),
						),
						// Gap - never highlighted
						Span::styled(
							Cow::from(" "),
							self.theme.text(false, false),
						),
						Span::styled(
							Cow::from(content),
							self.theme
								.diff_line(line.left_type, selected),
						),
					])
				} else {
					Line::from(vec![
						Span::styled(Cow::from(marker), marker_style),
						Span::styled(
							Cow::from(line_num_str),
							self.theme.text(false, false),
						),
						// Gap between line number and content - never highlighted
						Span::styled(
							Cow::from(" "),
							self.theme.text(false, false),
						),
						Span::styled(
							Cow::from(content),
							self.theme
								.diff_line(line.left_type, selected),
						),
					])
				}
			})
			.collect();

		// Build right column text with selection highlighting
		let right_txt: Vec<Line> = lines
			.iter()
			.map(|line| {
				let selected = self.focused()
					&& line.global_line_idx == current_selection;
				let hunk_selected = self.focused()
					&& selected_hunk
						.is_some_and(|h| h == line.hunk_idx);
				let right_content =
					trim_offset(&line.right_content, scrolled_right);
				let line_num_str = line
					.right_line_num
					.map_or(String::from("    "), |n| {
						format!("{n:4}")
					});

				// Get hunk marker style
				let marker_style =
					self.theme.diff_hunk_marker(hunk_selected);
				let marker = if line.is_hunk_end {
					symbols::line::BOTTOM_LEFT
				} else if line.is_hunk_start {
					symbols::line::TOP_LEFT
				} else {
					symbols::line::VERTICAL
				};

				// Pad content to fill width when selected
				let content = if selected {
					format!("{:w$}\n", right_content, w = panel_width)
				} else {
					format!("{right_content}\n")
				};

				// For lines where right side is empty (Header or paired Delete),
				// still apply selection highlight to maintain visual consistency.
				// Show line_break symbol (¶) for empty Add/Delete lines, same as unified mode.
				if line.right_type == DiffLineType::Header
					|| line.right_content.is_empty()
				{
					// Show line_break symbol for empty Add/Delete lines (but not Header)
					let display_content = if line.right_type
						!= DiffLineType::None
						&& line.right_type != DiffLineType::Header
					{
						self.theme.line_break()
					} else {
						String::new()
					};
					let filler = if selected {
						format!(
							"{:w$}\n",
							display_content,
							w = panel_width
						)
					} else {
						format!("{display_content}\n")
					};
					Line::from(vec![
						Span::styled(Cow::from(marker), marker_style),
						Span::styled(
							Cow::from(line_num_str),
							self.theme.text(false, false),
						),
						// Gap - never highlighted
						Span::styled(
							Cow::from(" "),
							self.theme.text(false, false),
						),
						Span::styled(
							Cow::from(filler),
							self.theme
								.diff_line(line.right_type, selected),
						),
					])
				} else {
					Line::from(vec![
						Span::styled(Cow::from(marker), marker_style),
						Span::styled(
							Cow::from(line_num_str),
							self.theme.text(false, false),
						),
						// Gap between line number and content - never highlighted
						Span::styled(
							Cow::from(" "),
							self.theme.text(false, false),
						),
						Span::styled(
							Cow::from(content),
							self.theme
								.diff_line(line.right_type, selected),
						),
					])
				}
			})
			.collect();

		// Draw left column
		f.render_widget(
			Paragraph::new(left_txt).block(
				Block::default()
					.title(Span::styled(
						format!("{title} [Old]"),
						self.theme.title(self.focused()),
					))
					.borders(Borders::ALL)
					.border_style(self.theme.block(self.focused())),
			),
			chunks[0],
		);

		// Draw right column
		f.render_widget(
			Paragraph::new(right_txt).block(
				Block::default()
					.title(Span::styled(
						"[New]",
						self.theme.title(self.focused()),
					))
					.borders(Borders::ALL)
					.border_style(self.theme.block(self.focused())),
			),
			chunks[1],
		);

		if self.focused() {
			self.vertical_scroll.draw(f, r, &self.theme);

			if self.max_scroll_right() > 0 {
				self.horizontal_scroll.draw(f, r, &self.theme);
			}
		}

		Ok(())
	}

	const fn is_stage(&self) -> bool {
		self.current.is_stage
	}
}

impl DrawableComponent for DiffComponent {
	fn draw(&self, f: &mut Frame, r: Rect) -> Result<()> {
		self.current_size.set((
			r.width.saturating_sub(2),
			r.height.saturating_sub(2),
		));

		let current_width = self.current_size.get().0;
		let current_height = self.current_size.get().1;

		// Use display line count for side-by-side mode
		let lines_count = if self.diff_mode == DiffMode::SideBySide {
			self.side_by_side_lines_count()
		} else {
			self.lines_count()
		};

		self.vertical_scroll.update(
			self.selection.get_end(),
			lines_count,
			usize::from(current_height),
		);

		// In side-by-side mode, each panel content width is smaller
		// chunks[0].width ≈ r.width / 2, content = chunks[0].width - 7
		// ≈ current_width / 2 - 6
		let panel_content_width: usize =
			if self.diff_mode == DiffMode::SideBySide {
				(current_width / 2).saturating_sub(6).into()
			} else {
				current_width.into()
			};

		self.horizontal_scroll.update_no_selection(
			self.longest_line,
			panel_content_width,
		);

		let title = format!(
			"{}{}",
			strings::title_diff(&self.key_config),
			self.current.path
		);

		if self.diff_mode == DiffMode::SideBySide && !self.pending {
			self.draw_side_by_side(f, r, &title, current_height)?;
		} else {
			let txt = if self.pending {
				vec![Line::from(vec![Span::styled(
					Cow::from(strings::loading_text(
						&self.key_config,
					)),
					self.theme.text(false, false),
				)])]
			} else {
				self.get_text(r.width, current_height)
			};

			f.render_widget(
				Paragraph::new(txt).block(
					Block::default()
						.title(Span::styled(
							title.as_str(),
							self.theme.title(self.focused()),
						))
						.borders(Borders::ALL)
						.border_style(
							self.theme.block(self.focused()),
						),
				),
				r,
			);

			if self.focused() {
				self.vertical_scroll.draw(f, r, &self.theme);

				if self.max_scroll_right() > 0 {
					self.horizontal_scroll.draw(f, r, &self.theme);
				}
			}
		}

		Ok(())
	}
}

impl Component for DiffComponent {
	fn commands(
		&self,
		out: &mut Vec<CommandInfo>,
		_force_all: bool,
	) -> CommandBlocking {
		out.push(CommandInfo::new(
			strings::commands::scroll(&self.key_config),
			self.can_scroll(),
			self.focused(),
		));
		out.push(CommandInfo::new(
			strings::commands::diff_hunk_next(&self.key_config),
			self.calc_hunk_move_target(1) != self.selected_hunk,
			self.focused(),
		));
		out.push(CommandInfo::new(
			strings::commands::diff_hunk_prev(&self.key_config),
			self.calc_hunk_move_target(-1) != self.selected_hunk,
			self.focused(),
		));
		out.push(
			CommandInfo::new(
				strings::commands::diff_home_end(&self.key_config),
				self.can_scroll(),
				self.focused(),
			)
			.hidden(),
		);

		if !self.is_immutable {
			out.push(CommandInfo::new(
				strings::commands::edit_item(&self.key_config),
				self.can_edit_file(),
				self.focused() && self.can_edit_file(),
			));
			out.push(CommandInfo::new(
				strings::commands::diff_hunk_remove(&self.key_config),
				self.selected_hunk.is_some(),
				self.focused() && self.is_stage(),
			));
			out.push(CommandInfo::new(
				strings::commands::diff_hunk_add(&self.key_config),
				self.selected_hunk.is_some(),
				self.focused() && !self.is_stage(),
			));
			out.push(CommandInfo::new(
				strings::commands::diff_hunk_revert(&self.key_config),
				self.selected_hunk.is_some(),
				self.focused() && !self.is_stage(),
			));
			out.push(CommandInfo::new(
				strings::commands::diff_lines_revert(
					&self.key_config,
				),
				//TODO: only if any modifications are selected
				true,
				self.focused() && !self.is_stage(),
			));
			out.push(CommandInfo::new(
				strings::commands::diff_lines_stage(&self.key_config),
				//TODO: only if any modifications are selected
				true,
				self.focused() && !self.is_stage(),
			));
			out.push(CommandInfo::new(
				strings::commands::diff_lines_unstage(
					&self.key_config,
				),
				//TODO: only if any modifications are selected
				true,
				self.focused() && self.is_stage(),
			));
		}

		out.push(CommandInfo::new(
			strings::commands::copy(&self.key_config),
			true,
			self.focused(),
		));

		CommandBlocking::PassingOn
	}

	#[allow(clippy::cognitive_complexity, clippy::too_many_lines)]
	fn event(&mut self, ev: &Event) -> Result<EventState> {
		if self.focused() {
			if let Event::Key(e) = ev {
				return if key_match(e, self.key_config.keys.move_down)
				{
					self.move_selection(ScrollType::Down);
					Ok(EventState::Consumed)
				} else if key_match(
					e,
					self.key_config.keys.shift_down,
				) {
					self.modify_selection(Direction::Down);
					Ok(EventState::Consumed)
				} else if key_match(e, self.key_config.keys.shift_up)
				{
					self.modify_selection(Direction::Up);
					Ok(EventState::Consumed)
				} else if key_match(e, self.key_config.keys.end) {
					self.move_selection(ScrollType::End);
					Ok(EventState::Consumed)
				} else if key_match(e, self.key_config.keys.home) {
					self.move_selection(ScrollType::Home);
					Ok(EventState::Consumed)
				} else if key_match(e, self.key_config.keys.move_up) {
					self.move_selection(ScrollType::Up);
					Ok(EventState::Consumed)
				} else if key_match(e, self.key_config.keys.page_up) {
					self.move_selection(ScrollType::PageUp);
					Ok(EventState::Consumed)
				} else if key_match(e, self.key_config.keys.page_down)
				{
					self.move_selection(ScrollType::PageDown);
					Ok(EventState::Consumed)
				} else if key_match(
					e,
					self.key_config.keys.move_right,
				) {
					self.horizontal_scroll
						.move_right(HorizontalScrollType::Right);
					Ok(EventState::Consumed)
				} else if key_match(e, self.key_config.keys.move_left)
				{
					self.horizontal_scroll
						.move_right(HorizontalScrollType::Left);
					Ok(EventState::Consumed)
				} else if key_match(
					e,
					self.key_config.keys.diff_hunk_next,
				) {
					self.diff_hunk_move_up_down(1);
					Ok(EventState::Consumed)
				} else if key_match(
					e,
					self.key_config.keys.diff_hunk_prev,
				) {
					self.diff_hunk_move_up_down(-1);
					Ok(EventState::Consumed)
				} else if key_match(e, self.key_config.keys.edit_file)
					&& self.can_edit_file()
				{
					self.queue.push(
						InternalEvent::OpenExternalEditor(Some(
							self.current.path.clone(),
						)),
					);
					Ok(EventState::Consumed)
				} else if key_match(
					e,
					self.key_config.keys.stage_unstage_item,
				) && !self.is_immutable
				{
					try_or_popup!(
						self,
						"hunk error:",
						self.stage_unstage_hunk()
					);

					Ok(EventState::Consumed)
				} else if key_match(
					e,
					self.key_config.keys.status_reset_item,
				) && !self.is_immutable
					&& !self.is_stage()
				{
					if let Some(diff) = &self.diff {
						if diff.untracked {
							self.reset_untracked();
						} else {
							self.reset_hunk();
						}
					}
					Ok(EventState::Consumed)
				} else if key_match(
					e,
					self.key_config.keys.diff_stage_lines,
				) && !self.is_immutable
				{
					self.stage_lines();
					Ok(EventState::Consumed)
				} else if key_match(
					e,
					self.key_config.keys.diff_reset_lines,
				) && !self.is_immutable
					&& !self.is_stage()
				{
					if let Some(diff) = &self.diff {
						//TODO: reset untracked lines
						if !diff.untracked {
							self.reset_lines();
						}
					}
					Ok(EventState::Consumed)
				} else if key_match(e, self.key_config.keys.copy) {
					self.copy_selection();
					Ok(EventState::Consumed)
				} else if key_match(
					e,
					self.key_config.keys.diff_mode_toggle,
				) {
					self.toggle_diff_mode();
					Ok(EventState::Consumed)
				} else {
					Ok(EventState::NotConsumed)
				};
			}
		}

		Ok(EventState::NotConsumed)
	}

	fn focused(&self) -> bool {
		self.focused
	}
	fn focus(&mut self, focus: bool) {
		self.focused = focus;
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		app::Environment, queue::InternalEvent, ui::style::Theme,
	};
	use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
	use std::io::Write;
	use std::rc::Rc;
	use tempfile::NamedTempFile;

	#[test]
	fn test_line_break() {
		let diff_line = DiffLine {
			content: "".into(),
			line_type: DiffLineType::Add,
			position: Default::default(),
		};

		{
			let default_theme = Rc::new(Theme::default());

			assert_eq!(
				DiffComponent::get_line_to_add(
					4,
					&diff_line,
					false,
					false,
					false,
					&default_theme,
					0
				)
				.spans
				.last()
				.unwrap(),
				&Span::styled(
					Cow::from("¶\n"),
					default_theme
						.diff_line(diff_line.line_type, false)
				)
			);
		}

		{
			let mut file = NamedTempFile::new().unwrap();

			writeln!(
				file,
				r#"
(
	line_break: Some("+")
)
"#
			)
			.unwrap();

			let theme =
				Rc::new(Theme::init(&file.path().to_path_buf()));

			assert_eq!(
				DiffComponent::get_line_to_add(
					4, &diff_line, false, false, false, &theme, 0
				)
				.spans
				.last()
				.unwrap(),
				&Span::styled(
					Cow::from("+\n"),
					theme.diff_line(diff_line.line_type, false)
				)
			);
		}
	}

	#[test]
	fn diff_component_opens_editor_for_current_file() {
		let env = Environment::test_env();
		let mut diff = DiffComponent::new(&env, false);

		diff.focus(true);
		diff.current.path = String::from("src/main.rs");

		let event = Event::Key(KeyEvent::new(
			KeyCode::Char('e'),
			KeyModifiers::empty(),
		));

		assert!(matches!(
			diff.event(&event).unwrap(),
			EventState::Consumed
		));

		let event = env.queue.pop();
		assert!(matches!(
			event,
			Some(InternalEvent::OpenExternalEditor(Some(path)))
				if path == "src/main.rs"
		));
	}

	#[test]
	fn test_commands_no_longer_contains_toggle_diff() {
		let env = Environment::test_env();
		let diff = DiffComponent::new(&env, false);
		let mut cmds = Vec::new();
		diff.commands(&mut cmds, true);

		let contains_toggle = cmds.iter().any(|c| {
			c.text.name
				== strings::commands::diff_toggle_mode(&env.key_config)
					.name
		});
		assert!(!contains_toggle);
	}

	#[test]
	fn test_diff_mode_toggle_event() {
		let env = Environment::test_env();
		let mut diff = DiffComponent::new(&env, false);
		diff.focus(true);

		let event = Event::Key(KeyEvent::from(
			&env.key_config.keys.diff_mode_toggle,
		));

		assert!(matches!(
			diff.event(&event).unwrap(),
			EventState::Consumed
		));
	}
}
