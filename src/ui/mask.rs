use ratatui::style::{Color, Style};
use ratatui::widgets::Widget;
use ratatui::Frame;
use ratatui::{buffer::Buffer, layout::Rect};

struct Mask;

impl Widget for Mask {
	fn render(self, area: Rect, buf: &mut Buffer) {
		for y in area.top()..area.bottom() {
			for x in area.left()..area.right() {
				if let Some(cell) = buf.cell_mut((x, y)) {
					// TODO(prprabhu): What do we want here?
					// Question 1: Set background color vs foreground color
					// Question 2: What color should we set to? The total number
					//   of colors available to us across all backends is
					//   limited, and we can't really tint the theme colors due
					//   to the limited number of colors available.
					// Question 3: Should we pick a reasonable color and add it
					//   to the theme?
					cell.set_style(
						Style::default().fg(Color::DarkGray),
					);
				}
			}
		}
	}
}

pub fn draw_mask(frame: &mut Frame, rect: Rect) {
	let mask = Mask;
	frame.render_widget(mask, rect);
}
