use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::{fs::canonicalize, path::PathBuf, rc::Rc};

use crate::{args::get_app_config_path, strings::symbol};

use super::{
	key_list::{key_match, GituiKeyEvent, KeysList},
	symbols::KeySymbols,
};

pub type SharedKeyConfig = Rc<KeyConfig>;
const KEY_LIST_FILENAME: &str = "key_bindings.ron";
const KEY_SYMBOLS_FILENAME: &str = "key_symbols.ron";

#[derive(Default, Clone)]
pub struct KeyConfig {
	pub keys: KeysList,
	symbols: KeySymbols,
}

impl KeyConfig {
	fn get_config_file() -> Result<PathBuf> {
		let app_home = get_app_config_path()?;
		let config_file = app_home.join(KEY_LIST_FILENAME);
		canonicalize(&config_file)
			.map_or_else(|_| Ok(config_file), Ok)
	}

	fn get_symbols_file() -> Result<PathBuf> {
		let app_home = get_app_config_path()?;
		let symbols_file = app_home.join(KEY_SYMBOLS_FILENAME);
		canonicalize(&symbols_file)
			.map_or_else(|_| Ok(symbols_file), Ok)
	}

	pub fn init(
		key_bindings_path: Option<&PathBuf>,
		key_symbols_path: Option<&PathBuf>,
	) -> Result<Self> {
		let keys = KeysList::init(
			key_bindings_path
				.unwrap_or(&Self::get_config_file()?)
				.clone(),
		);
		let symbols = KeySymbols::init(
			key_symbols_path
				.unwrap_or(&Self::get_symbols_file()?)
				.clone(),
		);

		Ok(Self { keys, symbols })
	}

	pub fn is_nav_up(&self, key: &KeyEvent) -> bool {
		key_match(key, self.keys.move_up)
			|| (key.code == KeyCode::Char('k')
				&& key.modifiers == KeyModifiers::NONE)
	}

	pub fn is_nav_down(&self, key: &KeyEvent) -> bool {
		key_match(key, self.keys.move_down)
			|| (key.code == KeyCode::Char('j')
				&& key.modifiers == KeyModifiers::NONE)
	}

	pub fn is_nav_right(&self, key: &KeyEvent) -> bool {
		key_match(key, self.keys.move_right)
			|| (key.code == KeyCode::Char('l')
				&& key.modifiers == KeyModifiers::NONE)
	}

	fn get_key_symbol(&self, k: KeyCode) -> &str {
		match k {
			KeyCode::Enter => &self.symbols.enter,
			KeyCode::Left => &self.symbols.left,
			KeyCode::Right => &self.symbols.right,
			KeyCode::Up => &self.symbols.up,
			KeyCode::Down => &self.symbols.down,
			KeyCode::Backspace => &self.symbols.backspace,
			KeyCode::Home => &self.symbols.home,
			KeyCode::End => &self.symbols.end,
			KeyCode::PageUp => &self.symbols.page_up,
			KeyCode::PageDown => &self.symbols.page_down,
			KeyCode::Tab => &self.symbols.tab,
			KeyCode::BackTab => &self.symbols.back_tab,
			KeyCode::Delete => &self.symbols.delete,
			KeyCode::Insert => &self.symbols.insert,
			KeyCode::Esc => &self.symbols.esc,
			_ => "?",
		}
	}

	pub fn get_hint(&self, ev: GituiKeyEvent) -> String {
		match ev.code {
			KeyCode::Down
			| KeyCode::Up
			| KeyCode::Right
			| KeyCode::Left
			| KeyCode::Enter
			| KeyCode::Backspace
			| KeyCode::Home
			| KeyCode::End
			| KeyCode::PageUp
			| KeyCode::PageDown
			| KeyCode::Tab
			| KeyCode::BackTab
			| KeyCode::Delete
			| KeyCode::Insert
			| KeyCode::Esc => {
				format!(
					"{}{}",
					self.get_modifier_hint(ev.modifiers),
					self.get_key_symbol(ev.code)
				)
			}
			KeyCode::Char(' ') => String::from(symbol::SPACE),
			KeyCode::Char(c) => {
				format!(
					"{}{}",
					self.get_modifier_hint(ev.modifiers),
					c
				)
			}
			KeyCode::F(u) => {
				format!(
					"{}F{}",
					self.get_modifier_hint(ev.modifiers),
					u
				)
			}
			KeyCode::Null => {
				self.get_modifier_hint(ev.modifiers).into()
			}
			_ => String::new(),
		}
	}

	fn get_modifier_hint(&self, modifier: KeyModifiers) -> &str {
		match modifier {
			KeyModifiers::CONTROL => &self.symbols.control,
			KeyModifiers::SHIFT => &self.symbols.shift,
			KeyModifiers::ALT => &self.symbols.alt,
			_ => "",
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::fs;
	use std::io::Write;
	use tempfile::NamedTempFile;

	fn key_event(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
		KeyEvent::new(code, modifiers)
	}

	#[test]
	fn test_vim_nav_up() {
		let config = KeyConfig::default();
		assert!(config
			.is_nav_up(&key_event(KeyCode::Up, KeyModifiers::NONE)));
		assert!(config.is_nav_up(&key_event(
			KeyCode::Char('k'),
			KeyModifiers::NONE
		)));
		assert!(!config.is_nav_up(&key_event(
			KeyCode::Char('k'),
			KeyModifiers::CONTROL
		)));
		assert!(!config.is_nav_up(&key_event(
			KeyCode::Char('j'),
			KeyModifiers::NONE
		)));
	}

	#[test]
	fn test_vim_nav_down() {
		let config = KeyConfig::default();
		assert!(config.is_nav_down(&key_event(
			KeyCode::Down,
			KeyModifiers::NONE
		)));
		assert!(config.is_nav_down(&key_event(
			KeyCode::Char('j'),
			KeyModifiers::NONE
		)));
		assert!(!config.is_nav_down(&key_event(
			KeyCode::Char('j'),
			KeyModifiers::CONTROL
		)));
		assert!(!config.is_nav_down(&key_event(
			KeyCode::Char('k'),
			KeyModifiers::NONE
		)));
	}

	#[test]
	fn test_vim_nav_right() {
		let config = KeyConfig::default();
		assert!(config.is_nav_right(&key_event(
			KeyCode::Right,
			KeyModifiers::NONE
		)));
		assert!(config.is_nav_right(&key_event(
			KeyCode::Char('l'),
			KeyModifiers::NONE
		)));
		assert!(!config.is_nav_right(&key_event(
			KeyCode::Char('l'),
			KeyModifiers::CONTROL
		)));
		assert!(!config.is_nav_right(&key_event(
			KeyCode::Char('j'),
			KeyModifiers::NONE
		)));
	}

	#[test]
	fn test_get_hint() {
		let config = KeyConfig::default();
		let h = config.get_hint(GituiKeyEvent::new(
			KeyCode::Char('c'),
			KeyModifiers::CONTROL,
		));
		assert_eq!(h, "^c");
	}

	#[test]
	fn test_symbolic_links() {
		let app_home = get_app_config_path().unwrap();
		fs::create_dir_all(&app_home).unwrap();
		// save current config
		let original_key_list_path = app_home.join(KEY_LIST_FILENAME);
		let renamed_key_list = if original_key_list_path.exists() {
			let temp = NamedTempFile::new_in(&app_home).unwrap();
			fs::rename(&original_key_list_path, &temp).unwrap();
			Some(temp)
		} else {
			None
		};
		let original_key_symbols_path =
			app_home.join(KEY_SYMBOLS_FILENAME);
		let renamed_key_symbols = if original_key_symbols_path
			.exists()
		{
			let temp = NamedTempFile::new_in(&app_home).unwrap();
			fs::rename(&original_key_symbols_path, &temp).unwrap();
			Some(temp)
		} else {
			None
		};

		// create temporary config files
		let mut temporary_key_list =
			NamedTempFile::new_in(&app_home).unwrap();
		writeln!(
			temporary_key_list,
			r#"
(
	move_down: Some(( code: Char('j'), modifiers: "CONTROL")),
)
"#
		)
		.unwrap();

		let mut temporary_key_symbols =
			NamedTempFile::new_in(&app_home).unwrap();
		writeln!(
			temporary_key_symbols,
			r#"
(
	esc: Some("Esc"),
)
"#
		)
		.unwrap();

		// testing
		let result = std::panic::catch_unwind(|| {
			let loaded_config = KeyConfig::init(None, None).unwrap();
			assert_eq!(
				loaded_config.keys.move_down,
				KeysList::default().move_down
			);
			assert_eq!(
				loaded_config.symbols.esc,
				KeySymbols::default().esc
			);

			create_symlink(
				&temporary_key_symbols,
				&original_key_symbols_path,
			)
			.unwrap();
			let loaded_config = KeyConfig::init(None, None).unwrap();
			assert_eq!(
				loaded_config.keys.move_down,
				KeysList::default().move_down
			);
			assert_eq!(loaded_config.symbols.esc, "Esc");

			create_symlink(
				&temporary_key_list,
				&original_key_list_path,
			)
			.unwrap();
			let loaded_config = KeyConfig::init(None, None).unwrap();
			assert_eq!(
				loaded_config.keys.move_down,
				GituiKeyEvent::new(
					KeyCode::Char('j'),
					KeyModifiers::CONTROL
				)
			);
			assert_eq!(loaded_config.symbols.esc, "Esc");

			fs::remove_file(&original_key_symbols_path).unwrap();
			let loaded_config = KeyConfig::init(None, None).unwrap();
			assert_eq!(
				loaded_config.keys.move_down,
				GituiKeyEvent::new(
					KeyCode::Char('j'),
					KeyModifiers::CONTROL
				)
			);
			assert_eq!(
				loaded_config.symbols.esc,
				KeySymbols::default().esc
			);

			fs::remove_file(&original_key_list_path).unwrap();
		});

		// remove symlinks from testing if they still exist
		let _ = fs::remove_file(&original_key_list_path);
		let _ = fs::remove_file(&original_key_symbols_path);

		// restore original config files
		if let Some(temp) = renamed_key_list {
			let _ = fs::rename(&temp, &original_key_list_path);
		}

		if let Some(temp) = renamed_key_symbols {
			let _ = fs::rename(&temp, &original_key_symbols_path);
		}

		assert!(result.is_ok());
	}

	#[cfg(not(target_os = "windows"))]
	fn create_symlink<
		P: AsRef<std::path::Path>,
		Q: AsRef<std::path::Path>,
	>(
		original: P,
		link: Q,
	) -> Result<(), std::io::Error> {
		std::os::unix::fs::symlink(original, link)
	}

	#[cfg(target_os = "windows")]
	fn create_symlink<
		P: AsRef<std::path::Path>,
		Q: AsRef<std::path::Path>,
	>(
		original: P,
		link: Q,
	) -> Result<(), std::io::Error> {
		std::os::windows::fs::symlink_file(original, link)
	}
}
