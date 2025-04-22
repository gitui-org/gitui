use super::{
	utils::{
		filetree::{FileTreeItem, FileTreeItemKind},
		statustree::{MoveSelection, StatusTree},
	},
	CommandBlocking, DrawableComponent,
};
use crate::{
	app::Environment,
	components::{CommandInfo, Component, EventState},
	keys::{key_match, SharedKeyConfig},
	popups::{BlameFileOpen, FileRevOpen},
	queue::{InternalEvent, NeedsUpdate, Queue, StackablePopupOpen},
	strings::{self, order},
	ui::{self, style::SharedTheme},
};
use anyhow::Result;
use asyncgit::{hash, sync::CommitId, StatusItem, StatusItemType};
use crossterm::event::Event;
use ratatui::{layout::Rect, text::Span, Frame};
use std::{borrow::Cow, cell::Cell, path::Path};

//TODO: use new `filetreelist` crate

///
#[allow(clippy::struct_excessive_bools)]
pub struct StatusTreeComponent {
	title: String,
	tree: StatusTree,
	pending: bool,
	current_hash: u64,
	focused: bool,
	show_selection: bool,
	queue: Queue,
	theme: SharedTheme,
	key_config: SharedKeyConfig,
	scroll_top: Cell<usize>,
	visible: bool,
	revision: Option<CommitId>,
}

impl StatusTreeComponent {
	///
	pub fn new(env: &Environment, title: &str, focus: bool) -> Self {
		Self {
			title: title.to_string(),
			tree: StatusTree::default(),
			current_hash: 0,
			focused: focus,
			show_selection: focus,
			queue: env.queue.clone(),
			theme: env.theme.clone(),
			key_config: env.key_config.clone(),
			scroll_top: Cell::new(0),
			pending: true,
			visible: false,
			revision: None,
		}
	}

	pub fn set_commit(&mut self, revision: Option<CommitId>) {
		self.revision = revision;
	}

	///
	pub fn update(&mut self, list: &[StatusItem]) -> Result<()> {
		self.pending = false;

		let new_hash = hash(list);
		if self.current_hash != new_hash {
			self.tree.update(list)?;
			self.current_hash = new_hash;
		}

		Ok(())
	}

	///
	pub fn selection(&self) -> Option<FileTreeItem> {
		self.tree.selected_item()
	}

	///
	pub fn selection_file(&self) -> Option<StatusItem> {
		self.tree.selected_item().and_then(|f| {
			if let FileTreeItemKind::File(f) = f.kind {
				Some(f)
			} else {
				None
			}
		})
	}

	///
	pub fn show_selection(&mut self, show: bool) {
		self.show_selection = show;
	}

	/// returns true if list is empty
	pub fn is_empty(&self) -> bool {
		self.tree.is_empty()
	}

	///
	pub const fn file_count(&self) -> usize {
		self.tree.tree.file_count()
	}

	///
	pub fn set_title(&mut self, title: String) {
		self.title = title;
	}

	///
	pub fn clear(&mut self) -> Result<()> {
		self.current_hash = 0;
		self.pending = true;
		self.tree.update(&[])
	}

	///
	pub fn is_file_selected(&self) -> bool {
		self.tree.selected_item().is_some_and(|item| {
			match item.kind {
				FileTreeItemKind::File(_) => true,
				FileTreeItemKind::Path(..) => false,
			}
		})
	}

	fn move_selection(&mut self, dir: MoveSelection) -> bool {
		let changed = self.tree.move_selection(dir);

		if changed {
			self.queue.push(InternalEvent::Update(NeedsUpdate::DIFF));
		}

		changed
	}

	const fn item_status_char(item_type: StatusItemType) -> char {
		match item_type {
			StatusItemType::Modified => 'M',
			StatusItemType::New => '+',
			StatusItemType::Deleted => '-',
			StatusItemType::Renamed => 'R',
			StatusItemType::Typechange => ' ',
			StatusItemType::Conflicted => '!',
		}
	}

	fn item_to_text<'b>(
		string: &str,
		indent: usize,
		visible: bool,
		file_item_kind: &FileTreeItemKind,
		width: u16,
		selected: bool,
		theme: &'b SharedTheme,
	) -> Option<Span<'b>> {
		let indent_str = if indent == 0 {
			String::new()
		} else {
			format!("{:w$}", " ", w = indent * 2)
		};

		if !visible {
			return None;
		}

		match file_item_kind {
			FileTreeItemKind::File(status_item) => {
				let status_char =
					Self::item_status_char(status_item.status);
				let file = Path::new(&status_item.path)
					.file_name()
					.and_then(std::ffi::OsStr::to_str)
					.expect("invalid path.");

				let txt = if selected {
					format!(
						"{} {}{:w$}",
						status_char,
						indent_str,
						file,
						w = width as usize
					)
				} else {
					format!("{status_char} {indent_str}{file}")
				};

				Some(Span::styled(
					Cow::from(txt),
					theme.item(status_item.status, selected),
				))
			}

			FileTreeItemKind::Path(path_collapsed) => {
				let collapse_char =
					if path_collapsed.0 { '▸' } else { '▾' };

				let txt = if selected {
					format!(
						"  {}{}{:w$}",
						indent_str,
						collapse_char,
						string,
						w = width as usize
					)
				} else {
					format!("  {indent_str}{collapse_char}{string}",)
				};

				Some(Span::styled(
					Cow::from(txt),
					theme.text(true, selected),
				))
			}
		}
	}

	/// Returns a `Vec<TextDrawInfo>` which is used to draw the `FileTreeComponent` correctly,
	/// allowing folders to be folded up if they are alone in their directory
	fn build_vec_text_draw_info_for_drawing(
		&self,
	) -> (Vec<TextDrawInfo>, usize, usize) {
		let mut should_skip_over: usize = 0;
		let mut selection_offset: usize = 0;
		let mut selection_offset_visible: usize = 0;
		let mut vec_draw_text_info: Vec<TextDrawInfo> = vec![];
		let tree_items = self.tree.tree.items();

		for (index, item) in tree_items.iter().enumerate() {
			if should_skip_over > 0 {
				should_skip_over -= 1;
				continue;
			}

			let index_above_select =
				index < self.tree.selection.unwrap_or(0);

			if !item.info.visible && index_above_select {
				selection_offset_visible += 1;
			}

			vec_draw_text_info.push(TextDrawInfo {
				name: item.info.path.clone(),
				indent: item.info.indent,
				visible: item.info.visible,
				item_kind: &item.kind,
			});

			let mut idx_temp = index;

			while idx_temp < tree_items.len().saturating_sub(2)
				&& tree_items[idx_temp].info.indent
					< tree_items[idx_temp + 1].info.indent
			{
				// fold up the folder/file
				idx_temp += 1;
				should_skip_over += 1;

				// don't fold files up
				if let FileTreeItemKind::File(_) =
					&tree_items[idx_temp].kind
				{
					should_skip_over -= 1;
					break;
				}
				// don't fold up if more than one folder in folder
				else if self
					.tree
					.tree
					.multiple_items_at_path(idx_temp)
				{
					should_skip_over -= 1;
					break;
				}

				// There is only one item at this level (i.e only one folder in the folder),
				// so do fold up

				let vec_draw_text_info_len = vec_draw_text_info.len();
				vec_draw_text_info[vec_draw_text_info_len - 1]
					.name += &(String::from("/")
					+ &tree_items[idx_temp].info.path);
				if index_above_select {
					selection_offset += 1;
				}
			}
		}
		(
			vec_draw_text_info,
			selection_offset,
			selection_offset_visible,
		)
	}

	// Copy the real path of selected file to clickboard
	fn copy_file_path(&self) {
		if let Some(item) = self.selection() {
			if crate::clipboard::copy_string(&item.info.full_path)
				.is_err()
			{
				self.queue.push(InternalEvent::ShowErrorMsg(
					strings::POPUP_FAIL_COPY.to_string(),
				));
			}
		}
	}
}

/// Used for drawing the `FileTreeComponent`
struct TextDrawInfo<'a> {
	name: String,
	indent: u8,
	visible: bool,
	item_kind: &'a FileTreeItemKind,
}

impl DrawableComponent for StatusTreeComponent {
	fn draw(&self, f: &mut Frame, r: Rect) -> Result<()> {
		if !self.is_visible() {
			return Ok(());
		}

		if self.pending {
			let items = vec![Span::styled(
				Cow::from(strings::loading_text(&self.key_config)),
				self.theme.text(false, false),
			)];

			ui::draw_list(
				f,
				r,
				self.title.as_str(),
				items.into_iter(),
				self.focused,
				&self.theme,
			);
		} else {
			let (
				vec_draw_text_info,
				selection_offset,
				selection_offset_visible,
			) = self.build_vec_text_draw_info_for_drawing();

			let select = self
				.tree
				.selection
				.map(|idx| idx.saturating_sub(selection_offset))
				.unwrap_or_default();
			let tree_height = r.height.saturating_sub(2) as usize;
			self.tree.window_height.set(Some(tree_height));

			self.scroll_top.set(ui::calc_scroll_top(
				self.scroll_top.get(),
				tree_height,
				select.saturating_sub(selection_offset_visible),
			));

			let items = vec_draw_text_info
				.iter()
				.enumerate()
				.filter_map(|(index, draw_text_info)| {
					Self::item_to_text(
						&draw_text_info.name,
						draw_text_info.indent as usize,
						draw_text_info.visible,
						draw_text_info.item_kind,
						r.width,
						self.show_selection && select == index,
						&self.theme,
					)
				})
				.skip(self.scroll_top.get());

			ui::draw_list(
				f,
				r,
				self.title.as_str(),
				items,
				self.focused,
				&self.theme,
			);
		}

		Ok(())
	}
}

impl Component for StatusTreeComponent {
	fn commands(
		&self,
		out: &mut Vec<CommandInfo>,
		force_all: bool,
	) -> CommandBlocking {
		out.push(
			CommandInfo::new(
				strings::commands::navigate_tree(&self.key_config),
				!self.is_empty(),
				self.focused || force_all,
			)
			.order(order::NAV),
		);

		out.push(
			CommandInfo::new(
				strings::commands::blame_file(&self.key_config),
				self.selection_file().is_some(),
				self.focused || force_all,
			)
			.order(order::RARE_ACTION),
		);

		out.push(
			CommandInfo::new(
				strings::commands::open_file_history(
					&self.key_config,
				),
				self.selection_file().is_some(),
				self.focused || force_all,
			)
			.order(order::RARE_ACTION),
		);

		out.push(
			CommandInfo::new(
				strings::commands::edit_item(&self.key_config),
				self.selection_file().is_some(),
				self.focused || force_all,
			)
			.order(order::RARE_ACTION),
		);

		out.push(
			CommandInfo::new(
				strings::commands::copy_path(&self.key_config),
				self.selection_file().is_some(),
				self.focused || force_all,
			)
			.order(order::RARE_ACTION),
		);

		CommandBlocking::PassingOn
	}

	fn event(&mut self, ev: &Event) -> Result<EventState> {
		if self.focused {
			if let Event::Key(e) = ev {
				return if key_match(e, self.key_config.keys.blame) {
					if let Some(status_item) = self.selection_file() {
						self.hide();
						self.queue.push(InternalEvent::OpenPopup(
							StackablePopupOpen::BlameFile(
								BlameFileOpen {
									file_path: status_item.path,
									commit_id: self.revision,
									selection: None,
								},
							),
						));
					}
					Ok(EventState::Consumed)
				} else if key_match(
					e,
					self.key_config.keys.file_history,
				) {
					if let Some(status_item) = self.selection_file() {
						self.hide();
						self.queue.push(InternalEvent::OpenPopup(
							StackablePopupOpen::FileRevlog(
								FileRevOpen::new(status_item.path),
							),
						));
					}
					Ok(EventState::Consumed)
				} else if key_match(e, self.key_config.keys.edit_file)
				{
					if let Some(status_item) = self.selection_file() {
						self.queue.push(
							InternalEvent::OpenExternalEditor(Some(
								status_item.path,
							)),
						);
					}
					Ok(EventState::Consumed)
				} else if key_match(e, self.key_config.keys.copy) {
					self.copy_file_path();
					Ok(EventState::Consumed)
				} else if key_match(e, self.key_config.keys.move_down)
				{
					Ok(self
						.move_selection(MoveSelection::Down)
						.into())
				} else if key_match(e, self.key_config.keys.move_up) {
					Ok(self.move_selection(MoveSelection::Up).into())
				} else if key_match(e, self.key_config.keys.home)
					|| key_match(e, self.key_config.keys.shift_up)
				{
					Ok(self
						.move_selection(MoveSelection::Home)
						.into())
				} else if key_match(e, self.key_config.keys.end)
					|| key_match(e, self.key_config.keys.shift_down)
				{
					Ok(self.move_selection(MoveSelection::End).into())
				} else if key_match(e, self.key_config.keys.page_up) {
					Ok(self
						.move_selection(MoveSelection::PageUp)
						.into())
				} else if key_match(e, self.key_config.keys.page_down)
				{
					Ok(self
						.move_selection(MoveSelection::PageDown)
						.into())
				} else if key_match(e, self.key_config.keys.move_left)
				{
					Ok(self
						.move_selection(MoveSelection::Left)
						.into())
				} else if key_match(
					e,
					self.key_config.keys.move_right,
				) {
					Ok(self
						.move_selection(MoveSelection::Right)
						.into())
				} else {
					Ok(EventState::NotConsumed)
				};
			}
		}

		Ok(EventState::NotConsumed)
	}

	fn focused(&self) -> bool {
		self.focused
	}
	fn focus(&mut self, focus: bool) {
		self.focused = focus;
		self.show_selection(focus);
	}

	fn is_visible(&self) -> bool {
		self.visible
	}

	fn hide(&mut self) {
		self.visible = false;
	}

	fn show(&mut self) -> Result<()> {
		self.visible = true;
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn string_vec_to_status(items: &[&str]) -> Vec<StatusItem> {
		items
			.iter()
			.map(|a| StatusItem {
				path: String::from(*a),
				status: StatusItemType::Modified,
			})
			.collect::<Vec<_>>()
	}

	#[test]
	fn test_correct_scroll_position() {
		let items = string_vec_to_status(&[
			"a/b/b1", //
			"a/b/b2", //
			"a/c/c1", //
		]);

		//0 a/
		//1   b/
		//2     b1
		//3     b2
		//4  c/
		//5    c1

		// Set up test terminal
		let test_backend =
			ratatui::backend::TestBackend::new(100, 100);
		let mut terminal = ratatui::Terminal::new(test_backend)
			.expect("Unable to set up terminal");
		let mut frame = terminal.get_frame();

		// set up file tree
		let mut ftc = StatusTreeComponent::new(
			&Environment::test_env(),
			"title",
			true,
		);
		ftc.update(&items)
			.expect("Updating FileTreeComponent failed");

		ftc.move_selection(MoveSelection::Down); // Move to b/
		ftc.move_selection(MoveSelection::Left); // Fold b/
		ftc.move_selection(MoveSelection::Down); // Move to c/

		ftc.draw(&mut frame, Rect::new(0, 0, 10, 5))
			.expect("Draw failed");

		assert_eq!(ftc.scroll_top.get(), 0); // should still be at top
	}

	#[test]
	fn test_correct_foldup_and_not_visible_scroll_position() {
		let items = string_vec_to_status(&[
			"a/b/b1", //
			"c/d1",   //
			"c/d2",   //
		]);

		//0 a/b/
		//2     b1
		//3 c/
		//4   d1
		//5   d2

		// Set up test terminal
		let test_backend =
			ratatui::backend::TestBackend::new(100, 100);
		let mut terminal = ratatui::Terminal::new(test_backend)
			.expect("Unable to set up terminal");
		let mut frame = terminal.get_frame();

		// set up file tree
		let mut ftc = StatusTreeComponent::new(
			&Environment::test_env(),
			"title",
			true,
		);
		ftc.update(&items)
			.expect("Updating FileTreeComponent failed");

		ftc.move_selection(MoveSelection::Left); // Fold a/b/
		ftc.move_selection(MoveSelection::Down); // Move to c/

		ftc.draw(&mut frame, Rect::new(0, 0, 10, 5))
			.expect("Draw failed");

		assert_eq!(ftc.scroll_top.get(), 0); // should still be at top
	}
}
