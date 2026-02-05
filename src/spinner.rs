use ratatui::{backend::Backend, Terminal};
use std::{cell::Cell, char, io};

// static SPINNER_CHARS: &[char] = &['◢', '◣', '◤', '◥'];
// static SPINNER_CHARS: &[char] = &['⢹', '⢺', '⢼', '⣸', '⣇', '⡧', '⡗', '⡏'];
static SPINNER_CHARS: &[char] =
	&['⣷', '⣯', '⣟', '⡿', '⢿', '⣻', '⣽', '⣾'];

///
pub struct Spinner {
	idx: usize,
	active: bool,
	last_char: Cell<char>,
}

impl Default for Spinner {
	fn default() -> Self {
		Self {
			idx: 0,
			active: false,
			last_char: Cell::new(' '),
		}
	}
}

impl Spinner {
	/// increment spinner graphic by one
	pub fn update(&mut self) {
		self.idx += 1;
		self.idx %= SPINNER_CHARS.len();
	}

	///
	pub fn set_state(&mut self, active: bool) {
		self.active = active;
	}

	/// draws or removes spinner char depending on `pending` state
	pub fn draw<B: ratatui::backend::Backend>(
		&self,
		terminal: &mut Terminal<B>,
	) -> io::Result<()> {
		let idx = self.idx;

		let char_to_draw =
			if self.active { SPINNER_CHARS[idx] } else { ' ' };

		if self.last_char.get() != char_to_draw {
			self.last_char.set(char_to_draw);

			let c = ratatui::buffer::Cell::default()
				.set_char(char_to_draw)
				.clone();

			terminal
				.backend_mut()
				.draw(vec![(0_u16, 0_u16, &c)].into_iter())?;

			Backend::flush(terminal.backend_mut())?;
		}

		Ok(())
	}
}
