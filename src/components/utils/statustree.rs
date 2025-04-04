use super::filetree::{
	FileTreeItem, FileTreeItemKind, FileTreeItems, PathCollapsed,
};
use anyhow::Result;
use asyncgit::StatusItem;
use std::{cell::Cell, cmp, collections::BTreeSet};

//TODO: use new `filetreelist` crate

///
#[derive(Default)]
pub struct StatusTree {
	pub tree: FileTreeItems,
	pub selection: Option<usize>,

	// some folders may be folded up, this allows jumping
	// over folders which are folded into their parent
	pub available_selections: Vec<usize>,

	pub window_height: Cell<Option<usize>>,
}

///
#[derive(Copy, Clone, Debug)]
pub enum MoveSelection {
	Up,
	Down,
	Left,
	Right,
	Home,
	End,
	PageDown,
	PageUp,
}

#[derive(Copy, Clone, Debug)]
struct SelectionChange {
	new_index: usize,
	changes: bool,
}
impl SelectionChange {
	const fn new(new_index: usize, changes: bool) -> Self {
		Self { new_index, changes }
	}
}

impl StatusTree {
	/// update tree with a new list, try to retain selection and collapse states
	pub fn update(&mut self, list: &[StatusItem]) -> Result<()> {
		let last_collapsed = self.all_collapsed();

		let last_selection =
			self.selected_item().map(|e| e.info.full_path);
		let last_selection_index = self.selection.unwrap_or(0);

		self.tree = FileTreeItems::new(list, &last_collapsed)?;
		self.selection = last_selection.as_ref().map_or_else(
			|| self.tree.items().first().map(|_| 0),
			|last_selection| {
				self.find_last_selection(
					last_selection,
					last_selection_index,
				)
				.or_else(|| self.tree.items().first().map(|_| 0))
			},
		);

		self.update_visibility(None, 0, true);
		self.available_selections = self.setup_available_selections();

		//NOTE: now that visibility is set we can make sure selection is visible
		if let Some(idx) = self.selection {
			self.selection = Some(self.find_visible_idx(idx));
		}

		Ok(())
	}

	/// Return which indices can be selected, taking into account that
	/// some folders may be folded up into their parent
	///
	/// It should be impossible to select a folder which has been folded into its parent
	fn setup_available_selections(&self) -> Vec<usize> {
		// use the same algorithm as in filetree build_vec_text_for_drawing function
		let mut should_skip_over: usize = 0;
		let mut vec_available_selections: Vec<usize> = vec![];
		let tree_items = self.tree.items();
		for index in 0..tree_items.len() {
			if should_skip_over > 0 {
				should_skip_over -= 1;
				continue;
			}
			let mut idx_temp = index;
			vec_available_selections.push(index);

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
				if self.tree.multiple_items_at_path(idx_temp) {
					should_skip_over -= 1;
					break;
				}
			}
		}
		vec_available_selections
	}

	fn find_visible_idx(&self, mut idx: usize) -> usize {
		while idx > 0 {
			if self.is_visible_index(idx) {
				break;
			}

			idx -= 1;
		}

		idx
	}

	///
	pub fn move_selection(&mut self, dir: MoveSelection) -> bool {
		self.selection.is_some_and(|selection| {
			let selection_change = match dir {
				MoveSelection::Up => {
					self.selection_updown(selection, true)
				}
				MoveSelection::Down => {
					self.selection_updown(selection, false)
				}
				MoveSelection::Left => self.selection_left(selection),
				MoveSelection::Right => {
					self.selection_right(selection)
				}
				MoveSelection::Home => SelectionChange::new(0, false),
				MoveSelection::End => self.selection_end(),
				MoveSelection::PageUp => self.selection_page_updown(
					selection,
					(0..=selection).rev(),
				),
				MoveSelection::PageDown => self
					.selection_page_updown(
						selection,
						selection..(self.tree.len()),
					),
			};

			let changed_index =
				selection_change.new_index != selection;

			self.selection = Some(selection_change.new_index);

			changed_index || selection_change.changes
		})
	}

	///
	pub fn selected_item(&self) -> Option<FileTreeItem> {
		self.selection.map(|i| self.tree[i].clone())
	}

	///
	pub fn is_empty(&self) -> bool {
		self.tree.items().is_empty()
	}

	fn all_collapsed(&self) -> BTreeSet<&String> {
		let mut res = BTreeSet::new();

		for i in self.tree.items() {
			if let FileTreeItemKind::Path(PathCollapsed(collapsed)) =
				i.kind
			{
				if collapsed {
					res.insert(&i.info.full_path);
				}
			}
		}

		res
	}

	fn find_last_selection(
		&self,
		last_selection: &str,
		last_index: usize,
	) -> Option<usize> {
		if self.is_empty() {
			return None;
		}

		if let Ok(i) = self.tree.items().binary_search_by(|e| {
			e.info.full_path.as_str().cmp(last_selection)
		}) {
			return Some(i);
		}

		Some(cmp::min(last_index, self.tree.len() - 1))
	}

	fn selection_updown(
		&self,
		current_index: usize,
		up: bool,
	) -> SelectionChange {
		let mut current_index_in_available_selections;
		let mut cur_index_find = current_index;
		if self.available_selections.is_empty() {
			// Go to top
			current_index_in_available_selections = 0;
		} else {
			loop {
				if let Some(pos) = self
					.available_selections
					.iter()
					.position(|i| *i == cur_index_find)
				{
					current_index_in_available_selections = pos;
					break;
				}

				// Find the closest to the index, usually this shouldn't happen
				if current_index == 0 {
					// This should never happen
					current_index_in_available_selections = 0;
					break;
				}
				cur_index_find -= 1;
			}
		}

		let mut new_index;

		loop {
			// Use available_selections to go to the correct selection as
			// some of the folders may be folded up
			new_index = if up {
				current_index_in_available_selections =
					current_index_in_available_selections
						.saturating_sub(1);
				self.available_selections
					[current_index_in_available_selections]
			} else if current_index_in_available_selections
				.saturating_add(1)
				<= self.available_selections.len().saturating_sub(1)
			{
				current_index_in_available_selections =
					current_index_in_available_selections
						.saturating_add(1);
				self.available_selections
					[current_index_in_available_selections]
			} else {
				// can't move down anymore
				new_index = current_index;
				break;
			};

			if self.is_visible_index(new_index) {
				break;
			}
		}
		SelectionChange::new(new_index, false)
	}

	fn selection_end(&self) -> SelectionChange {
		let items_max = self.tree.len().saturating_sub(1);

		let mut new_index = items_max;

		loop {
			if self.is_visible_index(new_index) {
				break;
			}

			if new_index == 0 {
				break;
			}

			new_index = new_index.saturating_sub(1);
			new_index = cmp::min(new_index, items_max);
		}

		SelectionChange::new(new_index, false)
	}

	fn selection_page_updown(
		&self,
		current_index: usize,
		range: impl Iterator<Item = usize>,
	) -> SelectionChange {
		let page_size = self.window_height.get().unwrap_or(0);

		let new_index = range
			.filter(|index| {
				self.available_selections.contains(index)
					&& self.is_visible_index(*index)
			})
			.take(page_size)
			.last()
			.unwrap_or(current_index);

		SelectionChange::new(new_index, false)
	}

	fn is_visible_index(&self, idx: usize) -> bool {
		self.tree[idx].info.visible
	}

	fn selection_right(
		&mut self,
		current_selection: usize,
	) -> SelectionChange {
		let item_kind = self.tree[current_selection].kind.clone();
		let item_path =
			self.tree[current_selection].info.full_path.clone();

		match item_kind {
			FileTreeItemKind::Path(PathCollapsed(collapsed))
				if collapsed =>
			{
				self.expand(&item_path, current_selection);
				return SelectionChange::new(current_selection, true);
			}
			FileTreeItemKind::Path(PathCollapsed(collapsed))
				if !collapsed =>
			{
				return self
					.selection_updown(current_selection, false);
			}
			_ => (),
		}

		SelectionChange::new(current_selection, false)
	}

	fn selection_left(
		&mut self,
		current_selection: usize,
	) -> SelectionChange {
		let item_kind = self.tree[current_selection].kind.clone();
		let item_path =
			self.tree[current_selection].info.full_path.clone();

		if matches!(item_kind, FileTreeItemKind::File(_))
			|| matches!(item_kind,FileTreeItemKind::Path(PathCollapsed(collapsed))
        if collapsed)
		{
			let mut cur_parent =
				self.tree.find_parent_index(current_selection);
			while !self.available_selections.contains(&cur_parent)
				&& cur_parent != 0
			{
				cur_parent = self.tree.find_parent_index(cur_parent);
			}
			SelectionChange::new(cur_parent, false)
		} else if matches!(item_kind,  FileTreeItemKind::Path(PathCollapsed(collapsed))
        if !collapsed)
		{
			self.collapse(&item_path, current_selection);
			SelectionChange::new(current_selection, true)
		} else {
			SelectionChange::new(current_selection, false)
		}
	}

	fn collapse(&mut self, path: &str, index: usize) {
		if let FileTreeItemKind::Path(PathCollapsed(
			ref mut collapsed,
		)) = self.tree[index].kind
		{
			*collapsed = true;
		}

		let path = format!("{path}/");

		for i in index + 1..self.tree.len() {
			let item = &mut self.tree[i];
			let item_path = &item.info.full_path;
			if item_path.starts_with(&path) {
				item.info.visible = false;
			} else {
				return;
			}
		}
	}

	fn expand(&mut self, path: &str, current_index: usize) {
		if let FileTreeItemKind::Path(PathCollapsed(
			ref mut collapsed,
		)) = self.tree[current_index].kind
		{
			*collapsed = false;
		}

		let path = format!("{path}/");

		self.update_visibility(
			Some(path.as_str()),
			current_index + 1,
			false,
		);
	}

	fn update_visibility(
		&mut self,
		prefix: Option<&str>,
		start_idx: usize,
		set_defaults: bool,
	) {
		// if we are in any subpath that is collapsed we keep skipping over it
		let mut inner_collapsed: Option<String> = None;

		for i in start_idx..self.tree.len() {
			if let Some(ref collapsed_path) = inner_collapsed {
				let p: &String = &self.tree[i].info.full_path;
				if p.starts_with(collapsed_path) {
					if set_defaults {
						self.tree[i].info.visible = false;
					}
					// we are still in a collapsed inner path
					continue;
				}
				inner_collapsed = None;
			}

			let item_kind = self.tree[i].kind.clone();
			let item_path = &self.tree[i].info.full_path;

			if matches!(item_kind, FileTreeItemKind::Path(PathCollapsed(collapsed)) if collapsed)
			{
				// we encountered an inner path that is still collapsed
				inner_collapsed = Some(format!("{}/", &item_path));
			}

			if prefix
				.map_or(true, |prefix| item_path.starts_with(prefix))
			{
				self.tree[i].info.visible = true;
			} else {
				// if we do not set defaults we can early out
				if set_defaults {
					self.tree[i].info.visible = false;
				} else {
					return;
				}
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use asyncgit::StatusItemType;

	fn string_vec_to_status(items: &[&str]) -> Vec<StatusItem> {
		items
			.iter()
			.map(|a| StatusItem {
				path: String::from(*a),
				status: StatusItemType::Modified,
			})
			.collect::<Vec<_>>()
	}

	fn get_visible(tree: &StatusTree) -> Vec<bool> {
		tree.tree
			.items()
			.iter()
			.map(|e| e.info.visible)
			.collect::<Vec<_>>()
	}

	#[test]
	fn test_selection() {
		let items = string_vec_to_status(&[
			"a/b", //
		]);

		let mut res = StatusTree::default();
		res.update(&items).unwrap();

		assert!(res.move_selection(MoveSelection::Down));

		assert_eq!(res.selection, Some(1));

		assert!(res.move_selection(MoveSelection::Left));

		assert_eq!(res.selection, Some(0));
	}

	#[test]
	fn test_keep_selected_item() {
		let mut res = StatusTree::default();
		res.update(&string_vec_to_status(&["b"])).unwrap();

		assert_eq!(res.selection, Some(0));

		res.update(&string_vec_to_status(&["a", "b"])).unwrap();

		assert_eq!(res.selection, Some(1));
	}

	#[test]
	fn test_keep_selected_index() {
		let mut res = StatusTree::default();
		res.update(&string_vec_to_status(&["a", "b"])).unwrap();
		res.selection = Some(1);

		res.update(&string_vec_to_status(&["d", "c", "a"])).unwrap();
		assert_eq!(res.selection, Some(1));
	}

	#[test]
	fn test_keep_selected_index_if_not_collapsed() {
		let mut res = StatusTree::default();
		res.update(&string_vec_to_status(&["a/b", "c"])).unwrap();

		res.collapse("a/b", 0);

		res.selection = Some(2);

		res.update(&string_vec_to_status(&["a/b"])).unwrap();
		assert_eq!(
			get_visible(&res),
			vec![
				true,  //
				false, //
			]
		);
		assert!(res.is_visible_index(res.selection.unwrap()));
		assert_eq!(res.selection, Some(0));
	}

	#[test]
	fn test_keep_collapsed_states() {
		let mut res = StatusTree::default();
		res.update(&string_vec_to_status(&[
			"a/b", //
			"c",
		]))
		.unwrap();

		res.collapse("a", 0);

		assert_eq!(
			res.all_collapsed().iter().collect::<Vec<_>>(),
			vec![&&String::from("a")]
		);

		assert_eq!(
			get_visible(&res),
			vec![
				true,  //
				false, //
				true,  //
			]
		);

		res.update(&string_vec_to_status(&[
			"a/b", //
			"c",   //
			"d",
		]))
		.unwrap();

		assert_eq!(
			res.all_collapsed().iter().collect::<Vec<_>>(),
			vec![&&String::from("a")]
		);

		assert_eq!(
			get_visible(&res),
			vec![
				true,  //
				false, //
				true,  //
				true
			]
		);
	}

	#[test]
	fn test_expand() {
		let items = string_vec_to_status(&[
			"a/b/c", //
			"a/d",   //
		]);

		//0 a/
		//1   b/
		//2     c
		//3   d

		let mut res = StatusTree::default();
		res.update(&items).unwrap();

		res.collapse(&String::from("a/b"), 1);

		let visible = get_visible(&res);

		assert_eq!(
			visible,
			vec![
				true,  //
				true,  //
				false, //
				true,
			]
		);

		res.expand(&String::from("a/b"), 1);

		let visible = get_visible(&res);

		assert_eq!(
			visible,
			vec![
				true, //
				true, //
				true, //
				true,
			]
		);
	}

	#[test]
	fn test_expand_bug() {
		let items = string_vec_to_status(&[
			"a/b/c",  //
			"a/b2/d", //
		]);

		//0 a/
		//1   b/
		//2     c
		//3   b2/
		//4     d

		let mut res = StatusTree::default();
		res.update(&items).unwrap();

		res.collapse(&String::from("b"), 1);
		res.collapse(&String::from("a"), 0);

		assert_eq!(
			get_visible(&res),
			vec![
				true,  //
				false, //
				false, //
				false, //
				false,
			]
		);

		res.expand(&String::from("a"), 0);

		assert_eq!(
			get_visible(&res),
			vec![
				true,  //
				true,  //
				false, //
				true,  //
				true,
			]
		);
	}

	#[test]
	fn test_collapse_too_much() {
		let items = string_vec_to_status(&[
			"a/b",  //
			"a2/c", //
		]);

		//0 a/
		//1   b
		//2 a2/
		//3   c

		let mut res = StatusTree::default();
		res.update(&items).unwrap();

		res.collapse(&String::from("a"), 0);

		let visible = get_visible(&res);

		assert_eq!(
			visible,
			vec![
				true,  //
				false, //
				true,  //
				true,
			]
		);
	}

	#[test]
	fn test_expand_with_collapsed_sub_parts() {
		let items = string_vec_to_status(&[
			"a/b/c", //
			"a/d",   //
		]);

		//0 a/
		//1   b/
		//2     c
		//3   d

		let mut res = StatusTree::default();
		res.update(&items).unwrap();

		res.collapse(&String::from("a/b"), 1);

		let visible = get_visible(&res);

		assert_eq!(
			visible,
			vec![
				true,  //
				true,  //
				false, //
				true,
			]
		);

		res.collapse(&String::from("a"), 0);

		let visible = get_visible(&res);

		assert_eq!(
			visible,
			vec![
				true,  //
				false, //
				false, //
				false,
			]
		);

		res.expand(&String::from("a"), 0);

		let visible = get_visible(&res);

		assert_eq!(
			visible,
			vec![
				true,  //
				true,  //
				false, //
				true,
			]
		);
	}

	#[test]
	fn test_selection_skips_collapsed() {
		let items = string_vec_to_status(&[
			"a/b/c", //
			"a/d",   //
		]);

		//0 a/
		//1   b/
		//2     c
		//3   d

		let mut res = StatusTree::default();
		res.update(&items).unwrap();
		res.collapse(&String::from("a/b"), 1);
		res.selection = Some(1);

		assert!(res.move_selection(MoveSelection::Down));

		assert_eq!(res.selection, Some(3));
	}

	#[test]
	fn test_folders_fold_up_if_alone_in_directory() {
		let items = string_vec_to_status(&[
			"a/b/c/d", //
			"a/e/f/g", //
			"a/h/i/j", //
		]);

		//0 a/
		//1   b/
		//2     c/
		//3       d
		//4   e/
		//5     f/
		//6       g
		//7   h/
		//8     i/
		//9       j

		//0 a/
		//1   b/c/
		//3       d
		//4   e/f/
		//6       g
		//7   h/i/
		//9       j

		let mut res = StatusTree::default();
		res.update(&items).unwrap();
		res.selection = Some(0);

		assert!(res.move_selection(MoveSelection::Down));
		assert_eq!(res.selection, Some(1));

		assert!(res.move_selection(MoveSelection::Down));
		assert_eq!(res.selection, Some(3));

		assert!(res.move_selection(MoveSelection::Down));
		assert_eq!(res.selection, Some(4));

		assert!(res.move_selection(MoveSelection::Down));
		assert_eq!(res.selection, Some(6));

		assert!(res.move_selection(MoveSelection::Down));
		assert_eq!(res.selection, Some(7));

		assert!(res.move_selection(MoveSelection::Down));
		assert_eq!(res.selection, Some(9));
	}

	#[test]
	fn test_folders_fold_up_if_alone_in_directory_2() {
		let items = string_vec_to_status(&["a/b/c/d/e/f/g/h"]);

		//0 a/
		//1   b/
		//2     c/
		//3       d/
		//4         e/
		//5           f/
		//6             g/
		//7               h

		//0 a/b/c/d/e/f/g/
		//7               h

		let mut res = StatusTree::default();
		res.update(&items).unwrap();
		res.selection = Some(0);

		assert!(res.move_selection(MoveSelection::Down));
		assert_eq!(res.selection, Some(7));
	}

	#[test]
	fn test_folders_fold_up_down_with_selection_left_right() {
		let items = string_vec_to_status(&[
			"a/b/c/d", //
			"a/e/f/g", //
			"a/h/i/j", //
		]);

		//0 a/
		//1   b/
		//2     c/
		//3       d
		//4   e/
		//5     f/
		//6       g
		//7   h/
		//8     i/
		//9       j

		//0 a/
		//1   b/c/
		//3       d
		//4   e/f/
		//6       g
		//7   h/i/
		//9       j

		let mut res = StatusTree::default();
		res.update(&items).unwrap();
		res.selection = Some(0);

		assert!(res.move_selection(MoveSelection::Left));
		assert_eq!(res.selection, Some(0));

		// These should do nothing
		res.move_selection(MoveSelection::Left);
		res.move_selection(MoveSelection::Left);
		assert_eq!(res.selection, Some(0));
		//
		assert!(res.move_selection(MoveSelection::Right)); // unfold 0
		assert_eq!(res.selection, Some(0));

		assert!(res.move_selection(MoveSelection::Right)); // move to 1
		assert_eq!(res.selection, Some(1));

		assert!(res.move_selection(MoveSelection::Left)); // fold 1
		assert!(res.move_selection(MoveSelection::Down)); // move to 4
		assert_eq!(res.selection, Some(4));

		assert!(res.move_selection(MoveSelection::Left)); // fold 4
		assert!(res.move_selection(MoveSelection::Down)); // move to 7
		assert_eq!(res.selection, Some(7));

		assert!(res.move_selection(MoveSelection::Right)); // move to 9
		assert_eq!(res.selection, Some(9));

		assert!(res.move_selection(MoveSelection::Left)); // move to 7
		assert_eq!(res.selection, Some(7));

		assert!(res.move_selection(MoveSelection::Left)); // folds 7
		assert_eq!(res.selection, Some(7));

		assert!(res.move_selection(MoveSelection::Left)); // jump to 0
		assert_eq!(res.selection, Some(0));
	}
}
