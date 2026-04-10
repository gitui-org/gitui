use crate::{item::FileTreeItem, treeitems_iter::TreeItemsIterator};

pub struct TreeIterator<'a> {
	item_iter: TreeItemsIterator<'a>,
	selection: Option<usize>,
}

impl<'a> TreeIterator<'a> {
	pub const fn new(
		item_iter: TreeItemsIterator<'a>,
		selection: Option<usize>,
	) -> Self {
		Self {
			item_iter,
			selection,
		}
	}
}

impl<'a> Iterator for TreeIterator<'a> {
	type Item = (&'a FileTreeItem, bool);

	fn next(&mut self) -> Option<Self::Item> {
		self.item_iter.next().map(|(index, item)| {
			(item, self.selection.is_some_and(|i| i == index))
		})
	}
}
