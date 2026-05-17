use unicode_segmentation::UnicodeSegmentation;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

///
pub fn trim_length_left(s: &str, width: usize) -> &str {
	let len = s.len();
	if len > width {
		for i in len - width..len {
			if s.is_char_boundary(i) {
				return &s[i..];
			}
		}
	}

	s
}

const TAB_WIDTH: usize = 8;

// TODO: allow customize tabsize (e.g. via .editorconfig)
pub fn tabs_to_spaces(input: String) -> String {
	if !input.contains('\t') {
		return input;
	}

	let mut out = String::with_capacity(input.len());
	let mut column = 0usize;

	for ch in input.chars() {
		match ch {
			'\t' => {
				let spaces = TAB_WIDTH - (column % TAB_WIDTH);
				out.extend(std::iter::repeat_n(' ', spaces));
				column += spaces;
			}
			'\n' => {
				out.push('\n');
				column = 0;
			}
			ch => {
				out.push(ch);
				column += ch.width().unwrap_or(0);
			}
		}
	}

	out
}

/// This function will return a str slice which start at specified offset.
/// As src is a unicode str, start offset has to be calculated with each character.
pub fn trim_offset(src: &str, mut offset: usize) -> &str {
	let mut start = 0;
	for c in UnicodeSegmentation::graphemes(src, true) {
		let w = c.width();
		if w <= offset {
			offset -= w;
			start += c.len();
		} else {
			break;
		}
	}
	&src[start..]
}

#[cfg(test)]
mod test {
	use pretty_assertions::assert_eq;

	use crate::string_utils::trim_length_left;

	#[test]
	fn test_trim() {
		assert_eq!(trim_length_left("👍foo", 3), "foo");
		assert_eq!(trim_length_left("👍foo", 4), "foo");
	}

	#[test]
	fn test_tabs_to_spaces() {
		use super::tabs_to_spaces;

		assert_eq!(tabs_to_spaces("no-tabs".into()), "no-tabs");
		assert_eq!(tabs_to_spaces("\tfoo".into()), "        foo");
		assert_eq!(tabs_to_spaces("a\tb".into()), "a       b");
	}
}
