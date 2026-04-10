use crate::{strings, ui::style::SharedTheme};
use ratatui::text::Span;
use std::borrow::Cow;

pub enum Detail {
	Author,
	Date,
	Committer,
	Sha,
	Message,
}

pub fn style_detail<'a>(
	theme: &'a SharedTheme,
	field: &Detail,
) -> Span<'a> {
	match field {
		Detail::Author => Span::styled(
			Cow::from(strings::commit::details_author()),
			theme.text(false, false),
		),
		Detail::Date => Span::styled(
			Cow::from(strings::commit::details_date()),
			theme.text(false, false),
		),
		Detail::Committer => Span::styled(
			Cow::from(strings::commit::details_committer()),
			theme.text(false, false),
		),
		Detail::Sha => Span::styled(
			Cow::from(strings::commit::details_tags()),
			theme.text(false, false),
		),
		Detail::Message => Span::styled(
			Cow::from(strings::commit::details_message()),
			theme.text(false, false),
		),
	}
}
