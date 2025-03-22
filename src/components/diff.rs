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
	layout::Rect,
	symbols,
	text::{Line, Span},
	widgets::{Block, Borders, Paragraph},
	Frame,
};
use std::{borrow::Cow, cell::Cell, cmp, path::Path};

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
			let max = diff.lines.saturating_sub(1);

			let new_start = match move_type {
				ScrollType::Down => {
					self.selection.get_bottom().saturating_add(1)
				}
				ScrollType::Up => {
					self.selection.get_top().saturating_sub(1)
				}
				ScrollType::Home => 0,
				ScrollType::End => max,
				ScrollType::PageDown => {
					self.selection.get_bottom().saturating_add(
						self.current_size.get().1.saturating_sub(1)
							as usize,
					)
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
			let max = diff.lines.saturating_sub(1);
			let new_start = cmp::min(max, new_start);
			self.selection = Selection::Single(new_start);
			self.selected_hunk =
				Self::find_selected_hunk(diff, new_start);
		}
	}

	fn lines_count(&self) -> usize {
		self.diff.as_ref().map_or(0, |diff| diff.lines)
	}

	fn max_scroll_right(&self) -> usize {
		self.longest_line
			.saturating_sub(self.current_size.get().0.into())
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

	fn get_text(&self, width: u16, height: u16) -> Vec<Line> {
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

	fn get_text_binary(&self, diff: &FileDiff) -> Vec<Line> {
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

		self.vertical_scroll.update(
			self.selection.get_end(),
			self.lines_count(),
			usize::from(current_height),
		);

		self.horizontal_scroll.update_no_selection(
			self.longest_line,
			current_width.into(),
		);

		let title = format!(
			"{}{}",
			strings::title_diff(&self.key_config),
			self.current.path
		);

		let txt = if self.pending {
			vec![Line::from(vec![Span::styled(
				Cow::from(strings::loading_text(&self.key_config)),
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
					.border_style(self.theme.block(self.focused())),
			),
			r,
		);

		if self.focused() {
			self.vertical_scroll.draw(f, r, &self.theme);

			if self.max_scroll_right() > 0 {
				self.horizontal_scroll.draw(f, r, &self.theme);
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
	use crate::ui::style::Theme;
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
}
