use crate::{
	components::ScrollType,
	ui::{draw_scrollbar, style::SharedTheme, Orientation},
};
use ratatui::{layout::Rect, Frame};
use std::cell::Cell;

pub struct VerticalScroll {
	top: Cell<usize>,
	max_top: Cell<usize>,
	visual_height: Cell<usize>,
}

impl VerticalScroll {
	pub const fn new() -> Self {
		Self {
			top: Cell::new(0),
			max_top: Cell::new(0),
			visual_height: Cell::new(0),
		}
	}

	pub fn get_top(&self) -> usize {
		self.top.get()
	}

	pub fn reset(&self) {
		self.top.set(0);
	}

	pub fn move_top(&self, move_type: ScrollType) -> bool {
		let old = self.top.get();
		let max = self.max_top.get();

		let new_scroll_top = match move_type {
			ScrollType::Down => old.saturating_add(1),
			ScrollType::Up => old.saturating_sub(1),
			ScrollType::PageDown => old
				.saturating_sub(1)
				.saturating_add(self.visual_height.get()),
			ScrollType::PageUp => old
				.saturating_add(1)
				.saturating_sub(self.visual_height.get()),
			ScrollType::Home => 0,
			ScrollType::End => max,
		};

		let new_scroll_top = new_scroll_top.clamp(0, max);

		if new_scroll_top == old {
			return false;
		}

		self.top.set(new_scroll_top);

		true
	}

	pub fn move_area_to_visible(
		&self,
		height: usize,
		start: usize,
		end: usize,
	) {
		let top = self.top.get();
		let bottom = top + height;
		let max_top = self.max_top.get();
		// the top of some content is hidden
		if start < top {
			self.top.set(start);
			return;
		}
		// the bottom of some content is hidden and there is visible space available
		if end > bottom && start > top {
			let avail_space = start.saturating_sub(top);
			let diff = std::cmp::min(
				avail_space,
				end.saturating_sub(bottom),
			);
			let top = top.saturating_add(diff);
			self.top.set(std::cmp::min(max_top, top));
		}
	}

	pub fn update(
		&self,
		selection: usize,
		selection_max: usize,
		visual_height: usize,
	) -> usize {
		self.visual_height.set(visual_height);

		let new_top = calc_scroll_top(
			self.get_top(),
			visual_height,
			selection,
			selection_max,
		);
		self.top.set(new_top);

		if visual_height == 0 {
			self.max_top.set(0);
		} else {
			let new_max = selection_max.saturating_sub(visual_height);
			self.max_top.set(new_max);
		}

		new_top
	}

	pub fn update_no_selection(
		&self,
		line_count: usize,
		visual_height: usize,
	) -> usize {
		self.update(self.get_top(), line_count, visual_height)
	}

	pub fn draw(&self, f: &mut Frame, r: Rect, theme: &SharedTheme) {
		draw_scrollbar(
			f,
			r,
			theme,
			self.max_top.get(),
			self.top.get(),
			Orientation::Vertical,
		);
	}
}

const fn calc_scroll_top(
	current_top: usize,
	height_in_lines: usize,
	selection: usize,
	selection_max: usize,
) -> usize {
	if height_in_lines == 0 {
		return 0;
	}
	if selection_max <= height_in_lines {
		return 0;
	}

	if current_top + height_in_lines <= selection {
		selection.saturating_sub(height_in_lines) + 1
	} else if current_top > selection {
		selection
	} else {
		current_top
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use pretty_assertions::assert_eq;

	#[test]
	fn test_scroll_no_scroll_to_top() {
		assert_eq!(calc_scroll_top(1, 10, 4, 4), 0);
	}

	#[test]
	fn test_scroll_zero_height() {
		assert_eq!(calc_scroll_top(4, 0, 4, 3), 0);
	}

	#[test]
	fn test_scroll_bottom_into_view() {
		let visual_height = 10;
		let line_count = 20;
		let scroll = VerticalScroll::new();
		scroll.max_top.set(line_count - visual_height);

		// intersecting with the bottom of the visible area
		scroll.move_area_to_visible(visual_height, 9, 11);
		assert_eq!(scroll.get_top(), 1);

		// completely below the visible area
		scroll.move_area_to_visible(visual_height, 15, 17);
		assert_eq!(scroll.get_top(), 7);

		// scrolling to the bottom overflow
		scroll.move_area_to_visible(visual_height, 30, 40);
		assert_eq!(scroll.get_top(), 10);
	}

	#[test]
	fn test_scroll_top_into_view() {
		let visual_height = 10;
		let line_count = 20;
		let scroll = VerticalScroll::new();
		scroll.max_top.set(line_count - visual_height);
		scroll.top.set(4);

		// intersecting with the top of the visible area
		scroll.move_area_to_visible(visual_height, 2, 8);
		assert_eq!(scroll.get_top(), 2);

		// completely above the visible area
		scroll.move_area_to_visible(visual_height, 0, 2);
		assert_eq!(scroll.get_top(), 0);
	}

	#[test]
	fn test_scroll_with_pageup_pagedown() {
		let scroll = VerticalScroll::new();
		scroll.max_top.set(10);
		scroll.visual_height.set(8);

		assert!(scroll.move_top(ScrollType::End));
		assert_eq!(scroll.get_top(), 10);

		assert!(!scroll.move_top(ScrollType::PageDown));
		assert_eq!(scroll.get_top(), 10);

		assert!(scroll.move_top(ScrollType::PageUp));
		assert_eq!(scroll.get_top(), 3);

		assert!(scroll.move_top(ScrollType::PageUp));
		assert_eq!(scroll.get_top(), 0);

		assert!(!scroll.move_top(ScrollType::PageUp));
		assert_eq!(scroll.get_top(), 0);
	}
}
