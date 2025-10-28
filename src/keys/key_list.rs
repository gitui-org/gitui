use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::{Deserialize, Serialize};
use std::{fs::File, path::PathBuf};
use struct_patch::traits::Patch as PatchTrait;
use struct_patch::Patch;

#[derive(Debug, PartialOrd, Clone, Copy, Serialize, Deserialize)]
pub struct GituiKeyEvent {
	pub code: KeyCode,
	pub modifiers: KeyModifiers,
}

impl GituiKeyEvent {
	pub const fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
		Self { code, modifiers }
	}
}

pub fn key_match(ev: &KeyEvent, binding: GituiKeyEvent) -> bool {
	ev.code == binding.code && ev.modifiers == binding.modifiers
}

impl PartialEq for GituiKeyEvent {
	fn eq(&self, other: &Self) -> bool {
		let ev: KeyEvent = self.into();
		let other: KeyEvent = other.into();
		ev == other
	}
}

impl From<&GituiKeyEvent> for KeyEvent {
	fn from(other: &GituiKeyEvent) -> Self {
		Self::new(other.code, other.modifiers)
	}
}

#[derive(Debug, Clone, Patch)]
#[patch(attribute(derive(Deserialize, Debug)))]
pub struct KeysList {
	pub tab_status: GituiKeyEvent,
	pub tab_log: GituiKeyEvent,
	pub tab_files: GituiKeyEvent,
	pub tab_stashing: GituiKeyEvent,
	pub tab_stashes: GituiKeyEvent,
	pub tab_toggle: GituiKeyEvent,
	pub tab_toggle_reverse: GituiKeyEvent,
	pub toggle_workarea: GituiKeyEvent,
	pub exit: GituiKeyEvent,
	pub quit: GituiKeyEvent,
	pub exit_popup: GituiKeyEvent,
	pub open_commit: GituiKeyEvent,
	pub open_commit_editor: GituiKeyEvent,
	pub open_help: GituiKeyEvent,
	pub open_options: GituiKeyEvent,
	pub move_left: GituiKeyEvent,
	pub move_right: GituiKeyEvent,
	pub move_up: GituiKeyEvent,
	pub move_down: GituiKeyEvent,
	pub tree_collapse_recursive: GituiKeyEvent,
	pub tree_expand_recursive: GituiKeyEvent,
	pub home: GituiKeyEvent,
	pub end: GituiKeyEvent,
	pub popup_up: GituiKeyEvent,
	pub popup_down: GituiKeyEvent,
	pub page_down: GituiKeyEvent,
	pub page_up: GituiKeyEvent,
	pub shift_up: GituiKeyEvent,
	pub shift_down: GituiKeyEvent,
	pub enter: GituiKeyEvent,
	pub blame: GituiKeyEvent,
	pub file_history: GituiKeyEvent,
	pub edit_file: GituiKeyEvent,
	pub status_stage_all: GituiKeyEvent,
	pub status_reset_item: GituiKeyEvent,
	pub status_ignore_file: GituiKeyEvent,
	pub diff_stage_lines: GituiKeyEvent,
	pub diff_reset_lines: GituiKeyEvent,
	pub stashing_save: GituiKeyEvent,
	pub stashing_toggle_untracked: GituiKeyEvent,
	pub stashing_toggle_index: GituiKeyEvent,
	pub stash_apply: GituiKeyEvent,
	pub stash_open: GituiKeyEvent,
	pub stash_drop: GituiKeyEvent,
	pub cmd_bar_toggle: GituiKeyEvent,
	pub log_tag_commit: GituiKeyEvent,
	pub log_mark_commit: GituiKeyEvent,
	pub log_checkout_commit: GituiKeyEvent,
	pub log_reset_commit: GituiKeyEvent,
	pub log_reword_commit: GituiKeyEvent,
	pub log_find: GituiKeyEvent,
	pub find_commit_sha: GituiKeyEvent,
	pub commit_amend: GituiKeyEvent,
	pub toggle_signoff: GituiKeyEvent,
	pub toggle_verify: GituiKeyEvent,
	pub copy: GituiKeyEvent,
	pub create_branch: GituiKeyEvent,
	pub rename_branch: GituiKeyEvent,
	pub select_branch: GituiKeyEvent,
	pub delete_branch: GituiKeyEvent,
	pub merge_branch: GituiKeyEvent,
	pub rebase_branch: GituiKeyEvent,
	pub reset_branch: GituiKeyEvent,
	pub compare_commits: GituiKeyEvent,
	pub tags: GituiKeyEvent,
	pub delete_tag: GituiKeyEvent,
	pub select_tag: GituiKeyEvent,
	pub push: GituiKeyEvent,
	pub open_file_tree: GituiKeyEvent,
	pub file_find: GituiKeyEvent,
	pub branch_find: GituiKeyEvent,
	pub force_push: GituiKeyEvent,
	pub fetch: GituiKeyEvent,
	pub pull: GituiKeyEvent,
	pub abort_merge: GituiKeyEvent,
	pub undo_commit: GituiKeyEvent,
	pub diff_hunk_next: GituiKeyEvent,
	pub diff_hunk_prev: GituiKeyEvent,
	pub stage_unstage_item: GituiKeyEvent,
	pub tag_annotate: GituiKeyEvent,
	pub view_submodules: GituiKeyEvent,
	pub view_remotes: GituiKeyEvent,
	pub update_remote_name: GituiKeyEvent,
	pub update_remote_url: GituiKeyEvent,
	pub add_remote: GituiKeyEvent,
	pub delete_remote: GituiKeyEvent,
	pub view_submodule_parent: GituiKeyEvent,
	pub update_submodule: GituiKeyEvent,
	pub commit_history_next: GituiKeyEvent,
	pub commit: GituiKeyEvent,
	pub newline: GituiKeyEvent,
}

#[rustfmt::skip]
impl Default for KeysList {
	fn default() -> Self {
		Self {
			tab_status: GituiKeyEvent::new(KeyCode::Char('1'), KeyModifiers::empty()),
			tab_log: GituiKeyEvent::new(KeyCode::Char('2'),  KeyModifiers::empty()),
			tab_files: GituiKeyEvent::new(KeyCode::Char('3'),  KeyModifiers::empty()),
			tab_stashing: GituiKeyEvent::new(KeyCode::Char('4'),  KeyModifiers::empty()),
			tab_stashes: GituiKeyEvent::new(KeyCode::Char('5'),  KeyModifiers::empty()),
			tab_toggle: GituiKeyEvent::new(KeyCode::Tab,  KeyModifiers::empty()),
			tab_toggle_reverse: GituiKeyEvent::new(KeyCode::BackTab,  KeyModifiers::SHIFT),
			toggle_workarea: GituiKeyEvent::new(KeyCode::Char('w'),  KeyModifiers::empty()),
			exit: GituiKeyEvent::new(KeyCode::Char('c'),  KeyModifiers::CONTROL),
			quit: GituiKeyEvent::new(KeyCode::Char('q'),  KeyModifiers::empty()),
			exit_popup: GituiKeyEvent::new(KeyCode::Esc,  KeyModifiers::empty()),
			open_commit: GituiKeyEvent::new(KeyCode::Char('c'),  KeyModifiers::empty()),
			open_commit_editor: GituiKeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL),
			open_help: GituiKeyEvent::new(KeyCode::Char('h'),  KeyModifiers::empty()),
			open_options: GituiKeyEvent::new(KeyCode::Char('o'),  KeyModifiers::empty()),
			move_left: GituiKeyEvent::new(KeyCode::Left,  KeyModifiers::empty()),
			move_right: GituiKeyEvent::new(KeyCode::Right,  KeyModifiers::empty()),
			tree_collapse_recursive: GituiKeyEvent::new(KeyCode::Left,  KeyModifiers::SHIFT),
			tree_expand_recursive: GituiKeyEvent::new(KeyCode::Right,  KeyModifiers::SHIFT),
			home: GituiKeyEvent::new(KeyCode::Home,  KeyModifiers::empty()),
			end: GituiKeyEvent::new(KeyCode::End,  KeyModifiers::empty()),
			move_up: GituiKeyEvent::new(KeyCode::Up,  KeyModifiers::empty()),
			move_down: GituiKeyEvent::new(KeyCode::Down,  KeyModifiers::empty()),
			popup_up: GituiKeyEvent::new(KeyCode::Up,  KeyModifiers::empty()),
			popup_down: GituiKeyEvent::new(KeyCode::Down,  KeyModifiers::empty()),
			page_down: GituiKeyEvent::new(KeyCode::PageDown,  KeyModifiers::empty()),
			page_up: GituiKeyEvent::new(KeyCode::PageUp,  KeyModifiers::empty()),
			shift_up: GituiKeyEvent::new(KeyCode::Up,  KeyModifiers::SHIFT),
			shift_down: GituiKeyEvent::new(KeyCode::Down,  KeyModifiers::SHIFT),
			enter: GituiKeyEvent::new(KeyCode::Enter,  KeyModifiers::empty()),
			blame: GituiKeyEvent::new(KeyCode::Char('B'),  KeyModifiers::SHIFT),
			file_history: GituiKeyEvent::new(KeyCode::Char('H'),  KeyModifiers::SHIFT),
			edit_file: GituiKeyEvent::new(KeyCode::Char('e'),  KeyModifiers::empty()),
			status_stage_all: GituiKeyEvent::new(KeyCode::Char('a'),  KeyModifiers::empty()),
			status_reset_item: GituiKeyEvent::new(KeyCode::Char('D'),  KeyModifiers::SHIFT),
			diff_reset_lines: GituiKeyEvent::new(KeyCode::Char('d'),  KeyModifiers::empty()),
			status_ignore_file: GituiKeyEvent::new(KeyCode::Char('i'),  KeyModifiers::empty()),
			diff_stage_lines: GituiKeyEvent::new(KeyCode::Char('s'),  KeyModifiers::empty()),
			stashing_save: GituiKeyEvent::new(KeyCode::Char('s'),  KeyModifiers::empty()),
			stashing_toggle_untracked: GituiKeyEvent::new(KeyCode::Char('u'),  KeyModifiers::empty()),
			stashing_toggle_index: GituiKeyEvent::new(KeyCode::Char('i'),  KeyModifiers::empty()),
			stash_apply: GituiKeyEvent::new(KeyCode::Char('a'),  KeyModifiers::empty()),
			stash_open: GituiKeyEvent::new(KeyCode::Right,  KeyModifiers::empty()),
			stash_drop: GituiKeyEvent::new(KeyCode::Char('D'),  KeyModifiers::SHIFT),
			cmd_bar_toggle: GituiKeyEvent::new(KeyCode::Char('.'),  KeyModifiers::empty()),
			log_tag_commit: GituiKeyEvent::new(KeyCode::Char('t'),  KeyModifiers::empty()),
			log_mark_commit: GituiKeyEvent::new(KeyCode::Char(' '),  KeyModifiers::empty()),
			log_checkout_commit: GituiKeyEvent { code: KeyCode::Char('S'), modifiers: KeyModifiers::SHIFT },
			log_reset_commit: GituiKeyEvent { code: KeyCode::Char('R'), modifiers: KeyModifiers::SHIFT },
			log_reword_commit: GituiKeyEvent { code: KeyCode::Char('r'), modifiers: KeyModifiers::empty() },
			log_find: GituiKeyEvent { code: KeyCode::Char('f'), modifiers: KeyModifiers::empty() },
			find_commit_sha: GituiKeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL),
			commit_amend: GituiKeyEvent::new(KeyCode::Char('a'),  KeyModifiers::CONTROL),
			toggle_signoff: GituiKeyEvent::new(KeyCode::Char('s'),  KeyModifiers::CONTROL),
			toggle_verify: GituiKeyEvent::new(KeyCode::Char('f'),  KeyModifiers::CONTROL),
			copy: GituiKeyEvent::new(KeyCode::Char('y'),  KeyModifiers::empty()),
			create_branch: GituiKeyEvent::new(KeyCode::Char('c'),  KeyModifiers::empty()),
			rename_branch: GituiKeyEvent::new(KeyCode::Char('r'),  KeyModifiers::empty()),
			select_branch: GituiKeyEvent::new(KeyCode::Char('b'),  KeyModifiers::empty()),
			delete_branch: GituiKeyEvent::new(KeyCode::Char('D'),  KeyModifiers::SHIFT),
			merge_branch: GituiKeyEvent::new(KeyCode::Char('m'),  KeyModifiers::empty()),
			rebase_branch: GituiKeyEvent::new(KeyCode::Char('R'),  KeyModifiers::SHIFT),
			reset_branch: GituiKeyEvent::new(KeyCode::Char('s'),  KeyModifiers::empty()),
			compare_commits: GituiKeyEvent::new(KeyCode::Char('C'),  KeyModifiers::SHIFT),
			tags: GituiKeyEvent::new(KeyCode::Char('T'),  KeyModifiers::SHIFT),
			delete_tag: GituiKeyEvent::new(KeyCode::Char('D'),  KeyModifiers::SHIFT),
			select_tag: GituiKeyEvent::new(KeyCode::Enter,  KeyModifiers::empty()),
			push: GituiKeyEvent::new(KeyCode::Char('p'),  KeyModifiers::empty()),
			force_push: GituiKeyEvent::new(KeyCode::Char('P'),  KeyModifiers::SHIFT),
			undo_commit: GituiKeyEvent::new(KeyCode::Char('U'),  KeyModifiers::SHIFT),
			fetch: GituiKeyEvent::new(KeyCode::Char('F'),  KeyModifiers::SHIFT),
			pull: GituiKeyEvent::new(KeyCode::Char('f'),  KeyModifiers::empty()),
			abort_merge: GituiKeyEvent::new(KeyCode::Char('A'),  KeyModifiers::SHIFT),
			open_file_tree: GituiKeyEvent::new(KeyCode::Char('F'),  KeyModifiers::SHIFT),
			file_find: GituiKeyEvent::new(KeyCode::Char('f'),  KeyModifiers::empty()),
			branch_find: GituiKeyEvent::new(KeyCode::Char('f'),  KeyModifiers::empty()),
			diff_hunk_next: GituiKeyEvent::new(KeyCode::Char('n'),  KeyModifiers::empty()),
			diff_hunk_prev: GituiKeyEvent::new(KeyCode::Char('p'),  KeyModifiers::empty()),
			stage_unstage_item: GituiKeyEvent::new(KeyCode::Enter,  KeyModifiers::empty()),
			tag_annotate: GituiKeyEvent::new(KeyCode::Char('a'),  KeyModifiers::CONTROL),
			view_submodules: GituiKeyEvent::new(KeyCode::Char('S'),  KeyModifiers::SHIFT),
			view_remotes: GituiKeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
			update_remote_name: GituiKeyEvent::new(KeyCode::Char('n'),KeyModifiers::NONE),
			update_remote_url: GituiKeyEvent::new(KeyCode::Char('u'),KeyModifiers::NONE),
			add_remote: GituiKeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE),
			delete_remote: GituiKeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE),
			view_submodule_parent: GituiKeyEvent::new(KeyCode::Char('p'),  KeyModifiers::empty()),
			update_submodule: GituiKeyEvent::new(KeyCode::Char('u'),  KeyModifiers::empty()),
			commit_history_next: GituiKeyEvent::new(KeyCode::Char('n'),  KeyModifiers::CONTROL),
			commit: GituiKeyEvent::new(KeyCode::Char('d'),  KeyModifiers::CONTROL),
			newline: GituiKeyEvent::new(KeyCode::Enter,  KeyModifiers::empty()),
		}
	}
}

impl KeysList {
	pub fn init(file: PathBuf) -> Self {
		let mut keys_list = Self::default();
		if let Ok(f) = File::open(file) {
			match ron::de::from_reader(f) {
				Ok(patch) => keys_list.apply(patch),
				Err(e) => {
					log::error!("KeysList parse error: {e}");
				}
			}
		}
		keys_list
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use pretty_assertions::assert_eq;
	use std::io::Write;
	use tempfile::NamedTempFile;

	#[test]
	fn test_apply_vim_style_example() {
		let mut keys_list = KeysList::default();
		let f = File::open("vim_style_key_config.ron")
			.expect("vim style config should exist");
		let patch = ron::de::from_reader(f)
			.expect("vim style config format incorrect");
		keys_list.apply(patch);
	}

	#[test]
	fn test_smoke() {
		let mut file = NamedTempFile::new().unwrap();

		writeln!(
			file,
			r#"
(
	move_down: Some(( code: Char('j'), modifiers: "CONTROL")),
	move_up: Some((code: Char('h'), modifiers: ""))
)
"#
		)
		.unwrap();

		let keys = KeysList::init(file.path().to_path_buf());

		assert_eq!(keys.move_right, KeysList::default().move_right);
		assert_eq!(
			keys.move_down,
			GituiKeyEvent::new(
				KeyCode::Char('j'),
				KeyModifiers::CONTROL
			)
		);
		assert_eq!(
			keys.move_up,
			GituiKeyEvent::new(
				KeyCode::Char('h'),
				KeyModifiers::NONE
			)
		);
	}
}
