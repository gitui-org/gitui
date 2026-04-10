mod reflow;
mod scrollbar;
mod scrolllist;
mod stateful_paragraph;
pub mod style;
mod syntax_text;

use filetreelist::MoveSelection;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
pub use scrollbar::{draw_scrollbar, Orientation};
pub use scrolllist::{draw_list, draw_list_block};
pub use stateful_paragraph::{
	ParagraphState, ScrollPos, StatefulParagraph,
};
pub use syntax_text::{AsyncSyntaxJob, SyntaxText};

use crate::keys::{key_match, SharedKeyConfig};

/// return the scroll position (line) necessary to have the `selection` in view if it is not already
pub const fn calc_scroll_top(
	current_top: usize,
	height_in_lines: usize,
	selection: usize,
) -> usize {
	if current_top.saturating_add(height_in_lines) <= selection {
		selection.saturating_sub(height_in_lines) + 1
	} else if current_top > selection {
		selection
	} else {
		current_top
	}
}

/// ui component size representation
#[derive(Copy, Clone)]
pub struct Size {
	pub width: u16,
	pub height: u16,
}

impl Size {
	pub const fn new(width: u16, height: u16) -> Self {
		Self { width, height }
	}
}

impl From<Rect> for Size {
	fn from(r: Rect) -> Self {
		Self {
			width: r.width,
			height: r.height,
		}
	}
}

/// use layouts to create a rects that
/// centers inside `r` and sizes `percent_x`/`percent_x` of `r`
pub fn centered_rect(
	percent_x: u16,
	percent_y: u16,
	r: Rect,
) -> Rect {
	let popup_layout = Layout::default()
		.direction(Direction::Vertical)
		.constraints(
			[
				Constraint::Percentage((100 - percent_y) / 2),
				Constraint::Percentage(percent_y),
				Constraint::Percentage((100 - percent_y) / 2),
			]
			.as_ref(),
		)
		.split(r);

	Layout::default()
		.direction(Direction::Horizontal)
		.constraints(
			[
				Constraint::Percentage((100 - percent_x) / 2),
				Constraint::Percentage(percent_x),
				Constraint::Percentage((100 - percent_x) / 2),
			]
			.as_ref(),
		)
		.split(popup_layout[1])[1]
}

/// makes sure Rect `r` at least stays as big as min and not bigger than max
pub fn rect_inside(min: Size, max: Size, r: Rect) -> Rect {
	let new_width = if min.width > max.width {
		max.width
	} else {
		r.width.clamp(min.width, max.width)
	};

	let new_height = if min.height > max.height {
		max.height
	} else {
		r.height.clamp(min.height, max.height)
	};

	let diff_width = new_width.saturating_sub(r.width);
	let diff_height = new_height.saturating_sub(r.height);

	Rect::new(
		r.x.saturating_sub(diff_width / 2),
		r.y.saturating_sub(diff_height / 2),
		new_width,
		new_height,
	)
}

pub fn centered_rect_absolute(
	width: u16,
	height: u16,
	r: Rect,
) -> Rect {
	Rect::new(
		(r.width.saturating_sub(width)) / 2,
		(r.height.saturating_sub(height)) / 2,
		width.min(r.width),
		height.min(r.height),
	)
}

///
pub fn common_nav(
	key: &crossterm::event::KeyEvent,
	key_config: &SharedKeyConfig,
) -> Option<MoveSelection> {
	if key_match(key, key_config.keys.move_down) {
		Some(MoveSelection::Down)
	} else if key_match(key, key_config.keys.move_up) {
		Some(MoveSelection::Up)
	} else if key_match(key, key_config.keys.page_up) {
		Some(MoveSelection::PageUp)
	} else if key_match(key, key_config.keys.page_down) {
		Some(MoveSelection::PageDown)
	} else if key_match(key, key_config.keys.move_right) {
		Some(MoveSelection::Right)
	} else if key_match(key, key_config.keys.move_left) {
		Some(MoveSelection::Left)
	} else if key_match(key, key_config.keys.home)
		|| key_match(key, key_config.keys.shift_up)
	{
		Some(MoveSelection::Top)
	} else if key_match(key, key_config.keys.end)
		|| key_match(key, key_config.keys.shift_down)
	{
		Some(MoveSelection::End)
	} else {
		None
	}
}

#[cfg(test)]
mod test {
	use super::{rect_inside, Size};
	use pretty_assertions::assert_eq;
	use ratatui::layout::Rect;

	#[test]
	fn test_small_rect_in_rect() {
		let rect = rect_inside(
			Size {
				width: 2,
				height: 2,
			},
			Size {
				width: 1,
				height: 1,
			},
			Rect {
				x: 0,
				y: 0,
				width: 10,
				height: 10,
			},
		);

		assert_eq!(
			rect,
			Rect {
				x: 0,
				y: 0,
				width: 1,
				height: 1
			}
		);
	}

	#[test]
	fn test_small_rect_in_rect2() {
		let rect = rect_inside(
			Size {
				width: 1,
				height: 3,
			},
			Size {
				width: 1,
				height: 2,
			},
			Rect {
				x: 0,
				y: 0,
				width: 10,
				height: 10,
			},
		);

		assert_eq!(
			rect,
			Rect {
				x: 0,
				y: 0,
				width: 1,
				height: 2
			}
		);
	}
}
