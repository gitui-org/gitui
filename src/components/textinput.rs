use crate::app::Environment;
use crate::components::VerticalScroll;
use crate::keys::key_match;
use crate::ui::Size;
use crate::{
	components::{
		visibility_blocking, CommandBlocking, CommandInfo, Component,
		DrawableComponent, EventState,
	},
	keys::SharedKeyConfig,
	strings,
	ui::{self, style::SharedTheme},
};
use anyhow::Result;
use crossterm::event::{
	Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
};
use ratatui::{
	layout::{Alignment, Rect},
	style::{Color, Modifier, Style},
	text::{Line, Span, Text},
	widgets::{Block, Borders, Clear, Paragraph, WidgetRef},
	Frame,
};
use std::borrow::Cow;
use std::cell::{Cell, OnceCell};
use std::iter::repeat_n;

///
#[derive(PartialEq, Eq)]
pub enum InputType {
	Singleline,
	Multiline,
	Password,
}

#[derive(Clone, Copy)]
enum CursorMove {
	Top,
	Bottom,
	Up,
	Down,
	Back,
	Forward,
	Home,
	End,
	PageUp,
	PageDown,
}

#[derive(Default, PartialEq)]
enum Key {
	#[default]
	Null,
	Up,
	Down,
	Left,
	Right,
	Home,
	End,
	PageUp,
	PageDown,
	Backspace,
	Delete,
	Tab,
	Char(char),
}

#[derive(Default)]
struct Input {
	key: Key,
	ctrl: bool,
	alt: bool,
}

impl From<Event> for Input {
	/// Convert [`crossterm::event::Event`] into [`Input`].
	fn from(event: Event) -> Self {
		match event {
			Event::Key(key) => Self::from(key),
			_ => Self::default(),
		}
	}
}

impl From<KeyCode> for Key {
	/// Convert [`crossterm::event::KeyCode`] into [`Key`].
	fn from(code: KeyCode) -> Self {
		match code {
			KeyCode::Char(c) => Self::Char(c),
			KeyCode::Backspace => Self::Backspace,
			KeyCode::Left => Self::Left,
			KeyCode::Right => Self::Right,
			KeyCode::Up => Self::Up,
			KeyCode::Down => Self::Down,
			KeyCode::Tab => Self::Tab,
			KeyCode::Delete => Self::Delete,
			KeyCode::Home => Self::Home,
			KeyCode::End => Self::End,
			KeyCode::PageUp => Self::PageUp,
			KeyCode::PageDown => Self::PageDown,
			_ => Self::Null,
		}
	}
}

impl From<KeyEvent> for Input {
	/// Convert [`crossterm::event::KeyEvent`] into [`Input`].
	fn from(key: KeyEvent) -> Self {
		if key.kind == KeyEventKind::Release {
			// On Windows or when `crossterm::event::PushKeyboardEnhancementFlags` is set,
			// key release event can be reported. Ignore it. (rhysd/tui-textarea#14)
			return Self::default();
		}

		let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
		let alt = key.modifiers.contains(KeyModifiers::ALT);
		let key = Key::from(key.code);

		Self { key, ctrl, alt }
	}
}

struct TextArea<'a> {
	lines: Vec<String>,
	block: Option<Block<'a>>,
	style: Style,
	/// 0-based (row, column)
	cursor: (usize, usize),
	cursor_style: Style,
	placeholder: String,
	placeholder_style: Style,
	mask_char: Option<char>,
	theme: SharedTheme,
	scroll: VerticalScroll,
}

impl<'a> TextArea<'a> {
	fn new(lines: Vec<String>, theme: SharedTheme) -> Self {
		let lines = if lines.is_empty() {
			vec![String::new()]
		} else {
			lines
		};

		Self {
			lines,
			block: None,
			style: Style::default(),
			cursor: (0, 0),
			cursor_style: Style::default()
				.add_modifier(Modifier::REVERSED),
			placeholder: String::new(),
			placeholder_style: Style::default().fg(Color::DarkGray),
			mask_char: None,
			theme,
			scroll: VerticalScroll::new(),
		}
	}

	#[cfg(test)]
	fn cursor(&mut self) -> (usize, usize) {
		self.cursor
	}

	fn move_cursor(&mut self, cursor_move: CursorMove) {
		let (current_row, current_column) = self.cursor;

		match cursor_move {
			CursorMove::Top => {
				self.cursor =
					(0, current_column.min(self.lines[0].len()));
			}
			CursorMove::Bottom => {
				let last_row = self.lines.len() - 1;

				self.cursor = (
					last_row,
					current_column.min(self.lines[last_row].len()),
				);
			}
			CursorMove::Up => {
				let new_row = current_row.saturating_sub(1);

				self.cursor = (
					new_row,
					current_column.min(self.lines[new_row].len()),
				);
			}
			CursorMove::Down => {
				let new_row =
					(current_row + 1).min(self.lines.len() - 1);

				self.cursor = (
					new_row,
					current_column.min(self.lines[new_row].len()),
				);
			}
			CursorMove::Back => {
				self.cursor =
					(current_row, current_column.saturating_sub(1));
			}
			CursorMove::Forward => {
				self.cursor = (
					current_row,
					(current_column + 1).min(
						self.lines[current_row]
							.char_indices()
							.count(),
					),
				);
			}
			CursorMove::Home => self.cursor = (current_row, 0),
			CursorMove::End => {
				self.cursor = (
					current_row,
					self.lines[current_row].char_indices().count(),
				);
			}
			CursorMove::PageUp => {
				let new_row = current_row
					.saturating_sub(self.scroll.get_visual_height());

				self.cursor = (
					new_row,
					current_column.min(self.lines[new_row].len()),
				);
			}
			CursorMove::PageDown => {
				let new_row = (current_row
					+ self.scroll.get_visual_height())
				.min(self.lines.len().saturating_sub(1));

				self.cursor = (
					new_row,
					current_column.min(self.lines[new_row].len()),
				);
			}
		}
	}

	fn delete_next_char(&mut self) {
		let (current_row, current_column) = self.cursor;
		let current_line = &mut self.lines[current_row];

		if current_column < current_line.len() {
			if let Some((offset, _)) =
				current_line.char_indices().nth(current_column)
			{
				current_line.remove(offset);
			}
		} else if current_row < self.lines.len().saturating_sub(1) {
			let next_line = self.lines.remove(current_row + 1);
			self.lines[current_row].push_str(&next_line);
		} else {
			// We're at the end of the input. Do nothing.
		}
	}

	fn delete_char(&mut self) {
		let (current_row, current_column) = self.cursor;
		let current_line = &mut self.lines[current_row];

		if current_column > 0 {
			if let Some((offset, _)) =
				current_line.char_indices().nth(current_column - 1)
			{
				current_line.remove(offset);
				self.cursor = (current_row, current_column - 1);
			}
		} else if current_row > 0 {
			let current_line = self.lines.remove(current_row);

			let previous_line = &mut self.lines[current_row - 1];
			let previous_line_length =
				previous_line.char_indices().count();

			previous_line.push_str(&current_line);
			self.cursor = (current_row - 1, previous_line_length);
		} else {
			// We're at (0, 0), there's no characters to be deleted. Do nothing.
		}
	}

	fn insert_char(&mut self, char: char) {
		let (current_row, current_column) = self.cursor;
		let current_line = &mut self.lines[current_row];

		let offset = current_line
			.char_indices()
			.nth(current_column)
			.map_or_else(|| current_line.len(), |(i, _)| i);

		current_line.insert(offset, char);
		self.cursor = (current_row, current_column + 1);
	}

	fn set_block(&mut self, block: Block<'a>) {
		self.block = Some(block);
	}

	fn set_style(&mut self, style: Style) {
		self.style = style;
	}

	fn set_placeholder_text(&mut self, placeholder: String) {
		self.placeholder = placeholder;
	}

	fn set_placeholder_style(&mut self, placeholder_style: Style) {
		self.placeholder_style = placeholder_style;
	}

	fn set_cursor_line_style(&mut self, _style: Style) {
		// Do nothing, implement or remove.
	}

	fn set_mask_char(&mut self, mask_char: char) {
		self.mask_char = Some(mask_char);
	}
}

type TextAreaComponent = TextArea<'static>;

// TODO:
// `TextArea` and `TextAreaComponent` likely can be merged.
impl<'a> TextAreaComponent {
	fn insert_newline(&mut self) {
		let (current_row, current_column) = self.cursor;
		let current_line = &self.lines[current_row];

		let offset = current_line
			.char_indices()
			.nth(current_column)
			.map_or_else(|| current_line.len(), |(i, _)| i);

		let new_line = current_line[offset..].to_string();

		self.lines.insert(current_row + 1, new_line);
		self.lines[current_row].truncate(offset);
		self.cursor = (current_row + 1, 0);
	}

	fn lines(&'a self) -> &'a [String] {
		&self.lines
	}

	fn draw_placeholder(&self, f: &mut Frame, rect: Rect) {
		let paragraph = Paragraph::new(Text::from(Line::from(
			self.placeholder.clone(),
		)))
		.style(self.placeholder_style);

		f.render_widget(paragraph, rect);
	}

	fn draw_lines(&self, f: &mut Frame, rect: Rect) {
		let (current_row, current_column) = self.cursor;

		let top = self.scroll.update(
			current_row,
			self.lines.len(),
			rect.height.into(),
		);

		let lines: Vec<_> = self
			.lines
			.iter()
			.enumerate()
			.skip(top)
			.map(|(row, line)| {
				let line: Cow<'_, str> = self.mask_char.map_or_else(
					|| line.into(),
					|mask_char| {
						repeat_n(mask_char, line.chars().count())
							.collect()
					},
				);

				if row == current_row {
					if current_column == line.char_indices().count() {
						return Line::from(vec![
							Span::from(line.clone()),
							Span::styled(" ", self.cursor_style),
						]);
					}

					if let Some((offset, _)) =
						line.char_indices().nth(current_column)
					{
						let (before_cursor, cursor) =
							line.split_at(offset);

						if let Some((next_offset, _)) =
							cursor.char_indices().nth(1)
						{
							let (cursor, after_cursor) =
								cursor.split_at(next_offset);

							return Line::from(vec![
								Span::from(before_cursor.to_string()),
								Span::styled(
									cursor.to_string(),
									self.cursor_style,
								),
								Span::from(after_cursor.to_string()),
							]);
						}

						return Line::from(vec![
							Span::from(before_cursor.to_string()),
							Span::styled(
								cursor.to_string(),
								self.cursor_style,
							),
						]);
					}
				}

				Line::from(line.clone())
			})
			.collect();
		let paragraph =
			Paragraph::new(Text::from(lines)).style(self.style);

		f.render_widget(paragraph, rect);
	}

	fn is_empty(&self) -> bool {
		self.lines == [""]
	}
}

impl DrawableComponent for TextAreaComponent {
	fn draw(&self, f: &mut Frame, rect: Rect) -> Result<()> {
		let inner_rect = self.block.as_ref().map_or(rect, |block| {
			block.render_ref(rect, f.buffer_mut());

			block.inner(rect)
		});

		if self.is_empty() && !self.placeholder.is_empty() {
			self.draw_placeholder(f, inner_rect);
		} else {
			self.draw_lines(f, inner_rect);
		}

		self.scroll.draw(f, rect, &self.theme);

		Ok(())
	}
}

///
pub struct TextInputComponent {
	title: String,
	default_msg: String,
	selected: Option<bool>,
	msg: OnceCell<String>,
	show_char_count: bool,
	theme: SharedTheme,
	key_config: SharedKeyConfig,
	input_type: InputType,
	current_area: Cell<Rect>,
	embed: bool,
	textarea: Option<TextAreaComponent>,
}

impl TextInputComponent {
	///
	pub fn new(
		env: &Environment,
		title: &str,
		default_msg: &str,
		show_char_count: bool,
	) -> Self {
		Self {
			msg: OnceCell::default(),
			theme: env.theme.clone(),
			key_config: env.key_config.clone(),
			show_char_count,
			title: title.to_string(),
			default_msg: default_msg.to_string(),
			selected: None,
			input_type: InputType::Multiline,
			current_area: Cell::new(Rect::default()),
			embed: false,
			textarea: None,
		}
	}

	///
	pub const fn with_input_type(
		mut self,
		input_type: InputType,
	) -> Self {
		self.input_type = input_type;
		self
	}

	///
	pub fn set_input_type(&mut self, input_type: InputType) {
		self.clear();
		self.input_type = input_type;
	}

	/// Clear the `msg`.
	pub fn clear(&mut self) {
		self.msg.take();
		if self.is_visible() {
			self.show_inner_textarea();
		}
	}

	/// Get the `msg`.
	pub fn get_text(&self) -> &str {
		// the fancy footwork with the OnceCell is to allow
		// the reading of msg as a &str.
		// tui_textarea returns its lines to the caller as &[String]
		// gitui wants &str of \n delimited text
		// it would be simple if this was a mut method. You could
		// just load up msg from the lines area and return an &str pointing at it
		// but its not a mut method. So we need to store the text in a OnceCell
		// The methods that change msg call take() on the cell. That makes
		// get_or_init run again

		self.msg.get_or_init(|| {
			self.textarea
				.as_ref()
				.map_or_else(String::new, |ta| ta.lines().join("\n"))
		})
	}

	/// screen area (last time we got drawn)
	pub fn get_area(&self) -> Rect {
		self.current_area.get()
	}

	/// embed into parent draw area
	pub fn embed(&mut self) {
		self.embed = true;
	}

	///
	pub fn enabled(&mut self, enable: bool) {
		self.selected = Some(enable);
	}

	fn show_inner_textarea(&mut self) {
		//	create the textarea and then load it with the text
		//	from self.msg
		let lines: Vec<String> = self
			.msg
			.get()
			.unwrap_or(&String::new())
			.split('\n')
			.map(ToString::to_string)
			.collect();

		self.textarea = Some({
			let mut text_area =
				TextArea::new(lines, self.theme.clone());
			if self.input_type == InputType::Password {
				text_area.set_mask_char('*');
			}

			text_area
				.set_cursor_line_style(self.theme.text(true, false));
			text_area.set_placeholder_text(self.default_msg.clone());
			text_area.set_placeholder_style(
				self.theme
					.text(self.selected.unwrap_or_default(), false),
			);
			text_area.set_style(
				self.theme.text(self.selected.unwrap_or(true), false),
			);

			if !self.embed {
				text_area.set_block(
					Block::default()
						.borders(Borders::ALL)
						.border_style(
							ratatui::style::Style::default()
								.add_modifier(
									ratatui::style::Modifier::BOLD,
								),
						)
						.title(self.title.clone()),
				);
			}
			text_area
		});
	}

	/// Set the `msg`.
	pub fn set_text(&mut self, msg: String) {
		self.msg = msg.into();
		if self.is_visible() {
			self.show_inner_textarea();
		}
	}

	/// Set the `title`.
	pub fn set_title(&mut self, t: String) {
		self.title = t;
	}

	///
	pub fn set_default_msg(&mut self, v: String) {
		self.default_msg = v;
		if self.is_visible() {
			self.show_inner_textarea();
		}
	}

	fn draw_char_count(&self, f: &mut Frame, r: Rect) {
		let count = self.get_text().len();
		if count > 0 {
			let w = Paragraph::new(format!("[{count} chars]"))
				.alignment(Alignment::Right);

			let mut rect = {
				let mut rect = r;
				rect.y += rect.height.saturating_sub(1);
				rect
			};

			rect.x += 1;
			rect.width = rect.width.saturating_sub(2);
			rect.height = rect
				.height
				.saturating_sub(rect.height.saturating_sub(1));

			f.render_widget(w, rect);
		}
	}

	#[allow(clippy::too_many_lines, clippy::unnested_or_patterns)]
	fn process_inputs(ta: &mut TextArea, input: &Input) -> bool {
		match input {
			Input {
				key: Key::Char(c),
				ctrl: false,
				alt: false,
				..
			} => {
				ta.insert_char(*c);
				true
			}
			Input {
				key: Key::Char('h'),
				ctrl: true,
				alt: false,
				..
			}
			| Input {
				key: Key::Backspace,
				ctrl: false,
				alt: false,
				..
			} => {
				ta.delete_char();
				true
			}
			Input {
				key: Key::Char('d'),
				ctrl: true,
				alt: false,
				..
			}
			| Input {
				key: Key::Delete,
				ctrl: false,
				alt: false,
				..
			} => {
				ta.delete_next_char();
				true
			}
			Input {
				key: Key::Char('n'),
				ctrl: true,
				alt: false,
				..
			}
			| Input {
				key: Key::Down,
				ctrl: false,
				alt: false,
				..
			} => {
				ta.move_cursor(CursorMove::Down);
				true
			}
			Input {
				key: Key::Char('p'),
				ctrl: true,
				alt: false,
				..
			}
			| Input {
				key: Key::Up,
				ctrl: false,
				alt: false,
				..
			} => {
				ta.move_cursor(CursorMove::Up);
				true
			}
			Input {
				key: Key::Char('f'),
				ctrl: true,
				alt: false,
				..
			}
			| Input {
				key: Key::Right,
				ctrl: false,
				alt: false,
				..
			} => {
				ta.move_cursor(CursorMove::Forward);
				true
			}
			Input {
				key: Key::Char('b'),
				ctrl: true,
				alt: false,
				..
			}
			| Input {
				key: Key::Left,
				ctrl: false,
				alt: false,
				..
			} => {
				ta.move_cursor(CursorMove::Back);
				true
			}
			Input {
				key: Key::Char('a'),
				ctrl: true,
				alt: false,
				..
			}
			| Input { key: Key::Home, .. }
			| Input {
				key: Key::Left | Key::Char('b'),
				ctrl: true,
				alt: true,
				..
			} => {
				ta.move_cursor(CursorMove::Home);
				true
			}
			Input {
				key: Key::Char('e'),
				ctrl: true,
				alt: false,
				..
			}
			| Input { key: Key::End, .. }
			| Input {
				key: Key::Right | Key::Char('f'),
				ctrl: true,
				alt: true,
				..
			} => {
				ta.move_cursor(CursorMove::End);
				true
			}
			Input {
				key: Key::Char('<'),
				ctrl: false,
				alt: true,
				..
			}
			| Input {
				key: Key::Up | Key::Char('p'),
				ctrl: true,
				alt: true,
				..
			} => {
				ta.move_cursor(CursorMove::Top);
				true
			}
			Input {
				key: Key::Char('>'),
				ctrl: false,
				alt: true,
				..
			}
			| Input {
				key: Key::Down | Key::Char('n'),
				ctrl: true,
				alt: true,
				..
			} => {
				ta.move_cursor(CursorMove::Bottom);
				true
			}

			Input {
				key: Key::Char('v'),
				ctrl: true,
				alt: false,
				..
			}
			| Input {
				key: Key::PageDown, ..
			} => {
				ta.move_cursor(CursorMove::PageDown);
				true
			}
			Input {
				key: Key::Char('v'),
				ctrl: false,
				alt: true,
				..
			}
			| Input {
				key: Key::PageUp, ..
			} => {
				ta.move_cursor(CursorMove::PageUp);
				true
			}
			_ => false,
		}
	}
}

impl DrawableComponent for TextInputComponent {
	fn draw(&self, f: &mut Frame, rect: Rect) -> Result<()> {
		// this should always be true since draw should only be being called
		// is control is visible
		if let Some(ta) = &self.textarea {
			let area = if self.embed {
				rect
			} else if self.input_type == InputType::Multiline {
				let area = ui::centered_rect(60, 20, f.area());
				ui::rect_inside(
					Size::new(10, 3),
					f.area().into(),
					area,
				)
			} else {
				let area = ui::centered_rect(60, 1, f.area());

				ui::rect_inside(
					Size::new(10, 3),
					f.area().into(),
					area,
				)
			};

			f.render_widget(Clear, area);

			ta.draw(f, area)?;

			if self.show_char_count {
				self.draw_char_count(f, area);
			}

			self.current_area.set(area);
		}
		Ok(())
	}
}

impl Component for TextInputComponent {
	fn commands(
		&self,
		out: &mut Vec<CommandInfo>,
		_force_all: bool,
	) -> CommandBlocking {
		out.push(
			CommandInfo::new(
				strings::commands::close_popup(&self.key_config),
				true,
				self.is_visible(),
			)
			.order(1),
		);

		//TODO: we might want to show the textarea specific commands here

		visibility_blocking(self)
	}

	fn event(&mut self, ev: &Event) -> Result<EventState> {
		let input = Input::from(ev.clone());

		if let Some(ta) = &mut self.textarea {
			let modified = if let Event::Key(e) = ev {
				if key_match(e, self.key_config.keys.exit_popup) {
					self.hide();
					return Ok(EventState::Consumed);
				}

				if key_match(e, self.key_config.keys.newline)
					&& self.input_type == InputType::Multiline
				{
					ta.insert_newline();
					true
				} else {
					Self::process_inputs(ta, &input)
				}
			} else {
				false
			};

			if modified {
				self.msg.take();
				return Ok(EventState::Consumed);
			}
		}

		Ok(EventState::NotConsumed)
	}

	/*
	  visible maps to textarea Option
	  None = > not visible
	  Some => visible
	*/
	fn is_visible(&self) -> bool {
		self.textarea.is_some()
	}

	fn hide(&mut self) {
		self.textarea = None;
	}

	fn show(&mut self) -> Result<()> {
		self.show_inner_textarea();
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_smoke() {
		let env = Environment::test_env();
		let mut comp = TextInputComponent::new(&env, "", "", false);
		comp.show_inner_textarea();
		comp.set_text(String::from("ab\nb"));
		assert!(comp.is_visible());

		if let Some(ta) = &mut comp.textarea {
			assert_eq!(ta.cursor(), (0, 0));

			ta.move_cursor(CursorMove::Forward);
			assert_eq!(ta.cursor(), (0, 1));

			ta.move_cursor(CursorMove::Back);
			assert_eq!(ta.cursor(), (0, 0));
		}
	}

	#[test]
	fn text_cursor_initial_position() {
		let env = Environment::test_env();
		let mut comp = TextInputComponent::new(&env, "", "", false);
		comp.show_inner_textarea();
		comp.set_text(String::from("a"));
		assert!(comp.is_visible());

		if let Some(ta) = &mut comp.textarea {
			let txt = ta.lines();
			assert_eq!(txt[0].len(), 1);
			assert_eq!(txt[0].as_bytes()[0], b'a');
		}
	}

	#[test]
	fn test_multiline() {
		let env = Environment::test_env();
		let mut comp = TextInputComponent::new(&env, "", "", false);
		comp.show_inner_textarea();
		comp.set_text(String::from("a\nb\nc"));
		assert!(comp.is_visible());

		if let Some(ta) = &mut comp.textarea {
			let txt = ta.lines();
			assert_eq!(txt[0], "a");
			assert_eq!(txt[1], "b");
			assert_eq!(txt[2], "c");
		}
	}

	#[test]
	fn test_move_cursor_horizontally() {
		let env = Environment::test_env();
		let mut comp = TextInputComponent::new(&env, "", "", false);
		comp.show_inner_textarea();
		comp.set_text(String::from("aa b;c"));
		assert!(comp.is_visible());

		if let Some(ta) = &mut comp.textarea {
			ta.move_cursor(CursorMove::Home);
			assert_eq!(ta.cursor(), (0, 0));

			ta.move_cursor(CursorMove::Forward);
			assert_eq!(ta.cursor(), (0, 1));

			ta.move_cursor(CursorMove::Forward);
			assert_eq!(ta.cursor(), (0, 2));

			ta.move_cursor(CursorMove::End);
			assert_eq!(ta.cursor(), (0, 6));

			ta.move_cursor(CursorMove::Back);
			assert_eq!(ta.cursor(), (0, 5));

			ta.move_cursor(CursorMove::Back);
			assert_eq!(ta.cursor(), (0, 4));
		}
	}

	#[test]
	fn test_move_cursor_vertically() {
		let env = Environment::test_env();
		let mut comp = TextInputComponent::new(&env, "", "", false);
		comp.show_inner_textarea();
		comp.set_text(String::from("aa \nd\ngitui"));
		assert!(comp.is_visible());

		if let Some(ta) = &mut comp.textarea {
			ta.move_cursor(CursorMove::Bottom);
			assert_eq!(ta.cursor(), (2, 0));

			ta.move_cursor(CursorMove::Up);
			assert_eq!(ta.cursor(), (1, 0));

			ta.move_cursor(CursorMove::Up);
			assert_eq!(ta.cursor(), (0, 0));

			ta.move_cursor(CursorMove::Bottom);
			assert_eq!(ta.cursor(), (2, 0));

			ta.move_cursor(CursorMove::Top);
			assert_eq!(ta.cursor(), (0, 0));

			ta.move_cursor(CursorMove::Down);
			assert_eq!(ta.cursor(), (1, 0));

			ta.move_cursor(CursorMove::Down);
			assert_eq!(ta.cursor(), (2, 0));

			ta.move_cursor(CursorMove::Down);
			assert_eq!(ta.cursor(), (2, 0));

			ta.move_cursor(CursorMove::End);
			assert_eq!(ta.cursor(), (2, 5));

			ta.move_cursor(CursorMove::Up);
			assert_eq!(ta.cursor(), (1, 1));

			ta.move_cursor(CursorMove::Bottom);
			ta.move_cursor(CursorMove::End);
			assert_eq!(ta.cursor(), (2, 5));

			ta.move_cursor(CursorMove::Top);
			assert_eq!(ta.cursor(), (0, 3));
		}
	}

	#[test]
	fn test_move_cursor_vertically_page_up_down() {
		let env = Environment::test_env();
		let mut comp = TextInputComponent::new(&env, "", "", false);
		comp.show_inner_textarea();
		comp.set_text(String::from(
			"aa \nd\ngitui\nasdf\ndf\ndfsdf\nsdfsdfsdfsdf",
		));
		assert!(comp.is_visible());

		let test_backend =
			ratatui::backend::TestBackend::new(100, 100);
		let mut terminal = ratatui::Terminal::new(test_backend)
			.expect("Unable to set up terminal");
		let mut frame = terminal.get_frame();
		let rect = Rect::new(0, 0, 10, 5);

		// we call draw once before running the actual test as the component only learns its dimensions
		// in a `draw` call. It needs to learn its dimensions because `PageUp` and `PageDown` rely on
		// them for calculating how far to move the cursor.
		comp.draw(&mut frame, rect).expect("draw not to fail");

		if let Some(ta) = &mut comp.textarea {
			ta.move_cursor(CursorMove::PageDown);
			assert_eq!(ta.cursor(), (6, 0));

			ta.move_cursor(CursorMove::PageUp);
			assert_eq!(ta.cursor(), (0, 0));
		}
	}

	#[test]
	fn test_insert_newline() {
		let env = Environment::test_env();
		let mut comp = TextInputComponent::new(&env, "", "", false);
		comp.show_inner_textarea();
		comp.set_text(String::from("aa b;c asdf asdf"));
		assert!(comp.is_visible());

		if let Some(ta) = &mut comp.textarea {
			ta.move_cursor(CursorMove::Forward);
			ta.move_cursor(CursorMove::Forward);
			ta.move_cursor(CursorMove::Forward);
			assert_eq!(ta.cursor(), (0, 3));

			ta.insert_newline();

			assert_eq!(ta.lines(), &["aa ", "b;c asdf asdf"]);
			assert_eq!(ta.cursor(), (1, 0));

			ta.move_cursor(CursorMove::Forward);
			ta.move_cursor(CursorMove::Forward);
			ta.move_cursor(CursorMove::Forward);
			assert_eq!(ta.cursor(), (1, 3));

			ta.insert_newline();

			assert_eq!(ta.lines(), &["aa ", "b;c", " asdf asdf"]);
			assert_eq!(ta.cursor(), (2, 0));
		}
	}

	#[test]
	fn test_insert_newline_unicode() {
		let env = Environment::test_env();
		let mut comp = TextInputComponent::new(&env, "", "", false);
		comp.show_inner_textarea();
		comp.set_text(String::from("äaä b;ö üü"));
		assert!(comp.is_visible());

		if let Some(ta) = &mut comp.textarea {
			ta.move_cursor(CursorMove::Forward);
			ta.move_cursor(CursorMove::Forward);
			ta.move_cursor(CursorMove::Forward);
			assert_eq!(ta.cursor(), (0, 3));

			ta.insert_newline();

			assert_eq!(ta.lines(), &["äaä", " b;ö üü"]);
			assert_eq!(ta.cursor(), (1, 0));

			ta.move_cursor(CursorMove::Forward);
			ta.move_cursor(CursorMove::Forward);
			ta.move_cursor(CursorMove::Forward);
			assert_eq!(ta.cursor(), (1, 3));

			ta.insert_newline();

			assert_eq!(ta.lines(), &["äaä", " b;", "ö üü"]);
			assert_eq!(ta.cursor(), (2, 0));
		}
	}

	#[test]
	fn test_delete_char() {
		let env = Environment::test_env();
		let mut comp = TextInputComponent::new(&env, "", "", false);
		comp.show_inner_textarea();
		comp.set_text(String::from("aa b;c\ndef sa\ngitui"));
		assert!(comp.is_visible());

		if let Some(ta) = &mut comp.textarea {
			ta.move_cursor(CursorMove::Bottom);
			ta.move_cursor(CursorMove::End);
			assert_eq!(ta.cursor(), (2, 5));

			ta.delete_char();
			assert_eq!(ta.lines(), &["aa b;c", "def sa", "gitu"]);
			assert_eq!(ta.cursor(), (2, 4));

			ta.delete_char();
			assert_eq!(ta.lines(), &["aa b;c", "def sa", "git"]);
			assert_eq!(ta.cursor(), (2, 3));

			ta.delete_char();
			assert_eq!(ta.lines(), &["aa b;c", "def sa", "gi"]);
			assert_eq!(ta.cursor(), (2, 2));

			ta.delete_char();
			assert_eq!(ta.lines(), &["aa b;c", "def sa", "g"]);
			assert_eq!(ta.cursor(), (2, 1));

			ta.delete_char();
			assert_eq!(ta.lines(), &["aa b;c", "def sa", ""]);
			assert_eq!(ta.cursor(), (2, 0));

			ta.delete_char();
			assert_eq!(ta.lines(), &["aa b;c", "def sa"]);
			assert_eq!(ta.cursor(), (1, 6));

			ta.delete_char();
			assert_eq!(ta.lines(), &["aa b;c", "def s"]);
			assert_eq!(ta.cursor(), (1, 5));

			ta.delete_char();
			assert_eq!(ta.lines(), &["aa b;c", "def "]);
			assert_eq!(ta.cursor(), (1, 4));

			ta.insert_char('g');
			assert_eq!(ta.lines(), &["aa b;c", "def g"]);
			assert_eq!(ta.cursor(), (1, 5));
		}
	}

	#[test]
	fn test_delete_char_unicode() {
		let env = Environment::test_env();
		let mut comp = TextInputComponent::new(&env, "", "", false);
		comp.show_inner_textarea();
		comp.set_text(String::from("äÜö"));
		assert!(comp.is_visible());

		if let Some(ta) = &mut comp.textarea {
			ta.move_cursor(CursorMove::End);
			assert_eq!(ta.cursor(), (0, 3));

			ta.delete_char();
			assert_eq!(ta.lines(), &["äÜ"]);
			assert_eq!(ta.cursor(), (0, 2));
		}
	}

	#[test]
	fn test_delete_char_cursor_position() {
		let env = Environment::test_env();
		let mut comp = TextInputComponent::new(&env, "", "", false);
		comp.show_inner_textarea();
		comp.set_text(String::from("aasd\nfdfsd\nölkj"));
		assert!(comp.is_visible());

		if let Some(ta) = &mut comp.textarea {
			ta.move_cursor(CursorMove::Bottom);
			assert_eq!(ta.cursor(), (2, 0));

			ta.delete_char();
			assert_eq!(ta.lines(), &["aasd", "fdfsdölkj"]);
			assert_eq!(ta.cursor(), (1, 5));
		}
	}

	#[test]
	fn test_delete_next_char() {
		let env = Environment::test_env();
		let mut comp = TextInputComponent::new(&env, "", "", false);
		comp.show_inner_textarea();
		comp.set_text(String::from("aa\ndef sa\ngitui"));
		assert!(comp.is_visible());

		if let Some(ta) = &mut comp.textarea {
			assert_eq!(ta.cursor(), (0, 0));

			ta.delete_next_char();
			assert_eq!(ta.lines(), &["a", "def sa", "gitui"]);
			assert_eq!(ta.cursor(), (0, 0));

			ta.delete_next_char();
			assert_eq!(ta.lines(), &["", "def sa", "gitui"]);
			assert_eq!(ta.cursor(), (0, 0));

			ta.delete_next_char();
			assert_eq!(ta.lines(), &["def sa", "gitui"]);
			assert_eq!(ta.cursor(), (0, 0));

			ta.move_cursor(CursorMove::Down);
			assert_eq!(ta.cursor(), (1, 0));

			ta.delete_next_char();
			assert_eq!(ta.lines(), &["def sa", "itui"]);
			assert_eq!(ta.cursor(), (1, 0));
		}
	}

	#[test]
	fn test_delete_next_char_empty() {
		let env = Environment::test_env();
		let mut comp = TextInputComponent::new(&env, "", "", false);
		comp.show_inner_textarea();
		comp.set_text("".into());
		assert!(comp.is_visible());

		if let Some(ta) = &mut comp.textarea {
			assert_eq!(ta.cursor(), (0, 0));

			ta.delete_next_char();
			assert_eq!(ta.lines(), &[""]);
			assert_eq!(ta.cursor(), (0, 0));
		}
	}

	#[test]
	fn test_delete_next_char_unicode() {
		let env = Environment::test_env();
		let mut comp = TextInputComponent::new(&env, "", "", false);
		comp.show_inner_textarea();
		comp.set_text("üäu".into());
		assert!(comp.is_visible());

		if let Some(ta) = &mut comp.textarea {
			ta.move_cursor(CursorMove::Forward);
			assert_eq!(ta.cursor(), (0, 1));

			ta.delete_next_char();
			assert_eq!(ta.lines(), &["üu"]);
			assert_eq!(ta.cursor(), (0, 1));
		}
	}
}
