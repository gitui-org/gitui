use crate::{filetreeitems::FileTreeItems, item::FileTreeItem};

pub struct TreeItemsIterator<'a> {
	tree: &'a FileTreeItems,
	index: usize,
	increments: Option<usize>,
	max_amount: usize,
}

impl<'a> TreeItemsIterator<'a> {
	pub const fn new(
		tree: &'a FileTreeItems,
		start: usize,
		max_amount: usize,
	) -> Self {
		TreeItemsIterator {
			max_amount,
			increments: None,
			index: start,
			tree,
		}
	}
}

impl<'a> Iterator for TreeItemsIterator<'a> {
	type Item = (usize, &'a FileTreeItem);

	fn next(&mut self) -> Option<Self::Item> {
		if self.increments.unwrap_or_default() < self.max_amount {
			let items = &self.tree.tree_items;

			let mut init = self.increments.is_none();

			if let Some(i) = self.increments.as_mut() {
				*i += 1;
			} else {
				self.increments = Some(0);
			}

			loop {
				if !init {
					self.index += 1;
				}
				init = false;

				if self.index >= self.tree.len() {
					break;
				}

				let elem = &items[self.index];

				if elem.info().is_visible() {
					return Some((self.index, &items[self.index]));
				}
			}
		}

		None
	}
}
