use crate::{
	components::HorizontalScrollType,
	ui::{draw_scrollbar, style::SharedTheme, Orientation},
};
use ratatui::{layout::Rect, Frame};
use std::cell::Cell;

pub struct HorizontalScroll {
	right: Cell<usize>,
	max_right: Cell<usize>,
}

impl HorizontalScroll {
	pub const fn new() -> Self {
		Self {
			right: Cell::new(0),
			max_right: Cell::new(0),
		}
	}

	pub const fn get_right(&self) -> usize {
		self.right.get()
	}

	pub fn reset(&self) {
		self.right.set(0);
	}

	pub fn move_right(
		&self,
		move_type: HorizontalScrollType,
	) -> bool {
		let old = self.right.get();
		let max = self.max_right.get();

		let new_scroll_right = match move_type {
			HorizontalScrollType::Left => old.saturating_sub(1),
			HorizontalScrollType::Right => old.saturating_add(1),
		};

		let new_scroll_right = new_scroll_right.clamp(0, max);

		if new_scroll_right == old {
			return false;
		}

		self.right.set(new_scroll_right);

		true
	}

	pub fn update(
		&self,
		selection: usize,
		max_selection: usize,
		visual_width: usize,
	) -> usize {
		let new_right = calc_scroll_right(
			self.get_right(),
			visual_width,
			selection,
			max_selection,
		);
		self.right.set(new_right);

		if visual_width == 0 {
			self.max_right.set(0);
		} else {
			let new_max_right =
				max_selection.saturating_sub(visual_width);
			self.max_right.set(new_max_right);
		}

		new_right
	}

	pub fn update_no_selection(
		&self,
		column_count: usize,
		visual_width: usize,
	) -> usize {
		self.update(self.get_right(), column_count, visual_width)
	}

	pub fn draw(&self, f: &mut Frame, r: Rect, theme: &SharedTheme) {
		draw_scrollbar(
			f,
			r,
			theme,
			self.max_right.get(),
			self.right.get(),
			Orientation::Horizontal,
		);
	}
}

const fn calc_scroll_right(
	current_right: usize,
	width_in_lines: usize,
	selection: usize,
	selection_max: usize,
) -> usize {
	if width_in_lines == 0 {
		return 0;
	}
	if selection_max <= width_in_lines {
		return 0;
	}

	if current_right + width_in_lines <= selection {
		selection.saturating_sub(width_in_lines) + 1
	} else if current_right > selection {
		selection
	} else {
		current_right
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use pretty_assertions::assert_eq;

	#[test]
	fn test_scroll_no_scroll_to_right() {
		assert_eq!(calc_scroll_right(1, 10, 4, 4), 0);
	}

	#[test]
	fn test_scroll_zero_width() {
		assert_eq!(calc_scroll_right(4, 0, 4, 3), 0);
	}
}
