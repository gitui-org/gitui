use crate::components::{
	visibility_blocking, CommandBlocking, CommandInfo, Component,
	DrawableComponent, EventState, ScrollType, VerticalScroll,
};
use crate::strings::order;
use crate::{
	app::Environment,
	keys::{key_match, SharedKeyConfig},
	strings, ui,
};
use anyhow::Result;
use crossterm::event::Event;
use ratatui::text::Line;
use ratatui::{
	layout::{Alignment, Rect},
	text::Span,
	widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
	Frame,
};
use ui::style::SharedTheme;

pub struct MsgPopup {
	title: String,
	msg: String,
	visible: bool,
	theme: SharedTheme,
	key_config: SharedKeyConfig,
	scroll: VerticalScroll,
}

const POPUP_HEIGHT: u16 = 25;
const BORDER_WIDTH: u16 = 2;
const MINIMUM_WIDTH: u16 = 60;

impl DrawableComponent for MsgPopup {
	fn draw(&self, f: &mut Frame, _rect: Rect) -> Result<()> {
		if !self.visible {
			return Ok(());
		}

		let max_width = f.area().width.max(MINIMUM_WIDTH);

		// determine the maximum width of text block
		let width = self
			.msg
			.lines()
			.map(str::len)
			.max()
			.unwrap_or(0)
			.saturating_add(BORDER_WIDTH.into())
			.clamp(MINIMUM_WIDTH.into(), max_width.into())
			.try_into()
			.expect("can't fail because we're clamping to u16 value");

		let area =
			ui::centered_rect_absolute(width, POPUP_HEIGHT, f.area());

		// Wrap lines and break words if there is not enough space
		let wrapped_msg = bwrap::wrap_maybrk!(
			&self.msg,
			area.width.saturating_sub(BORDER_WIDTH).into()
		);

		let msg_lines: Vec<String> =
			wrapped_msg.lines().map(String::from).collect();
		let line_num = msg_lines.len();

		let height = POPUP_HEIGHT
			.saturating_sub(BORDER_WIDTH)
			.min(f.area().height.saturating_sub(BORDER_WIDTH));

		let top =
			self.scroll.update_no_selection(line_num, height.into());

		let scrolled_lines = msg_lines
			.iter()
			.skip(top)
			.take(height.into())
			.map(|line| {
				Line::from(vec![Span::styled(
					line.clone(),
					self.theme.text(true, false),
				)])
			})
			.collect::<Vec<Line>>();

		f.render_widget(Clear, area);
		f.render_widget(
			Paragraph::new(scrolled_lines)
				.block(
					Block::default()
						.title(Span::styled(
							self.title.as_str(),
							self.theme.text_danger(),
						))
						.borders(Borders::ALL)
						.border_type(BorderType::Thick),
				)
				.alignment(Alignment::Left)
				.wrap(Wrap { trim: true }),
			area,
		);

		self.scroll.draw(f, area, &self.theme);

		Ok(())
	}
}

impl Component for MsgPopup {
	fn commands(
		&self,
		out: &mut Vec<CommandInfo>,
		_force_all: bool,
	) -> CommandBlocking {
		out.push(CommandInfo::new(
			strings::commands::close_popup(&self.key_config),
			true,
			self.visible,
		));
		out.push(
			CommandInfo::new(
				strings::commands::navigate_commit_message(
					&self.key_config,
				),
				true,
				self.visible,
			)
			.order(order::NAV),
		);

		visibility_blocking(self)
	}

	fn event(&mut self, ev: &Event) -> Result<EventState> {
		if self.visible {
			if let Event::Key(e) = ev {
				if key_match(e, self.key_config.keys.exit_popup) {
					self.hide();
				} else if key_match(
					e,
					self.key_config.keys.popup_down,
				) {
					self.scroll.move_top(ScrollType::Down);
				} else if key_match(e, self.key_config.keys.popup_up)
				{
					self.scroll.move_top(ScrollType::Up);
				}
			}
			Ok(EventState::Consumed)
		} else {
			Ok(EventState::NotConsumed)
		}
	}

	fn is_visible(&self) -> bool {
		self.visible
	}

	fn hide(&mut self) {
		self.visible = false;
	}

	fn show(&mut self) -> Result<()> {
		self.visible = true;

		Ok(())
	}
}

impl MsgPopup {
	pub fn new(env: &Environment) -> Self {
		Self {
			title: String::new(),
			msg: String::new(),
			visible: false,
			theme: env.theme.clone(),
			key_config: env.key_config.clone(),
			scroll: VerticalScroll::new(),
		}
	}

	fn set_new_msg(
		&mut self,
		msg: &str,
		title: String,
	) -> Result<()> {
		self.title = title;
		self.msg = msg.to_string();
		self.scroll.reset();
		self.show()
	}

	///
	pub fn show_error(&mut self, msg: &str) -> Result<()> {
		self.set_new_msg(
			msg,
			strings::msg_title_error(&self.key_config),
		)
	}

	///
	pub fn show_info(&mut self, msg: &str) -> Result<()> {
		self.set_new_msg(
			msg,
			strings::msg_title_info(&self.key_config),
		)
	}
}
