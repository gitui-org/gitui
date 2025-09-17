use crate::{
	components::CommandInfo, keys::SharedKeyConfig, strings,
	ui::style::SharedTheme,
};
use ratatui::{
	layout::{Alignment, Rect},
	text::{Line, Span},
	widgets::Paragraph,
	Frame,
};
use std::borrow::Cow;
use unicode_width::UnicodeWidthStr;

enum DrawListEntry {
	LineBreak,
	Splitter,
	Command(Command),
}

struct Command {
	txt: String,
	enabled: bool,
	line: usize,
}

/// helper to be used while drawing
pub struct CommandBar {
	draw_list: Vec<DrawListEntry>,
	cmd_infos: Vec<CommandInfo>,
	theme: SharedTheme,
	key_config: SharedKeyConfig,
	lines: u16,
	width: u16,
	expandable: bool,
	expanded: bool,
}

const MORE_WIDTH: u16 = 9;

impl CommandBar {
	pub const fn new(
		theme: SharedTheme,
		key_config: SharedKeyConfig,
	) -> Self {
		Self {
			draw_list: Vec::new(),
			cmd_infos: Vec::new(),
			theme,
			key_config,
			lines: 0,
			width: 0,
			expandable: false,
			expanded: false,
		}
	}

	pub fn refresh_width(&mut self, width: u16) {
		if width != self.width {
			self.refresh_list(width);
			self.width = width;
		}
	}

	fn is_multiline(&self, width: u16) -> bool {
		let mut line_width = 0_usize;
		for c in &self.cmd_infos {
			let entry_w =
				UnicodeWidthStr::width(c.text.name.as_str());

			if line_width + entry_w > width as usize {
				return true;
			}

			line_width += entry_w + 1;
		}

		false
	}

	fn refresh_list(&mut self, width: u16) {
		self.draw_list.clear();

		let width = if self.is_multiline(width) {
			width.saturating_sub(MORE_WIDTH)
		} else {
			width
		};

		let mut line_width = 0_usize;
		let mut lines = 1_u16;

		for c in &self.cmd_infos {
			let entry_w =
				UnicodeWidthStr::width(c.text.name.as_str());

			if line_width + entry_w > width as usize {
				self.draw_list.push(DrawListEntry::LineBreak);
				line_width = 0;
				lines += 1;
			} else if line_width > 0 {
				self.draw_list.push(DrawListEntry::Splitter);
			}

			line_width += entry_w + 1;

			self.draw_list.push(DrawListEntry::Command(Command {
				txt: c.text.name.clone(),
				enabled: c.enabled,
				line: lines.saturating_sub(1) as usize,
			}));
		}

		self.expandable = lines > 1;

		self.lines = lines;
	}

	pub fn set_cmds(&mut self, cmds: Vec<CommandInfo>) {
		self.cmd_infos = cmds
			.into_iter()
			.filter(CommandInfo::show_in_quickbar)
			.collect::<Vec<_>>();
		self.cmd_infos.sort_by_key(|e| e.order);
		self.refresh_list(self.width);
	}

	pub const fn height(&self) -> u16 {
		if self.expandable && self.expanded {
			self.lines
		} else {
			1_u16
		}
	}

	pub fn toggle_more(&mut self) {
		if self.expandable {
			self.expanded = !self.expanded;
		}
	}

	pub fn draw(&self, f: &mut Frame, r: Rect) {
		if r.width < MORE_WIDTH {
			return;
		}
		let splitter = Span::raw(Cow::from(strings::cmd_splitter(
			&self.key_config,
		)));

		let texts = self
			.draw_list
			.split(|c| matches!(c, DrawListEntry::LineBreak))
			.map(|c_arr| {
				Line::from(
					c_arr
						.iter()
						.map(|c| match c {
							DrawListEntry::Command(c) => {
								Span::styled(
									Cow::from(c.txt.as_str()),
									self.theme.commandbar(
										c.enabled, c.line,
									),
								)
							}
							DrawListEntry::LineBreak => {
								// Doesn't exist in split array
								Span::raw("")
							}
							DrawListEntry::Splitter => {
								splitter.clone()
							}
						})
						.collect::<Vec<Span>>(),
				)
			})
			.collect::<Vec<Line>>();

		f.render_widget(
			Paragraph::new(texts).alignment(Alignment::Left),
			r,
		);

		if self.expandable {
			let r = Rect::new(
				r.width.saturating_sub(MORE_WIDTH),
				r.y + r.height.saturating_sub(1),
				MORE_WIDTH.min(r.width),
				1.min(r.height),
			);

			f.render_widget(
				Paragraph::new(Line::from(vec![Span::raw(
					Cow::from(if self.expanded {
						"less [.]"
					} else {
						"more [.]"
					}),
				)]))
				.alignment(Alignment::Right),
				r,
			);
		}
	}
}
