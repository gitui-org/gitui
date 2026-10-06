pub const SYM_COMMIT: &str = "o";
pub const SYM_COMMIT_BRANCH: &str = "*";
pub const SYM_COMMIT_MERGE: &str = "M";
pub const SYM_COMMIT_STASH: &str = "*";
pub const SYM_COMMIT_UNCOMMITTED: &str = "+";
pub const SYM_VERTICAL: &str = "┃";
pub const SYM_VERTICAL_DOTTED: &str = "╏";
pub const SYM_HORIZONTAL: &str = "━";
pub const SYM_MERGE_BRIDGE_START: &str = "┓";
pub const SYM_MERGE_BRIDGE_MID: &str = "━";
pub const SYM_MERGE_BRIDGE_END: &str = "┏";
pub const SYM_BRANCH_UP: &str = "┛";
pub const SYM_BRANCH_UP_RIGHT: &str = "┗";
pub const SYM_TEE_LEFT: &str = "┫";
pub const SYM_TEE_RIGHT: &str = "┣";
pub const SYM_TEE_UP: &str = "┻";
pub const SYM_TEE_DOWN: &str = "┳";
pub const SYM_SPACE: &str = " ";

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn symbols_are_single_char_and_distinct() {
		let symbols = [
			SYM_COMMIT,
			SYM_COMMIT_BRANCH,
			SYM_COMMIT_MERGE,
			SYM_COMMIT_STASH,
			SYM_COMMIT_UNCOMMITTED,
			SYM_VERTICAL,
			SYM_VERTICAL_DOTTED,
			SYM_HORIZONTAL,
			SYM_MERGE_BRIDGE_START,
			SYM_MERGE_BRIDGE_MID,
			SYM_MERGE_BRIDGE_END,
			SYM_BRANCH_UP,
			SYM_BRANCH_UP_RIGHT,
			SYM_TEE_LEFT,
			SYM_TEE_RIGHT,
			SYM_TEE_UP,
			SYM_TEE_DOWN,
			SYM_SPACE,
		];

		for symbol in symbols {
			assert_eq!(
				symbol.chars().count(),
				1,
				"{symbol:?} should be a single glyph"
			);
		}
	}
}
