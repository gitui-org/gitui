use super::style::SharedTheme;
use easy_cast::CastFloat;
use ratatui::{
	buffer::Buffer,
	layout::{Margin, Rect},
	style::Style,
	symbols::{
		block::FULL,
		line::{DOUBLE_HORIZONTAL, DOUBLE_VERTICAL},
	},
	widgets::Widget,
	Frame,
};

pub enum Orientation {
	Vertical,
	Horizontal,
}

///
struct Scrollbar {
	max: u16,
	pos: u16,
	style_bar: Style,
	style_pos: Style,
	orientation: Orientation,
}

impl Scrollbar {
	fn new(max: usize, pos: usize, orientation: Orientation) -> Self {
		Self {
			max: u16::try_from(max).unwrap_or_default(),
			pos: u16::try_from(pos).unwrap_or_default(),
			style_pos: Style::default(),
			style_bar: Style::default(),
			orientation,
		}
	}

	fn render_vertical(self, area: Rect, buf: &mut Buffer) {
		if area.height <= 2 {
			return;
		}

		if self.max == 0 {
			return;
		}

		let right = area.right().saturating_sub(1);
		if right <= area.left() {
			return;
		}

		let (bar_top, bar_height) = {
			let scrollbar_area = area.inner(Margin {
				horizontal: 0,
				vertical: 1,
			});

			(scrollbar_area.top(), scrollbar_area.height)
		};

		for y in bar_top..(bar_top + bar_height) {
			buf.set_string(right, y, DOUBLE_VERTICAL, self.style_bar);
		}

		let progress = f32::from(self.pos) / f32::from(self.max);
		let progress = if progress > 1.0 { 1.0 } else { progress };
		let pos = f32::from(bar_height) * progress;

		let pos: u16 = pos.cast_nearest();
		let pos = pos.saturating_sub(1);

		buf.set_string(right, bar_top + pos, FULL, self.style_pos);
	}

	fn render_horizontal(self, area: Rect, buf: &mut Buffer) {
		if area.width <= 2 {
			return;
		}

		if self.max == 0 {
			return;
		}

		let bottom = area.bottom().saturating_sub(1);
		if bottom <= area.top() {
			return;
		}

		let (bar_left, bar_width) = {
			let scrollbar_area = area.inner(Margin {
				horizontal: 1,
				vertical: 0,
			});

			(scrollbar_area.left(), scrollbar_area.width)
		};

		for x in bar_left..(bar_left + bar_width) {
			buf.set_string(
				x,
				bottom,
				DOUBLE_HORIZONTAL,
				self.style_bar,
			);
		}

		let progress = f32::from(self.pos) / f32::from(self.max);
		let progress = if progress > 1.0 { 1.0 } else { progress };
		let pos = f32::from(bar_width) * progress;

		let pos: u16 = pos.cast_nearest();
		let pos = pos.saturating_sub(1);

		buf.set_string(bar_left + pos, bottom, FULL, self.style_pos);
	}
}

impl Widget for Scrollbar {
	fn render(self, area: Rect, buf: &mut Buffer) {
		match &self.orientation {
			Orientation::Vertical => self.render_vertical(area, buf),
			Orientation::Horizontal => {
				self.render_horizontal(area, buf);
			}
		}
	}
}

pub fn draw_scrollbar(
	f: &mut Frame,
	r: Rect,
	theme: &SharedTheme,
	max: usize,
	pos: usize,
	orientation: Orientation,
) {
	let mut widget = Scrollbar::new(max, pos, orientation);
	widget.style_pos = theme.scroll_bar_pos();
	f.render_widget(widget, r);
}
