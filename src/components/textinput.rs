use crate::app::Environment;
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
use crossterm::event::Event;
use ratatui::widgets::{Block, Borders};
use ratatui::{
	layout::{Alignment, Rect},
	widgets::{Clear, Paragraph},
	Frame,
};
use std::cell::Cell;
use std::cell::OnceCell;
use tui_textarea::{CursorMove, Input, Key, Scrolling, TextArea};

///
#[derive(PartialEq, Eq)]
pub enum InputType {
	Singleline,
	Multiline,
	Password,
}

#[derive(PartialEq, Eq)]
enum SelectionState {
	Selecting,
	NotSelecting,
	SelectionEndPending,
}

type TextAreaComponent = TextArea<'static>;

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
	select_state: SelectionState,
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
			select_state: SelectionState::NotSelecting,
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
			let mut text_area = TextArea::new(lines);
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

	fn should_select(&mut self, input: &Input) {
		if input.key == Key::Null {
			return;
		}
		// Should we start selecting text, stop the current selection, or do nothing?
		// the end is handled after the ending keystroke

		match (&self.select_state, input.shift) {
			(SelectionState::Selecting, true)
			| (SelectionState::NotSelecting, false) => {
				// continue selecting or not selecting
			}
			(SelectionState::Selecting, false) => {
				// end select
				self.select_state =
					SelectionState::SelectionEndPending;
			}
			(SelectionState::NotSelecting, true) => {
				// start select
				// this should always work since we are only called
				// if we have a textarea to get input
				if let Some(ta) = &mut self.textarea {
					ta.start_selection();
					self.select_state = SelectionState::Selecting;
				}
			}
			(SelectionState::SelectionEndPending, _) => {
				// this really should not happen because the end pending state
				// should have been picked up in the same pass as it was set
				// so lets clear it
				self.select_state = SelectionState::NotSelecting;
			}
		}
	}

	#[allow(clippy::too_many_lines, clippy::unnested_or_patterns)]
	fn process_inputs(ta: &mut TextArea<'_>, input: &Input) -> bool {
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
				key: Key::Tab,
				ctrl: false,
				alt: false,
				..
			} => {
				ta.insert_tab();
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
				key: Key::Char('k'),
				ctrl: true,
				alt: false,
				..
			} => {
				ta.delete_line_by_end();
				true
			}
			Input {
				key: Key::Char('j'),
				ctrl: true,
				alt: false,
				..
			} => {
				ta.delete_line_by_head();
				true
			}
			Input {
				key: Key::Char('w'),
				ctrl: true,
				alt: false,
				..
			}
			| Input {
				key: Key::Char('h'),
				ctrl: false,
				alt: true,
				..
			}
			| Input {
				key: Key::Backspace,
				ctrl: false,
				alt: true,
				..
			} => {
				ta.delete_word();
				true
			}
			Input {
				key: Key::Delete,
				ctrl: false,
				alt: true,
				..
			}
			| Input {
				key: Key::Char('d'),
				ctrl: false,
				alt: true,
				..
			} => {
				ta.delete_next_word();
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
				ta.move_cursor(CursorMove::Head);
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
				key: Key::Char('f'),
				ctrl: false,
				alt: true,
				..
			}
			| Input {
				key: Key::Right,
				ctrl: true,
				alt: false,
				..
			} => {
				ta.move_cursor(CursorMove::WordForward);
				true
			}
			Input {
				key: Key::Char('b'),
				ctrl: false,
				alt: true,
				..
			}
			| Input {
				key: Key::Left,
				ctrl: true,
				alt: false,
				..
			} => {
				ta.move_cursor(CursorMove::WordBack);
				true
			}

			Input {
				key: Key::Char(']'),
				ctrl: false,
				alt: true,
				..
			}
			| Input {
				key: Key::Char('n'),
				ctrl: false,
				alt: true,
				..
			}
			| Input {
				key: Key::Down,
				ctrl: true,
				alt: false,
				..
			} => {
				ta.move_cursor(CursorMove::ParagraphForward);
				true
			}
			Input {
				key: Key::Char('['),
				ctrl: false,
				alt: true,
				..
			}
			| Input {
				key: Key::Char('p'),
				ctrl: false,
				alt: true,
				..
			}
			| Input {
				key: Key::Up,
				ctrl: true,
				alt: false,
				..
			} => {
				ta.move_cursor(CursorMove::ParagraphBack);
				true
			}
			Input {
				key: Key::Char('u'),
				ctrl: true,
				alt: false,
				..
			} => {
				ta.undo();
				true
			}
			Input {
				key: Key::Char('r'),
				ctrl: true,
				alt: false,
				..
			} => {
				ta.redo();
				true
			}
			Input {
				key: Key::Char('y'),
				ctrl: true,
				alt: false,
				..
			} => {
				ta.paste();
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
				ta.scroll(Scrolling::PageDown);
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
				ta.scroll(Scrolling::PageUp);
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

			f.render_widget(ta, area);

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
		self.should_select(&input);
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

			if self.select_state
				== SelectionState::SelectionEndPending
			{
				ta.cancel_selection();
				self.select_state = SelectionState::NotSelecting;
			}

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
		comp.set_text(String::from("a\nb"));
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
	fn test_next_word_position() {
		let env = Environment::test_env();
		let mut comp = TextInputComponent::new(&env, "", "", false);
		comp.show_inner_textarea();
		comp.set_text(String::from("aa b;c"));
		assert!(comp.is_visible());
		if let Some(ta) = &mut comp.textarea {
			// from word start
			ta.move_cursor(CursorMove::Head);
			ta.move_cursor(CursorMove::WordForward);
			assert_eq!(ta.cursor(), (0, 3));
			// from inside start
			ta.move_cursor(CursorMove::Forward);
			ta.move_cursor(CursorMove::WordForward);
			assert_eq!(ta.cursor(), (0, 5));
			// to string end
			ta.move_cursor(CursorMove::Forward);
			ta.move_cursor(CursorMove::WordForward);
			assert_eq!(ta.cursor(), (0, 6));

			// from string end
			ta.move_cursor(CursorMove::Forward);
			let save_cursor = ta.cursor();
			ta.move_cursor(CursorMove::WordForward);
			assert_eq!(ta.cursor(), save_cursor);
		}
	}

	#[test]
	fn test_previous_word_position() {
		let env = Environment::test_env();
		let mut comp = TextInputComponent::new(&env, "", "", false);
		comp.show_inner_textarea();
		comp.set_text(String::from(" a bb;c"));
		assert!(comp.is_visible());

		if let Some(ta) = &mut comp.textarea {
			// from string end
			ta.move_cursor(CursorMove::End);
			ta.move_cursor(CursorMove::WordBack);
			assert_eq!(ta.cursor(), (0, 6));
			// from inside word
			ta.move_cursor(CursorMove::Back);
			ta.move_cursor(CursorMove::WordBack);
			assert_eq!(ta.cursor(), (0, 3));
			// from word start
			ta.move_cursor(CursorMove::WordBack);
			assert_eq!(ta.cursor(), (0, 1));
			// to string start
			ta.move_cursor(CursorMove::WordBack);
			assert_eq!(ta.cursor(), (0, 0));
			// from string start
			let save_cursor = ta.cursor();
			ta.move_cursor(CursorMove::WordBack);

			assert_eq!(ta.cursor(), save_cursor);
		}
	}

	#[test]
	fn test_next_word_multibyte() {
		let env = Environment::test_env();
		let mut comp = TextInputComponent::new(&env, "", "", false);
		// should emojis be word boundaries or not?
		// various editors (vs code, vim) do not agree with the
		// behavhior of the original textinput here.
		//
		// tui-textarea agrees with them.
		// So these tests are changed to match that behavior
		// FYI: this line is "a à ❤ab🤯 a"

		//              "01245       89A        EFG"
		let text = dbg!("a à \u{2764}ab\u{1F92F} a");
		comp.show_inner_textarea();
		comp.set_text(String::from(text));
		assert!(comp.is_visible());

		if let Some(ta) = &mut comp.textarea {
			ta.move_cursor(CursorMove::Head);
			ta.move_cursor(CursorMove::WordForward);
			assert_eq!(ta.cursor(), (0, 2));
			ta.move_cursor(CursorMove::WordForward);
			assert_eq!(ta.cursor(), (0, 4));
			ta.move_cursor(CursorMove::WordForward);
			assert_eq!(ta.cursor(), (0, 9));
			ta.move_cursor(CursorMove::WordForward);
			assert_eq!(ta.cursor(), (0, 10));
			let save_cursor = ta.cursor();
			ta.move_cursor(CursorMove::WordForward);
			assert_eq!(ta.cursor(), save_cursor);

			ta.move_cursor(CursorMove::End);
			ta.move_cursor(CursorMove::WordBack);
			assert_eq!(ta.cursor(), (0, 9));
			ta.move_cursor(CursorMove::WordBack);
			assert_eq!(ta.cursor(), (0, 4));
			ta.move_cursor(CursorMove::WordBack);
			assert_eq!(ta.cursor(), (0, 2));
			ta.move_cursor(CursorMove::WordBack);
			assert_eq!(ta.cursor(), (0, 0));
			let save_cursor = ta.cursor();
			ta.move_cursor(CursorMove::WordBack);
			assert_eq!(ta.cursor(), save_cursor);
		}
	}
}
