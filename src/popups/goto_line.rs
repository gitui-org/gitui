use crate::{
	app::Environment,
	components::{
		visibility_blocking, CommandBlocking, CommandInfo, Component,
		DrawableComponent, EventState,
	},
	keys::{key_match, SharedKeyConfig},
	queue::{InternalEvent, Queue},
	strings,
	ui::{self, style::SharedTheme},
};

use ratatui::{
	layout::Rect,
	style::{Color, Style},
	widgets::{Block, Clear, Paragraph},
	Frame,
};

use anyhow::Result;

use crossterm::event::{Event, KeyCode};

pub struct GotoLinePopup {
	visible: bool,
	input: String,
	line_number: usize,
	key_config: SharedKeyConfig,
	queue: Queue,
	theme: SharedTheme,
	invalid_input: bool,
	max_line: usize,
}

impl GotoLinePopup {
	pub fn new(env: &Environment) -> Self {
		Self {
			visible: false,
			input: String::new(),
			key_config: env.key_config.clone(),
			queue: env.queue.clone(),
			theme: env.theme.clone(),
			invalid_input: false,
			max_line: 0,
			line_number: 0,
		}
	}

	pub fn open(&mut self, max_line: usize) {
		self.visible = true;
		self.max_line = max_line;
	}
}

impl Component for GotoLinePopup {
	///
	fn commands(
		&self,
		out: &mut Vec<CommandInfo>,
		force_all: bool,
	) -> CommandBlocking {
		if self.is_visible() || force_all {
			out.push(
				CommandInfo::new(
					strings::commands::close_popup(&self.key_config),
					true,
					true,
				)
				.order(1),
			);
			out.push(
				CommandInfo::new(
					strings::commands::goto_line(&self.key_config),
					true,
					true,
				)
				.order(1),
			);
		}

		visibility_blocking(self)
	}

	fn is_visible(&self) -> bool {
		self.visible
	}

	///
	fn event(&mut self, event: &Event) -> Result<EventState> {
		if self.is_visible() {
			if let Event::Key(key) = event {
				if key_match(key, self.key_config.keys.exit_popup) {
					self.visible = false;
					self.input.clear();
				} else if let KeyCode::Char(c) = key.code {
					if c.is_ascii_digit() || c == '-' {
						self.input.push(c);
					}
				} else if key.code == KeyCode::Backspace {
					self.input.pop();
				} else if key_match(key, self.key_config.keys.enter) {
					self.visible = false;
					if self.invalid_input {
						self.queue.push(InternalEvent::ShowErrorMsg(
                            format!("Invalid input: only numbers between -{} and {} (included) are allowed (-1 denotes the last line, -2 denotes the second to last line, and so on)",self.max_line + 1, self.max_line))
                            ,
                        );
					} else if !self.input.is_empty() {
						self.queue.push(InternalEvent::GotoLine(
							self.line_number,
						));
					}
					self.input.clear();
					self.invalid_input = false;
				}
			}
			match self.input.parse::<isize>() {
				Ok(input) => {
					let mut max_value_allowed_abs = self.max_line;
					// negative indices are 1 based
					if input < 0 {
						max_value_allowed_abs += 1;
					}
					let input_abs = input.unsigned_abs();
					if input_abs > max_value_allowed_abs {
						self.invalid_input = true;
					} else {
						self.invalid_input = false;
						self.line_number = if input >= 0 {
							input_abs
						} else {
							max_value_allowed_abs - input_abs
						}
					}
				}
				Err(_) => {
					if !self.input.is_empty() {
						self.invalid_input = true;
					}
				}
			}
			return Ok(EventState::Consumed);
		}
		Ok(EventState::NotConsumed)
	}
}

impl DrawableComponent for GotoLinePopup {
	fn draw(&self, f: &mut Frame, area: Rect) -> Result<()> {
		if self.is_visible() {
			let style = if self.invalid_input {
				Style::default().fg(Color::Red)
			} else {
				self.theme.text(true, false)
			};
			let input = Paragraph::new(self.input.as_str())
				.style(style)
				.block(Block::bordered().title("Go to"));

			let input_area = ui::centered_rect_absolute(15, 3, area);
			f.render_widget(Clear, input_area);
			f.render_widget(input, input_area);
		}

		Ok(())
	}
}
