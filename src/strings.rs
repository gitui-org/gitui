use std::borrow::Cow;

use asyncgit::sync::CommitId;
use unicode_truncate::UnicodeTruncateStr;
use unicode_width::UnicodeWidthStr;

use crate::keys::SharedKeyConfig;

pub mod order {
	pub const RARE_ACTION: i8 = 30;
	pub const NAV: i8 = 20;
	pub const AVERAGE: i8 = 10;
	pub const PRIORITY: i8 = 1;
}

pub static PUSH_POPUP_MSG: &str = "Push";
pub static FORCE_PUSH_POPUP_MSG: &str = "Force Push";
pub static PULL_POPUP_MSG: &str = "Pull";
pub static FETCH_POPUP_MSG: &str = "Fetch";
pub static PUSH_POPUP_PROGRESS_NONE: &str = "preparing...";
pub static PUSH_POPUP_STATES_ADDING: &str = "adding objects (1/3)";
pub static PUSH_POPUP_STATES_DELTAS: &str = "deltas (2/3)";
pub static PUSH_POPUP_STATES_PUSHING: &str = "pushing (3/3)";
pub static PUSH_POPUP_STATES_TRANSFER: &str = "transfer";
pub static PUSH_POPUP_STATES_DONE: &str = "done";

pub static PUSH_TAGS_POPUP_MSG: &str = "Push Tags";
pub static PUSH_TAGS_STATES_FETCHING: &str = "fetching";
pub static PUSH_TAGS_STATES_PUSHING: &str = "pushing";
pub static PUSH_TAGS_STATES_DONE: &str = "done";

pub static POPUP_TITLE_SUBMODULES: &str = "Submodules";
pub static POPUP_TITLE_REMOTES: &str = "Remotes";
pub static POPUP_SUBTITLE_REMOTES: &str = "Details";
pub static POPUP_TITLE_FUZZY_FIND: &str = "Fuzzy Finder";
pub static POPUP_TITLE_LOG_SEARCH: &str = "Search";

pub static POPUP_FAIL_COPY: &str = "Failed to copy text";
pub static POPUP_SUCCESS_COPY: &str = "Copied Text";
pub static POPUP_COMMIT_SHA_INVALID: &str = "Invalid commit sha";

pub mod symbol {
	pub const CHECKMARK: &str = "\u{2713}"; //✓
	pub const SPACE: &str = "\u{02FD}"; //˽
	pub const EMPTY_SPACE: &str = " ";
	pub const FOLDER_ICON_COLLAPSED: &str = "\u{25b8}"; //▸
	pub const FOLDER_ICON_EXPANDED: &str = "\u{25be}"; //▾
	pub const EMPTY_STR: &str = "";
	pub const ELLIPSIS: char = '\u{2026}'; // …
}

pub fn title_branches() -> String {
	"Branches".to_string()
}
pub fn title_tags() -> String {
	"Tags".to_string()
}
pub fn title_status(_key_config: &SharedKeyConfig) -> String {
	"Unstaged Changes".to_string()
}
pub fn title_diff(_key_config: &SharedKeyConfig) -> String {
	"Diff: ".to_string()
}
pub fn title_index(_key_config: &SharedKeyConfig) -> String {
	"Staged Changes".to_string()
}
pub fn tab_status(key_config: &SharedKeyConfig) -> String {
	format!(
		"Status [{}]",
		key_config.get_hint(key_config.keys.tab_status)
	)
}
pub fn tab_log(key_config: &SharedKeyConfig) -> String {
	format!("Log [{}]", key_config.get_hint(key_config.keys.tab_log))
}
pub fn tab_files(key_config: &SharedKeyConfig) -> String {
	format!(
		"Files [{}]",
		key_config.get_hint(key_config.keys.tab_files)
	)
}
pub fn tab_stashing(key_config: &SharedKeyConfig) -> String {
	format!(
		"Stashing [{}]",
		key_config.get_hint(key_config.keys.tab_stashing)
	)
}
pub fn tab_stashes(key_config: &SharedKeyConfig) -> String {
	format!(
		"Stashes [{}]",
		key_config.get_hint(key_config.keys.tab_stashes)
	)
}
pub fn tab_divider(_key_config: &SharedKeyConfig) -> String {
	" | ".to_string()
}
pub fn cmd_splitter(_key_config: &SharedKeyConfig) -> String {
	" ".to_string()
}
pub fn msg_opening_editor(_key_config: &SharedKeyConfig) -> String {
	"opening editor...".to_string()
}
pub fn msg_title_error(_key_config: &SharedKeyConfig) -> String {
	"Error".to_string()
}
pub fn msg_title_info(_key_config: &SharedKeyConfig) -> String {
	"Info".to_string()
}
pub fn commit_title() -> String {
	"Commit".to_string()
}
pub fn commit_reword_title() -> String {
	"Reword Commit".to_string()
}

pub fn commit_title_merge() -> String {
	"Commit (Merge)".to_string()
}
pub fn commit_title_revert() -> String {
	"Commit (Revert)".to_string()
}
pub fn commit_title_amend() -> String {
	"Commit (Amend)".to_string()
}
pub fn commit_msg(_key_config: &SharedKeyConfig) -> String {
	"type commit message..".to_string()
}
pub fn commit_first_line_warning(count: usize) -> String {
	format!("[subject length: {count}]")
}
pub const fn branch_name_invalid() -> &'static str {
	"[invalid name]"
}
pub fn commit_editor_msg(_key_config: &SharedKeyConfig) -> String {
	r"
# Edit your commit message
# Lines starting with '#' will be ignored"
		.to_string()
}
pub fn stash_popup_title(_key_config: &SharedKeyConfig) -> String {
	"Stash".to_string()
}
pub fn stash_popup_msg(_key_config: &SharedKeyConfig) -> String {
	"type name (optional)".to_string()
}
pub fn confirm_title_reset() -> String {
	"Reset".to_string()
}
pub fn confirm_title_undo_commit() -> String {
	"Undo commit".to_string()
}
pub fn confirm_title_stashdrop(
	_key_config: &SharedKeyConfig,
	multiple: bool,
) -> String {
	format!("Drop Stash{}", if multiple { "es" } else { "" })
}
pub fn confirm_title_stashpop(
	_key_config: &SharedKeyConfig,
) -> String {
	"Pop".to_string()
}
pub fn confirm_title_merge(
	_key_config: &SharedKeyConfig,
	rebase: bool,
) -> String {
	if rebase {
		"Merge (via rebase)".to_string()
	} else {
		"Merge (via commit)".to_string()
	}
}
pub fn confirm_msg_merge(
	_key_config: &SharedKeyConfig,
	incoming: usize,
	rebase: bool,
) -> String {
	if rebase {
		format!("Rebase onto {incoming} incoming commits?")
	} else {
		format!("Merge of {incoming} incoming commits?")
	}
}

pub fn confirm_title_abortmerge() -> String {
	"Abort merge?".to_string()
}
pub fn confirm_title_abortrevert() -> String {
	"Abort revert?".to_string()
}
pub fn confirm_msg_revertchanges() -> String {
	"This will revert all uncommitted changes. Are you sure?"
		.to_string()
}
pub fn confirm_title_abortrebase() -> String {
	"Abort rebase?".to_string()
}
pub fn confirm_msg_abortrebase() -> String {
	"This will revert all uncommitted changes. Are you sure?"
		.to_string()
}
pub fn confirm_msg_reset() -> String {
	"confirm file reset?".to_string()
}
pub fn confirm_msg_reset_lines(lines: usize) -> String {
	format!(
		"are you sure you want to discard {lines} selected lines?"
	)
}
pub fn confirm_msg_undo_commit() -> String {
	"confirm undo last commit?".to_string()
}
pub fn confirm_msg_stashdrop(
	_key_config: &SharedKeyConfig,
	ids: &[CommitId],
) -> String {
	format!(
		"Sure you want to drop following {}stash{}?\n\n{}",
		if ids.len() > 1 {
			format!("{} ", ids.len())
		} else {
			String::default()
		},
		if ids.len() > 1 { "es" } else { "" },
		ids.iter()
			.map(CommitId::get_short_string)
			.collect::<Vec<_>>()
			.join(", ")
	)
}
pub fn confirm_msg_stashpop(_key_config: &SharedKeyConfig) -> String {
	"The stash will be applied and removed from the stash list. Confirm stash pop?"
        .to_string()
}
pub fn confirm_msg_resethunk(
	_key_config: &SharedKeyConfig,
) -> String {
	"confirm reset hunk?".to_string()
}
pub fn confirm_title_delete_branch(
	_key_config: &SharedKeyConfig,
) -> String {
	"Delete Branch".to_string()
}
pub fn confirm_msg_delete_branch(
	_key_config: &SharedKeyConfig,
	branch_ref: &str,
) -> String {
	format!("Confirm deleting branch: '{branch_ref}' ?")
}
pub fn confirm_title_delete_remote_branch(
	_key_config: &SharedKeyConfig,
) -> String {
	"Delete Remote Branch".to_string()
}
pub fn confirm_title_delete_remote(
	_key_config: &SharedKeyConfig,
) -> String {
	"Delete Remote".to_string()
}
pub fn confirm_msg_delete_remote(
	_key_config: &SharedKeyConfig,
	remote_name: &str,
) -> String {
	format!("Confirm deleting remote \"{remote_name}\"")
}
pub fn confirm_msg_delete_remote_branch(
	_key_config: &SharedKeyConfig,
	branch_ref: &str,
) -> String {
	format!("Confirm deleting remote branch: '{branch_ref}' ?")
}
pub fn confirm_title_delete_tag(
	_key_config: &SharedKeyConfig,
) -> String {
	"Delete Tag".to_string()
}
pub fn confirm_msg_delete_tag(
	_key_config: &SharedKeyConfig,
	tag_name: &str,
) -> String {
	format!("Confirm deleting Tag: '{tag_name}' ?")
}
pub fn confirm_title_delete_tag_remote() -> String {
	"Delete Tag (remote)".to_string()
}
pub fn confirm_msg_delete_tag_remote(remote_name: &str) -> String {
	format!("Confirm deleting tag on remote '{remote_name}'?")
}
pub fn confirm_title_force_push(
	_key_config: &SharedKeyConfig,
) -> String {
	"Force Push".to_string()
}
pub fn confirm_msg_force_push(
	_key_config: &SharedKeyConfig,
	branch_ref: &str,
) -> String {
	format!(
        "Confirm force push to branch '{branch_ref}' ?  This may rewrite history."
    )
}
pub fn log_title(_key_config: &SharedKeyConfig) -> String {
	"Commit".to_string()
}
pub fn file_log_title(
	file_path: &str,
	selected: usize,
	revisions: usize,
) -> String {
	format!("Revisions of '{file_path}' ({selected}/{revisions})")
}
pub fn blame_title(_key_config: &SharedKeyConfig) -> String {
	"Blame".to_string()
}
pub fn tag_popup_name_title() -> String {
	"Tag".to_string()
}
pub fn tag_popup_name_msg() -> String {
	"type tag name".to_string()
}
pub fn tag_popup_annotation_title(name: &str) -> String {
	format!("Tag Annotation ({name})")
}
pub fn tag_popup_annotation_msg() -> String {
	"type tag annotation".to_string()
}
pub fn stashlist_title(_key_config: &SharedKeyConfig) -> String {
	"Stashes".to_string()
}
pub fn help_title(_key_config: &SharedKeyConfig) -> String {
	"Help: all commands".to_string()
}
pub fn stashing_files_title(_key_config: &SharedKeyConfig) -> String {
	"Files to Stash".to_string()
}
pub fn stashing_options_title(
	_key_config: &SharedKeyConfig,
) -> String {
	"Options".to_string()
}
pub fn loading_text(_key_config: &SharedKeyConfig) -> String {
	"Loading ...".to_string()
}
pub fn create_branch_popup_title(
	_key_config: &SharedKeyConfig,
) -> String {
	"Branch".to_string()
}
pub fn create_branch_popup_msg(
	_key_config: &SharedKeyConfig,
) -> String {
	"type branch name".to_string()
}
pub fn rename_remote_popup_title(
	_key_config: &SharedKeyConfig,
) -> String {
	"Rename remote".to_string()
}
pub fn rename_remote_popup_msg(
	_key_config: &SharedKeyConfig,
) -> String {
	"new remote name".to_string()
}
pub fn update_remote_url_popup_title(
	_key_config: &SharedKeyConfig,
) -> String {
	"Update url".to_string()
}
pub fn update_remote_url_popup_msg(
	_key_config: &SharedKeyConfig,
) -> String {
	"new remote url".to_string()
}
pub fn create_remote_popup_title_name(
	_key_config: &SharedKeyConfig,
) -> String {
	"Remote name".to_string()
}
pub fn create_remote_popup_title_url(
	_key_config: &SharedKeyConfig,
) -> String {
	"Remote url".to_string()
}
pub fn create_remote_popup_msg_name(
	_key_config: &SharedKeyConfig,
) -> String {
	"type remote name".to_string()
}
pub fn create_remote_popup_msg_url(
	_key_config: &SharedKeyConfig,
) -> String {
	"type remote url".to_string()
}
pub const fn remote_name_invalid() -> &'static str {
	"[invalid name]"
}
pub fn username_popup_title(_key_config: &SharedKeyConfig) -> String {
	"Username".to_string()
}
pub fn username_popup_msg(_key_config: &SharedKeyConfig) -> String {
	"type username".to_string()
}
pub fn password_popup_title(_key_config: &SharedKeyConfig) -> String {
	"Password".to_string()
}
pub fn password_popup_msg(_key_config: &SharedKeyConfig) -> String {
	"type password".to_string()
}

pub fn rename_branch_popup_title(
	_key_config: &SharedKeyConfig,
) -> String {
	"Rename Branch".to_string()
}
pub fn rename_branch_popup_msg(
	_key_config: &SharedKeyConfig,
) -> String {
	"new branch name".to_string()
}

pub fn copy_success(s: &str) -> String {
	format!("{POPUP_SUCCESS_COPY} \"{s}\"")
}

pub fn ellipsis_trim_start(s: &str, width: usize) -> Cow<str> {
	if s.width() <= width {
		Cow::Borrowed(s)
	} else {
		Cow::Owned(format!(
			"[{}]{}",
			symbol::ELLIPSIS,
			s.unicode_truncate_start(
				width.saturating_sub(3 /* front indicator */)
			)
			.0
		))
	}
}

pub mod commit {
	use crate::keys::SharedKeyConfig;

	pub fn details_author() -> String {
		"Author: ".to_string()
	}
	pub fn details_committer() -> String {
		"Committer: ".to_string()
	}
	pub fn details_sha() -> String {
		"Sha: ".to_string()
	}
	pub fn details_date() -> String {
		"Date: ".to_string()
	}
	pub fn details_tags() -> String {
		"Tags: ".to_string()
	}
	pub fn details_message() -> String {
		"Subject: ".to_string()
	}
	pub fn details_info_title(
		_key_config: &SharedKeyConfig,
	) -> String {
		"Info".to_string()
	}
	pub fn compare_details_info_title(
		old: bool,
		hash: &str,
	) -> String {
		format!("{}: {hash}", if old { "Old" } else { "New" })
	}
	pub fn details_message_title(
		_key_config: &SharedKeyConfig,
	) -> String {
		"Message".to_string()
	}
	pub fn details_files_title(
		_key_config: &SharedKeyConfig,
	) -> String {
		"Files:".to_string()
	}
}

pub mod commands {
	use crate::components::CommandText;
	use crate::keys::SharedKeyConfig;

	static CMD_GROUP_GENERAL: &str = "-- General --";
	static CMD_GROUP_DIFF: &str = "-- Diff --";
	static CMD_GROUP_CHANGES: &str = "-- Changes --";
	static CMD_GROUP_COMMIT_POPUP: &str = "-- Commit Popup --";
	static CMD_GROUP_STASHING: &str = "-- Stashing --";
	static CMD_GROUP_STASHES: &str = "-- Stashes --";
	static CMD_GROUP_LOG: &str = "-- Log --";
	static CMD_GROUP_BRANCHES: &str = "-- Branches --";

	pub fn toggle_tabs(key_config: &SharedKeyConfig) -> CommandText {
		CommandText::new(
			format!(
				"Next [{}]",
				key_config.get_hint(key_config.keys.tab_toggle)
			),
			"switch to next tab",
			CMD_GROUP_GENERAL,
		)
	}
	pub fn find_file(key_config: &SharedKeyConfig) -> CommandText {
		CommandText::new(
			format!(
				"Find [{}]",
				key_config.get_hint(key_config.keys.file_find)
			),
			"find file in tree",
			CMD_GROUP_GENERAL,
		)
	}
	pub fn find_branch(key_config: &SharedKeyConfig) -> CommandText {
		CommandText::new(
			format!(
				"Find [{}]",
				key_config.get_hint(key_config.keys.branch_find)
			),
			"find branch in list",
			CMD_GROUP_GENERAL,
		)
	}
	pub fn toggle_tabs_direct(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Tab [{}{}{}{}{}]",
				key_config.get_hint(key_config.keys.tab_status),
				key_config.get_hint(key_config.keys.tab_log),
				key_config.get_hint(key_config.keys.tab_files),
				key_config.get_hint(key_config.keys.tab_stashing),
				key_config.get_hint(key_config.keys.tab_stashes),
			),
			"switch top level tabs directly",
			CMD_GROUP_GENERAL,
		)
	}
	pub fn options_popup(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Options [{}]",
				key_config.get_hint(key_config.keys.open_options),
			),
			"open options popup",
			CMD_GROUP_GENERAL,
		)
	}
	pub fn help_open(key_config: &SharedKeyConfig) -> CommandText {
		CommandText::new(
			format!(
				"Help [{}]",
				key_config.get_hint(key_config.keys.open_help)
			),
			"open this help screen",
			CMD_GROUP_GENERAL,
		)
	}
	pub fn navigate_commit_message(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Nav [{}{}]",
				key_config.get_hint(key_config.keys.move_up),
				key_config.get_hint(key_config.keys.move_down)
			),
			"navigate commit message",
			CMD_GROUP_GENERAL,
		)
	}
	pub fn navigate_tree(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Nav [{}{}{}{}]",
				key_config.get_hint(key_config.keys.move_up),
				key_config.get_hint(key_config.keys.move_down),
				key_config.get_hint(key_config.keys.move_right),
				key_config.get_hint(key_config.keys.move_left)
			),
			"navigate tree view, collapse, expand",
			CMD_GROUP_GENERAL,
		)
	}
	pub fn scroll(key_config: &SharedKeyConfig) -> CommandText {
		CommandText::new(
			format!(
				"Scroll [{}{}]",
				key_config.get_hint(key_config.keys.move_up),
				key_config.get_hint(key_config.keys.move_down)
			),
			"scroll up or down in focused view",
			CMD_GROUP_GENERAL,
		)
	}
	pub fn commit_list_mark(
		key_config: &SharedKeyConfig,
		marked: bool,
	) -> CommandText {
		CommandText::new(
			format!(
				"{} [{}]",
				if marked { "Unmark" } else { "Mark" },
				key_config.get_hint(key_config.keys.log_mark_commit),
			),
			"mark multiple commits",
			CMD_GROUP_GENERAL,
		)
	}
	pub fn copy(key_config: &SharedKeyConfig) -> CommandText {
		CommandText::new(
			format!(
				"Copy [{}]",
				key_config.get_hint(key_config.keys.copy),
			),
			"copy selected lines to clipboard",
			CMD_GROUP_DIFF,
		)
	}
	pub fn copy_hash(key_config: &SharedKeyConfig) -> CommandText {
		CommandText::new(
			format!(
				"Copy Hash [{}]",
				key_config.get_hint(key_config.keys.copy),
			),
			"copy selected commit hash to clipboard",
			CMD_GROUP_LOG,
		)
	}
	pub fn copy_path(key_config: &SharedKeyConfig) -> CommandText {
		CommandText::new(
			format!(
				"Copy Path [{}]",
				key_config.get_hint(key_config.keys.copy),
			),
			"copy selected file path to clipboard",
			CMD_GROUP_LOG,
		)
	}
	pub fn push_tags(key_config: &SharedKeyConfig) -> CommandText {
		CommandText::new(
			format!(
				"Push Tags [{}]",
				key_config.get_hint(key_config.keys.push),
			),
			"push tags to remote",
			CMD_GROUP_LOG,
		)
	}
	pub fn toggle_option(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Toggle Option [{}]",
				key_config.get_hint(key_config.keys.log_mark_commit),
			),
			"toggle search option selected",
			CMD_GROUP_LOG,
		)
	}
	pub fn show_tag_annotation(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Annotation [{}]",
				key_config.get_hint(key_config.keys.move_right),
			),
			"show tag annotation",
			CMD_GROUP_LOG,
		)
	}
	pub fn diff_hunk_next(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Next hunk [{}]",
				key_config.get_hint(key_config.keys.diff_hunk_next),
			),
			"move cursor to next hunk",
			CMD_GROUP_DIFF,
		)
	}
	pub fn diff_hunk_prev(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Prev hunk [{}]",
				key_config.get_hint(key_config.keys.diff_hunk_prev),
			),
			"move cursor to prev hunk",
			CMD_GROUP_DIFF,
		)
	}
	pub fn diff_home_end(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Jump up/down [{},{},{},{}]",
				key_config.get_hint(key_config.keys.home),
				key_config.get_hint(key_config.keys.end),
				key_config.get_hint(key_config.keys.move_up),
				key_config.get_hint(key_config.keys.move_down)
			),
			"scroll to top or bottom of diff",
			CMD_GROUP_DIFF,
		)
	}
	pub fn diff_hunk_add(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Add hunk [{}]",
				key_config
					.get_hint(key_config.keys.stage_unstage_item),
			),
			"adds selected hunk to stage",
			CMD_GROUP_DIFF,
		)
	}
	pub fn diff_hunk_revert(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Reset hunk [{}]",
				key_config
					.get_hint(key_config.keys.status_reset_item),
			),
			"reverts selected hunk",
			CMD_GROUP_DIFF,
		)
	}
	pub fn diff_lines_revert(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Reset lines [{}]",
				key_config.get_hint(key_config.keys.diff_reset_lines),
			),
			"resets selected lines",
			CMD_GROUP_DIFF,
		)
	}
	pub fn diff_lines_stage(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Stage lines [{}]",
				key_config.get_hint(key_config.keys.diff_stage_lines),
			),
			"stage selected lines",
			CMD_GROUP_DIFF,
		)
	}
	pub fn diff_lines_unstage(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Unstage lines [{}]",
				key_config.get_hint(key_config.keys.diff_stage_lines),
			),
			"unstage selected lines",
			CMD_GROUP_DIFF,
		)
	}
	pub fn diff_hunk_remove(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Remove hunk [{}]",
				key_config
					.get_hint(key_config.keys.stage_unstage_item),
			),
			"removes selected hunk from stage",
			CMD_GROUP_DIFF,
		)
	}
	pub fn close_fuzzy_finder(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Close [{}{}]",
				key_config.get_hint(key_config.keys.exit_popup),
				key_config.get_hint(key_config.keys.enter),
			),
			"close fuzzy finder",
			CMD_GROUP_GENERAL,
		)
	}
	pub fn close_popup(key_config: &SharedKeyConfig) -> CommandText {
		CommandText::new(
			format!(
				"Close [{}]",
				key_config.get_hint(key_config.keys.exit_popup),
			),
			"close overlay (e.g commit, help)",
			CMD_GROUP_GENERAL,
		)
	}
	pub fn scroll_popup(key_config: &SharedKeyConfig) -> CommandText {
		CommandText::new(
			format!(
				"Scroll [{}{}]",
				key_config.get_hint(key_config.keys.popup_down),
				key_config.get_hint(key_config.keys.popup_up),
			),
			"scroll up or down in popup",
			CMD_GROUP_GENERAL,
		)
	}
	pub fn close_msg(key_config: &SharedKeyConfig) -> CommandText {
		CommandText::new(
			format!(
				"Close [{}]",
				key_config.get_hint(key_config.keys.enter),
			),
			"close msg popup (e.g msg)",
			CMD_GROUP_GENERAL,
		)
		.hide_help()
	}
	pub fn validate_msg(key_config: &SharedKeyConfig) -> CommandText {
		CommandText::new(
			format!(
				"Validate [{}]",
				key_config.get_hint(key_config.keys.enter),
			),
			"validate msg",
			CMD_GROUP_GENERAL,
		)
		.hide_help()
	}

	pub fn abort_merge(key_config: &SharedKeyConfig) -> CommandText {
		CommandText::new(
			format!(
				"Abort merge [{}]",
				key_config.get_hint(key_config.keys.abort_merge),
			),
			"abort ongoing merge",
			CMD_GROUP_GENERAL,
		)
	}

	pub fn abort_revert(key_config: &SharedKeyConfig) -> CommandText {
		CommandText::new(
			format!(
				"Abort revert [{}]",
				key_config.get_hint(key_config.keys.abort_merge),
			),
			"abort ongoing revert",
			CMD_GROUP_GENERAL,
		)
	}

	pub fn view_submodules(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Submodules [{}]",
				key_config.get_hint(key_config.keys.view_submodules),
			),
			"open submodule view",
			CMD_GROUP_GENERAL,
		)
	}

	pub fn view_remotes(key_config: &SharedKeyConfig) -> CommandText {
		CommandText::new(
			format!(
				"Remotes [{}]",
				key_config.get_hint(key_config.keys.view_remotes)
			),
			"open remotes view",
			CMD_GROUP_GENERAL,
		)
	}

	pub fn update_remote_name(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Edit name [{}]",
				key_config
					.get_hint(key_config.keys.update_remote_name)
			),
			"updates a remote name",
			CMD_GROUP_GENERAL,
		)
	}

	pub fn update_remote_url(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Edit url [{}]",
				key_config
					.get_hint(key_config.keys.update_remote_url)
			),
			"updates a remote url",
			CMD_GROUP_GENERAL,
		)
	}

	pub fn create_remote(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Add [{}]",
				key_config.get_hint(key_config.keys.add_remote)
			),
			"creates a new remote",
			CMD_GROUP_GENERAL,
		)
	}

	pub fn delete_remote_popup(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Remove [{}]",
				key_config.get_hint(key_config.keys.delete_remote),
			),
			"remove a remote",
			CMD_GROUP_BRANCHES,
		)
	}

	pub fn remote_confirm_name_msg(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Confirm name [{}]",
				key_config.get_hint(key_config.keys.enter),
			),
			"confirm remote name",
			CMD_GROUP_BRANCHES,
		)
		.hide_help()
	}

	pub fn remote_confirm_url_msg(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Confirm url [{}]",
				key_config.get_hint(key_config.keys.enter),
			),
			"confirm remote url",
			CMD_GROUP_BRANCHES,
		)
		.hide_help()
	}

	pub fn open_submodule(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Open [{}]",
				key_config.get_hint(key_config.keys.enter),
			),
			"open submodule",
			CMD_GROUP_GENERAL,
		)
	}

	pub fn open_submodule_parent(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Open Parent [{}]",
				key_config
					.get_hint(key_config.keys.view_submodule_parent),
			),
			"open submodule parent repo",
			CMD_GROUP_GENERAL,
		)
	}

	pub fn update_submodule(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Update [{}]",
				key_config.get_hint(key_config.keys.update_submodule),
			),
			"update submodule",
			CMD_GROUP_GENERAL,
		)
	}

	pub fn continue_rebase(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Continue rebase [{}]",
				key_config.get_hint(key_config.keys.rebase_branch),
			),
			"continue ongoing rebase",
			CMD_GROUP_GENERAL,
		)
	}

	pub fn abort_rebase(key_config: &SharedKeyConfig) -> CommandText {
		CommandText::new(
			format!(
				"Abort rebase [{}]",
				key_config.get_hint(key_config.keys.abort_merge),
			),
			"abort ongoing rebase",
			CMD_GROUP_GENERAL,
		)
	}

	pub fn select_staging(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"To stage [{}]",
				key_config.get_hint(key_config.keys.toggle_workarea),
			),
			"focus/select staging area",
			CMD_GROUP_GENERAL,
		)
	}
	pub fn select_unstaged(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"To unstaged [{}]",
				key_config.get_hint(key_config.keys.toggle_workarea),
			),
			"focus/select unstaged area",
			CMD_GROUP_GENERAL,
		)
	}
	pub fn undo_commit(key_config: &SharedKeyConfig) -> CommandText {
		CommandText::new(
			format!(
				"Undo Commit [{}]",
				key_config.get_hint(key_config.keys.undo_commit),
			),
			"undo last commit",
			CMD_GROUP_GENERAL,
		)
	}
	pub fn commit_open(key_config: &SharedKeyConfig) -> CommandText {
		CommandText::new(
			format!(
				"Commit [{}]",
				key_config.get_hint(key_config.keys.open_commit),
			),
			"open commit popup (available in non-empty stage)",
			CMD_GROUP_GENERAL,
		)
	}
	pub fn commit_open_editor(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Open editor [{}]",
				key_config
					.get_hint(key_config.keys.open_commit_editor),
			),
			"open commit editor (available in commit popup)",
			CMD_GROUP_COMMIT_POPUP,
		)
	}
	pub fn commit_next_msg_from_history(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Previous Msg [{}]",
				key_config
					.get_hint(key_config.keys.commit_history_next),
			),
			"use previous commit message from history",
			CMD_GROUP_COMMIT_POPUP,
		)
	}
	pub fn commit_submit(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Do Commit [{}]",
				key_config.get_hint(key_config.keys.commit),
			),
			"commit (available when commit message is non-empty)",
			CMD_GROUP_COMMIT_POPUP,
		)
		.hide_help()
	}
	pub fn newline(key_config: &SharedKeyConfig) -> CommandText {
		CommandText::new(
			format!(
				"New line [{}]",
				key_config.get_hint(key_config.keys.newline),
			),
			"create line break",
			CMD_GROUP_COMMIT_POPUP,
		)
		.hide_help()
	}
	pub fn toggle_verify(
		key_config: &SharedKeyConfig,
		current_verify: bool,
	) -> CommandText {
		let verb = if current_verify { "disable" } else { "enable" };
		CommandText::new(
			format!(
				"{} hooks [{}]",
				verb,
				key_config.get_hint(key_config.keys.toggle_verify),
			),
			"toggle running on commit hooks (available in commit popup)",
			CMD_GROUP_COMMIT_POPUP,
		)
	}

	pub fn commit_amend(key_config: &SharedKeyConfig) -> CommandText {
		CommandText::new(
			format!(
				"Amend [{}]",
				key_config.get_hint(key_config.keys.commit_amend),
			),
			"amend last commit (available in commit popup)",
			CMD_GROUP_COMMIT_POPUP,
		)
	}
	pub fn commit_signoff(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Sign-off [{}]",
				key_config.get_hint(key_config.keys.toggle_signoff),
			),
			"sign-off commit (-s option)",
			CMD_GROUP_COMMIT_POPUP,
		)
	}
	pub fn edit_item(key_config: &SharedKeyConfig) -> CommandText {
		CommandText::new(
			format!(
				"Edit [{}]",
				key_config.get_hint(key_config.keys.edit_file),
			),
			"edit the currently selected file in an external editor",
			CMD_GROUP_CHANGES,
		)
	}
	pub fn stage_item(key_config: &SharedKeyConfig) -> CommandText {
		CommandText::new(
			format!(
				"Stage [{}]",
				key_config
					.get_hint(key_config.keys.stage_unstage_item),
			),
			"stage currently selected file or entire path",
			CMD_GROUP_CHANGES,
		)
	}
	pub fn stage_all(key_config: &SharedKeyConfig) -> CommandText {
		CommandText::new(
			format!(
				"Stage All [{}]",
				key_config.get_hint(key_config.keys.status_stage_all),
			),
			"stage all changes (in unstaged files)",
			CMD_GROUP_CHANGES,
		)
	}
	pub fn unstage_item(key_config: &SharedKeyConfig) -> CommandText {
		CommandText::new(
			format!(
				"Unstage [{}]",
				key_config
					.get_hint(key_config.keys.stage_unstage_item),
			),
			"unstage currently selected file or entire path",
			CMD_GROUP_CHANGES,
		)
	}
	pub fn unstage_all(key_config: &SharedKeyConfig) -> CommandText {
		CommandText::new(
			format!(
				"Unstage all [{}]",
				key_config.get_hint(key_config.keys.status_stage_all),
			),
			"unstage all files (in staged files)",
			CMD_GROUP_CHANGES,
		)
	}
	pub fn reset_item(key_config: &SharedKeyConfig) -> CommandText {
		CommandText::new(
			format!(
				"Reset [{}]",
				key_config
					.get_hint(key_config.keys.status_reset_item),
			),
			"revert changes in selected file or entire path",
			CMD_GROUP_CHANGES,
		)
	}
	pub fn ignore_item(key_config: &SharedKeyConfig) -> CommandText {
		CommandText::new(
			format!(
				"Ignore [{}]",
				key_config
					.get_hint(key_config.keys.status_ignore_file),
			),
			"Add file or path to .gitignore",
			CMD_GROUP_CHANGES,
		)
	}

	pub fn diff_focus_left(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Back [{}]",
				key_config.get_hint(key_config.keys.move_left),
			),
			"view and select changed files",
			CMD_GROUP_GENERAL,
		)
	}
	pub fn diff_focus_right(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Diff [{}]",
				key_config.get_hint(key_config.keys.move_right),
			),
			"inspect file diff",
			CMD_GROUP_GENERAL,
		)
	}
	pub fn quit(key_config: &SharedKeyConfig) -> CommandText {
		CommandText::new(
			format!(
				"Quit [{}]",
				key_config.get_hint(key_config.keys.exit),
			),
			"quit gitui application",
			CMD_GROUP_GENERAL,
		)
	}
	pub fn confirm_action(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Confirm [{}]",
				key_config.get_hint(key_config.keys.enter),
			),
			"confirm action",
			CMD_GROUP_GENERAL,
		)
	}
	pub fn stashing_save(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Save [{}]",
				key_config.get_hint(key_config.keys.stashing_save),
			),
			"opens stash name input popup",
			CMD_GROUP_STASHING,
		)
	}
	pub fn stashing_toggle_indexed(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Toggle Staged [{}]",
				key_config
					.get_hint(key_config.keys.stashing_toggle_index),
			),
			"toggle including staged files into stash",
			CMD_GROUP_STASHING,
		)
	}
	pub fn stashing_toggle_untracked(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Toggle Untracked [{}]",
				key_config.get_hint(
					key_config.keys.stashing_toggle_untracked
				),
			),
			"toggle including untracked files into stash",
			CMD_GROUP_STASHING,
		)
	}
	pub fn stashing_confirm_msg(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Stash [{}]",
				key_config.get_hint(key_config.keys.enter),
			),
			"save files to stash",
			CMD_GROUP_STASHING,
		)
	}
	pub fn stashlist_apply(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Apply [{}]",
				key_config.get_hint(key_config.keys.stash_apply),
			),
			"apply selected stash",
			CMD_GROUP_STASHES,
		)
	}
	pub fn stashlist_drop(
		key_config: &SharedKeyConfig,
		marked: usize,
	) -> CommandText {
		CommandText::new(
			format!(
				"Drop{} [{}]",
				if marked == 0 {
					String::default()
				} else {
					format!(" {marked}")
				},
				key_config.get_hint(key_config.keys.stash_drop),
			),
			"drop selected stash",
			CMD_GROUP_STASHES,
		)
	}
	pub fn stashlist_pop(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Pop [{}]",
				key_config.get_hint(key_config.keys.enter),
			),
			"pop selected stash",
			CMD_GROUP_STASHES,
		)
	}
	pub fn stashlist_inspect(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Inspect [{}]",
				key_config.get_hint(key_config.keys.stash_open),
			),
			"open stash commit details (allows to diff files)",
			CMD_GROUP_STASHES,
		)
	}
	pub fn log_details_toggle(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Details [{}]",
				key_config.get_hint(key_config.keys.enter),
			),
			"open details of selected commit",
			CMD_GROUP_LOG,
		)
	}

	pub fn commit_details_open(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Inspect [{}]",
				key_config.get_hint(key_config.keys.move_right),
			),
			"inspect selected commit in detail",
			CMD_GROUP_GENERAL,
		)
	}

	pub fn blame_file(key_config: &SharedKeyConfig) -> CommandText {
		CommandText::new(
			format!(
				"Blame [{}]",
				key_config.get_hint(key_config.keys.blame),
			),
			"open blame view of selected file",
			CMD_GROUP_GENERAL,
		)
	}
	pub fn blame_file_back(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Back [{}]",
				key_config.get_hint(key_config.keys.blame_back),
			),
			"go to previous blame",
			CMD_GROUP_GENERAL,
		)
	}
	pub fn open_file_history(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"History [{}]",
				key_config.get_hint(key_config.keys.file_history),
			),
			"open history of selected file",
			CMD_GROUP_LOG,
		)
	}
	pub fn log_tag_commit(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Tag [{}]",
				key_config.get_hint(key_config.keys.log_tag_commit),
			),
			"tag commit",
			CMD_GROUP_LOG,
		)
	}
	pub fn log_checkout_commit(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Checkout [{}]",
				key_config
					.get_hint(key_config.keys.log_checkout_commit),
			),
			"checkout commit",
			CMD_GROUP_LOG,
		)
	}
	pub fn inspect_file_tree(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Files [{}]",
				key_config.get_hint(key_config.keys.open_file_tree),
			),
			"inspect file tree at specific revision",
			CMD_GROUP_LOG,
		)
	}
	pub fn revert_commit(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Revert [{}]",
				key_config
					.get_hint(key_config.keys.status_reset_item),
			),
			"revert commit",
			CMD_GROUP_LOG,
		)
	}
	pub fn log_reset_commit(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Reset [{}]",
				key_config.get_hint(key_config.keys.log_reset_commit),
			),
			"reset to commit",
			CMD_GROUP_LOG,
		)
	}
	pub fn log_reword_commit(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Reword [{}]",
				key_config
					.get_hint(key_config.keys.log_reword_commit),
			),
			"reword commit message",
			CMD_GROUP_LOG,
		)
	}
	pub fn log_find_commit(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Find [{}]",
				key_config.get_hint(key_config.keys.file_find),
			),
			"start commit search",
			CMD_GROUP_LOG,
		)
	}
	pub fn log_close_search(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Exit Search [{}]",
				key_config.get_hint(key_config.keys.exit_popup),
			),
			"leave search mode",
			CMD_GROUP_LOG,
		)
	}

	pub fn reset_commit(key_config: &SharedKeyConfig) -> CommandText {
		CommandText::new(
			format!(
				"Confirm [{}]",
				key_config.get_hint(key_config.keys.enter),
			),
			"confirm reset",
			CMD_GROUP_LOG,
		)
	}

	pub fn reset_branch(key_config: &SharedKeyConfig) -> CommandText {
		CommandText::new(
			format!(
				"Reset [{}]",
				key_config.get_hint(key_config.keys.reset_branch),
			),
			"confirm reset",
			CMD_GROUP_BRANCHES,
		)
	}

	pub fn reset_type(key_config: &SharedKeyConfig) -> CommandText {
		CommandText::new(
			format!(
				"Change Type [{}{}]",
				key_config.get_hint(key_config.keys.move_up),
				key_config.get_hint(key_config.keys.move_down)
			),
			"change reset type",
			CMD_GROUP_LOG,
		)
	}
	pub fn tag_commit_confirm_msg(
		key_config: &SharedKeyConfig,
		is_annotation_mode: bool,
	) -> CommandText {
		CommandText::new(
			format!(
				"Tag [{}]",
				key_config.get_hint(if is_annotation_mode {
					key_config.keys.commit
				} else {
					key_config.keys.enter
				}),
			),
			"tag commit",
			CMD_GROUP_LOG,
		)
	}

	pub fn tag_annotate_msg(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Annotate [{}]",
				key_config.get_hint(key_config.keys.tag_annotate),
			),
			"annotate tag",
			CMD_GROUP_LOG,
		)
	}

	pub fn create_branch_confirm_msg(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Create Branch [{}]",
				key_config.get_hint(key_config.keys.enter),
			),
			"create branch",
			CMD_GROUP_BRANCHES,
		)
		.hide_help()
	}
	pub fn open_branch_create_popup(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Create [{}]",
				key_config.get_hint(key_config.keys.create_branch),
			),
			"open create branch popup",
			CMD_GROUP_BRANCHES,
		)
	}
	pub fn rename_branch_confirm_msg(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Rename Branch [{}]",
				key_config.get_hint(key_config.keys.enter),
			),
			"rename branch",
			CMD_GROUP_BRANCHES,
		)
		.hide_help()
	}
	pub fn rename_branch_popup(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Rename Branch [{}]",
				key_config.get_hint(key_config.keys.rename_branch),
			),
			"rename branch",
			CMD_GROUP_BRANCHES,
		)
	}
	pub fn delete_branch_popup(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Delete [{}]",
				key_config.get_hint(key_config.keys.delete_branch),
			),
			"delete a branch",
			CMD_GROUP_BRANCHES,
		)
	}
	pub fn merge_branch_popup(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Merge [{}]",
				key_config.get_hint(key_config.keys.merge_branch),
			),
			"merge a branch",
			CMD_GROUP_BRANCHES,
		)
	}

	pub fn branch_popup_rebase(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Rebase [{}]",
				key_config.get_hint(key_config.keys.rebase_branch),
			),
			"rebase a branch",
			CMD_GROUP_BRANCHES,
		)
	}

	pub fn compare_with_head(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Compare [{}]",
				key_config.get_hint(key_config.keys.compare_commits),
			),
			"compare with head",
			CMD_GROUP_BRANCHES,
		)
	}

	pub fn compare_commits(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Compare Commits [{}]",
				key_config.get_hint(key_config.keys.compare_commits),
			),
			"compare two marked commits",
			CMD_GROUP_LOG,
		)
	}

	pub fn select_branch_popup(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Checkout [{}]",
				key_config.get_hint(key_config.keys.enter),
			),
			"checkout branch",
			CMD_GROUP_BRANCHES,
		)
	}
	pub fn toggle_branch_popup(
		key_config: &SharedKeyConfig,
		local: bool,
	) -> CommandText {
		CommandText::new(
			format!(
				"{} [{}]",
				if local { "Remote" } else { "Local" },
				key_config.get_hint(key_config.keys.tab_toggle),
			),
			"toggle branch type (remote/local)",
			CMD_GROUP_BRANCHES,
		)
	}
	pub fn open_branch_select_popup(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Branches [{}]",
				key_config.get_hint(key_config.keys.select_branch),
			),
			"open branch popup",
			CMD_GROUP_BRANCHES,
		)
	}

	pub fn open_tags_popup(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Tags [{}]",
				key_config.get_hint(key_config.keys.tags),
			),
			"open tags popup",
			CMD_GROUP_GENERAL,
		)
	}
	pub fn delete_tag_popup(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Delete [{}]",
				key_config.get_hint(key_config.keys.delete_tag),
			),
			"delete a tag",
			CMD_GROUP_GENERAL,
		)
	}
	pub fn select_tag(key_config: &SharedKeyConfig) -> CommandText {
		CommandText::new(
			format!(
				"Select commit [{}]",
				key_config.get_hint(key_config.keys.select_tag),
			),
			"Select commit in revlog",
			CMD_GROUP_LOG,
		)
	}

	pub fn status_push(key_config: &SharedKeyConfig) -> CommandText {
		CommandText::new(
			format!(
				"Push [{}]",
				key_config.get_hint(key_config.keys.push),
			),
			"push to origin",
			CMD_GROUP_GENERAL,
		)
	}
	pub fn status_force_push(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Force Push [{}]",
				key_config.get_hint(key_config.keys.force_push),
			),
			"force push to origin",
			CMD_GROUP_GENERAL,
		)
	}

	pub fn status_fetch(key_config: &SharedKeyConfig) -> CommandText {
		CommandText::new(
			format!(
				"Fetch [{}]",
				key_config.get_hint(key_config.keys.fetch),
			),
			"fetch",
			CMD_GROUP_GENERAL,
		)
	}
	pub fn status_pull(key_config: &SharedKeyConfig) -> CommandText {
		CommandText::new(
			format!(
				"Pull [{}]",
				key_config.get_hint(key_config.keys.pull),
			),
			"fetch/merge",
			CMD_GROUP_GENERAL,
		)
	}

	pub fn fetch_remotes(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Fetch [{}]",
				key_config.get_hint(key_config.keys.fetch),
			),
			"fetch/prune",
			CMD_GROUP_BRANCHES,
		)
	}

	pub fn find_commit_sha(
		key_config: &SharedKeyConfig,
	) -> CommandText {
		CommandText::new(
			format!(
				"Search Hash [{}]",
				key_config.get_hint(key_config.keys.find_commit_sha),
			),
			"find commit from sha",
			CMD_GROUP_LOG,
		)
	}
}
