//! Debounce arrow-key edge navigation against key autorepeat.

use super::key_list::GituiKeyEvent;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::{
	collections::HashMap,
	time::{Duration, Instant},
};

const DEFAULT_COOLDOWN: Duration = Duration::from_millis(500);

#[derive(Hash, Eq, PartialEq, Clone, Copy)]
struct KeyId {
	code: KeyCode,
	modifiers: KeyModifiers,
}

impl From<GituiKeyEvent> for KeyId {
	fn from(key: GituiKeyEvent) -> Self {
		Self {
			code: key.code,
			modifiers: key.modifiers,
		}
	}
}

///
pub struct KeyRepeatGuard {
	last: HashMap<KeyId, Instant>,
	cooldown: Duration,
}

impl KeyRepeatGuard {
	///
	pub fn new() -> Self {
		Self {
			last: HashMap::new(),
			cooldown: DEFAULT_COOLDOWN,
		}
	}

	#[cfg(test)]
	pub fn with_cooldown(cooldown: Duration) -> Self {
		Self {
			last: HashMap::new(),
			cooldown,
		}
	}

	///
	pub fn record(&mut self, key: GituiKeyEvent) {
		self.last.insert(key.into(), Instant::now());
	}

	///
	pub fn record_key_event(&mut self, key: &KeyEvent) {
		self.record(GituiKeyEvent {
			code: key.code,
			modifiers: key.modifiers,
		});
	}

	/// Whether edge navigation (leaving a scrollable view) should run now.
	pub fn allow_edge_navigation(&self, key: GituiKeyEvent) -> bool {
		self.last
			.get(&key.into())
			.is_none_or(|t| Instant::now().duration_since(*t) >= self.cooldown)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crossterm::event::{KeyCode, KeyModifiers};
	use std::thread::sleep;

	#[test]
	fn test_blocks_rapid_repeats() {
		let mut guard = KeyRepeatGuard::with_cooldown(Duration::from_millis(50));
		let key =
			GituiKeyEvent::new(KeyCode::Up, KeyModifiers::empty());

		assert!(guard.allow_edge_navigation(key));
		guard.record(key);
		assert!(!guard.allow_edge_navigation(key));

		sleep(Duration::from_millis(60));
		assert!(guard.allow_edge_navigation(key));
	}
}
