use crate::{
	error::Error,
	item::{FileTreeItemKind, PathCollapsed},
	FileTreeItem,
};
use crate::{error::Result, treeitems_iter::TreeItemsIterator};
use std::{
	collections::{BTreeSet, HashMap},
	path::{Path, PathBuf},
};

///
#[derive(Default)]
pub struct FileTreeItems {
	pub tree_items: Vec<FileTreeItem>,
	files: usize,
}

impl FileTreeItems {
	///
	pub fn new(
		list: &[&Path],
		collapsed: &BTreeSet<&String>,
	) -> Result<Self> {
		let (mut items, paths) = Self::create_items(list, collapsed)?;

		Self::fold_paths(&mut items, &paths);

		Ok(Self {
			tree_items: items,
			files: list.len(),
		})
	}

	fn create_items<'a>(
		list: &'a [&Path],
		collapsed: &BTreeSet<&String>,
	) -> Result<(Vec<FileTreeItem>, HashMap<&'a Path, usize>)> {
		// scopetime::scope_time!("create_items");

		let mut items = Vec::with_capacity(list.len());
		let mut paths_added: HashMap<&Path, usize> =
			HashMap::with_capacity(list.len());

		for e in list {
			{
				Self::push_dirs(
					e,
					&mut items,
					&mut paths_added,
					collapsed,
				)?;
			}

			items.push(FileTreeItem::new_file(e)?);
		}

		Ok((items, paths_added))
	}

	/// how many individual items (files/paths) are in the list
	pub fn len(&self) -> usize {
		self.tree_items.len()
	}

	/// how many files were added to this list
	pub const fn file_count(&self) -> usize {
		self.files
	}

	/// iterates visible elements
	pub const fn iterate(
		&self,
		start: usize,
		max_amount: usize,
	) -> TreeItemsIterator<'_> {
		TreeItemsIterator::new(self, start, max_amount)
	}

	fn push_dirs<'a>(
		item_path: &'a Path,
		nodes: &mut Vec<FileTreeItem>,
		// helps to only add new nodes for paths that were not added before
		// we also count the number of children a node has for later folding
		paths_added: &mut HashMap<&'a Path, usize>,
		collapsed: &BTreeSet<&String>,
	) -> Result<()> {
		let mut ancestors =
			item_path.ancestors().skip(1).collect::<Vec<_>>();
		ancestors.reverse();

		for c in &ancestors {
			if c.parent().is_some() && !paths_added.contains_key(c) {
				// add node and set count to have no children
				paths_added.insert(c, 0);

				// increase the number of children in the parent node count
				if let Some(parent) = c.parent() {
					if !parent.as_os_str().is_empty() {
						*paths_added.entry(parent).or_insert(0) += 1;
					}
				}

				//TODO: make non alloc
				let path_string = Self::path_to_string(c)?;
				let is_collapsed = collapsed.contains(&path_string);
				nodes.push(FileTreeItem::new_path(c, is_collapsed)?);
			}
		}

		// increase child count in parent node (the above ancenstor ignores the leaf component)
		if let Some(parent) = item_path.parent() {
			*paths_added.entry(parent).or_insert(0) += 1;
		}

		Ok(())
	}

	//TODO: return ref
	fn path_to_string(p: &Path) -> Result<String> {
		Ok(p.to_str()
			.map_or_else(
				|| Err(Error::InvalidPath(p.to_path_buf())),
				Ok,
			)?
			.to_string())
	}

	pub fn collapse(&mut self, index: usize, recursive: bool) {
		if self.tree_items[index].kind().is_path() {
			self.tree_items[index].collapse_path();

			let path = PathBuf::from(
				self.tree_items[index].info().full_path_str(),
			);

			for i in index + 1..self.tree_items.len() {
				let item = &mut self.tree_items[i];

				if recursive && item.kind().is_path() {
					item.collapse_path();
				}

				let item_path =
					Path::new(item.info().full_path_str());

				if item_path.starts_with(&path) {
					item.hide();
				} else {
					break;
				}
			}
		}
	}

	pub fn expand(&mut self, index: usize, recursive: bool) {
		if self.tree_items[index].kind().is_path() {
			self.tree_items[index].expand_path();

			let full_path = PathBuf::from(
				self.tree_items[index].info().full_path_str(),
			);

			if recursive {
				for i in index + 1..self.tree_items.len() {
					let item = &mut self.tree_items[i];

					if !Path::new(item.info().full_path_str())
						.starts_with(&full_path)
					{
						break;
					}

					if item.kind().is_path()
						&& item.kind().is_path_collapsed()
					{
						item.expand_path();
					}
				}
			}

			self.update_visibility(
				Some(full_path).as_ref(),
				index + 1,
				false,
			);
		}
	}

	/// makes sure `index` is visible.
	/// this expands all parents and shows all siblings
	pub fn show_element(&mut self, index: usize) -> Option<usize> {
		Some(
			self.show_element_upward(index)?
				+ self.show_element_downward(index)?,
		)
	}

	fn show_element_upward(&mut self, index: usize) -> Option<usize> {
		let mut shown = 0_usize;

		let item = self.tree_items.get(index)?;
		let mut current_folder: (PathBuf, u8) = (
			item.info().full_path().parent()?.to_path_buf(),
			item.info().indent(),
		);

		let item_count = self.tree_items.len();
		for item in self
			.tree_items
			.iter_mut()
			.rev()
			.skip(item_count - index - 1)
		{
			if item.info().indent() == current_folder.1 {
				item.show();
				shown += 1;
			} else if item.info().indent() == current_folder.1 - 1 {
				// this must be our parent

				item.expand_path();

				if item.info().is_visible() {
					// early out if parent already visible
					break;
				}

				item.show();
				shown += 1;

				current_folder = (
					item.info().full_path().parent()?.to_path_buf(),
					item.info().indent(),
				);
			}
		}

		Some(shown)
	}

	fn show_element_downward(
		&mut self,
		index: usize,
	) -> Option<usize> {
		let mut shown = 0_usize;

		let item = self.tree_items.get(index)?;
		let mut current_folder: (PathBuf, u8) = (
			item.info().full_path().parent()?.to_path_buf(),
			item.info().indent(),
		);

		for item in self.tree_items.iter_mut().skip(index + 1) {
			if item.info().indent() == current_folder.1 {
				item.show();
				shown += 1;
			}
			if item.info().indent() == current_folder.1 - 1 {
				// this must be our parent

				item.show();
				shown += 1;

				current_folder = (
					item.info().full_path().parent()?.to_path_buf(),
					item.info().indent(),
				);
			}
		}

		Some(shown)
	}

	fn update_visibility(
		&mut self,
		prefix: Option<&PathBuf>,
		start_idx: usize,
		set_defaults: bool,
	) {
		// if we are in any subpath that is collapsed we keep skipping over it
		let mut inner_collapsed: Option<PathBuf> = None;

		for i in start_idx..self.tree_items.len() {
			if let Some(ref collapsed_path) = inner_collapsed {
				let p = Path::new(
					self.tree_items[i].info().full_path_str(),
				);
				if p.starts_with(collapsed_path) {
					if set_defaults {
						self.tree_items[i]
							.info_mut()
							.set_visible(false);
					}
					// we are still in a collapsed inner path
					continue;
				}
				inner_collapsed = None;
			}

			let item_kind = self.tree_items[i].kind().clone();
			let item_path =
				Path::new(self.tree_items[i].info().full_path_str());

			if matches!(item_kind, FileTreeItemKind::Path(PathCollapsed(collapsed)) if collapsed)
			{
				// we encountered an inner path that is still collapsed
				inner_collapsed = Some(item_path.into());
			}

			if prefix
				.as_ref()
				.map_or(true, |prefix| item_path.starts_with(prefix))
			{
				self.tree_items[i].info_mut().set_visible(true);
			} else {
				// if we do not set defaults we can early out
				if set_defaults {
					self.tree_items[i].info_mut().set_visible(false);
				} else {
					return;
				}
			}
		}
	}

	fn fold_paths(
		items: &mut Vec<FileTreeItem>,
		paths: &HashMap<&Path, usize>,
	) {
		let mut i = 0;

		while i < items.len() {
			let item = &items[i];
			if item.kind().is_path() {
				let children = paths
					.get(&Path::new(item.info().full_path_str()));

				if let Some(children) = children {
					if *children == 1 {
						if i + 1 >= items.len() {
							return;
						}

						if items
							.get(i + 1)
							.is_some_and(|item| item.kind().is_path())
						{
							let next_item = items.remove(i + 1);
							let item_mut = &mut items[i];
							item_mut.fold(next_item);

							let prefix = item_mut
								.info()
								.full_path_str()
								.to_owned();

							Self::unindent(items, &prefix, i + 1);
							continue;
						}
					}
				}
			}

			i += 1;
		}
	}

	fn unindent(
		items: &mut [FileTreeItem],
		prefix: &str,
		start: usize,
	) {
		for elem in items.iter_mut().skip(start) {
			if elem.info().full_path_str().starts_with(prefix) {
				elem.info_mut().unindent();
			} else {
				return;
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use pretty_assertions::assert_eq;

	#[test]
	fn test_simple() {
		let items = vec![
			Path::new("file.txt"), //
		];

		let res =
			FileTreeItems::new(&items, &BTreeSet::new()).unwrap();

		assert!(res.tree_items[0].info().is_visible());
		assert_eq!(res.tree_items[0].info().indent(), 0);
		assert_eq!(res.tree_items[0].info().path(), items[0]);
		assert_eq!(res.tree_items[0].info().full_path(), items[0]);

		let items = vec![
			Path::new("file.txt"),  //
			Path::new("file2.txt"), //
		];

		let res =
			FileTreeItems::new(&items, &BTreeSet::new()).unwrap();

		assert_eq!(res.tree_items.len(), 2);
		assert_eq!(res.tree_items.len(), res.len());
		assert_eq!(res.tree_items[1].info().path(), items[1]);
	}

	#[test]
	fn test_push_path() {
		let mut items = Vec::new();
		let mut paths: HashMap<&Path, usize> = HashMap::new();

		FileTreeItems::push_dirs(
			Path::new("a/b/c"),
			&mut items,
			&mut paths,
			&BTreeSet::new(),
		)
		.unwrap();

		assert_eq!(*paths.get(&Path::new("a")).unwrap(), 1);

		FileTreeItems::push_dirs(
			Path::new("a/b2/c"),
			&mut items,
			&mut paths,
			&BTreeSet::new(),
		)
		.unwrap();

		assert_eq!(*paths.get(&Path::new("a")).unwrap(), 2);
	}

	#[test]
	fn test_push_path2() {
		let mut items = Vec::new();
		let mut paths: HashMap<&Path, usize> = HashMap::new();

		FileTreeItems::push_dirs(
			Path::new("a/b/c"),
			&mut items,
			&mut paths,
			&BTreeSet::new(),
		)
		.unwrap();

		assert_eq!(*paths.get(&Path::new("a")).unwrap(), 1);
		assert_eq!(*paths.get(&Path::new("a/b")).unwrap(), 1);

		FileTreeItems::push_dirs(
			Path::new("a/b/d"),
			&mut items,
			&mut paths,
			&BTreeSet::new(),
		)
		.unwrap();

		assert_eq!(*paths.get(&Path::new("a")).unwrap(), 1);
		assert_eq!(*paths.get(&Path::new("a/b")).unwrap(), 2);
	}

	#[test]
	fn test_folder() {
		let items = vec![
			Path::new("a/file.txt"), //
		];

		let res = FileTreeItems::new(&items, &BTreeSet::new())
			.unwrap()
			.tree_items
			.iter()
			.map(|i| i.info().full_path_str().to_string())
			.collect::<Vec<_>>();

		assert_eq!(
			res,
			vec![String::from("a"), String::from("a/file.txt"),]
		);
	}

	#[test]
	fn test_indent() {
		let items = vec![
			Path::new("a/b/file.txt"), //
		];

		let list =
			FileTreeItems::new(&items, &BTreeSet::new()).unwrap();
		let mut res = list
			.tree_items
			.iter()
			.map(|i| (i.info().indent(), i.info().path()));

		assert_eq!(res.next(), Some((0, Path::new("a/b"))));
		assert_eq!(res.next(), Some((1, Path::new("file.txt"))));
	}

	#[test]
	fn test_indent_folder_file_name() {
		let items = vec![
			Path::new("a/b"),   //
			Path::new("a.txt"), //
		];

		let list =
			FileTreeItems::new(&items, &BTreeSet::new()).unwrap();
		let mut res = list
			.tree_items
			.iter()
			.map(|i| (i.info().indent(), i.info().path_str()));

		assert_eq!(res.next(), Some((0, "a")));
		assert_eq!(res.next(), Some((1, "b")));
		assert_eq!(res.next(), Some((0, "a.txt")));
	}

	#[test]
	fn test_folder_dup() {
		let items = vec![
			Path::new("a/file.txt"),  //
			Path::new("a/file2.txt"), //
		];

		let tree =
			FileTreeItems::new(&items, &BTreeSet::new()).unwrap();

		assert_eq!(tree.file_count(), 2);
		assert_eq!(tree.len(), 3);

		let res = tree
			.tree_items
			.iter()
			.map(|i| i.info().full_path_str().to_string())
			.collect::<Vec<_>>();

		assert_eq!(
			res,
			vec![
				String::from("a"),
				String::from("a/file.txt"),
				String::from("a/file2.txt"),
			]
		);
	}

	#[test]
	fn test_collapse() {
		let items = vec![
			Path::new("a/file1.txt"), //
			Path::new("b/file2.txt"), //
		];

		let mut tree =
			FileTreeItems::new(&items, &BTreeSet::new()).unwrap();

		assert!(tree.tree_items[1].info().is_visible());

		tree.collapse(0, false);

		assert!(!tree.tree_items[1].info().is_visible());
	}

	#[test]
	fn test_iterate_collapsed() {
		let items = vec![
			Path::new("a/file1.txt"), //
			Path::new("b/file2.txt"), //
		];

		let mut tree =
			FileTreeItems::new(&items, &BTreeSet::new()).unwrap();

		tree.collapse(0, false);

		let mut it = tree.iterate(0, 10);

		assert_eq!(it.next().unwrap().0, 0);
		assert_eq!(it.next().unwrap().0, 2);
		assert_eq!(it.next().unwrap().0, 3);
		assert_eq!(it.next(), None);
	}

	pub fn get_visible(tree: &FileTreeItems) -> Vec<bool> {
		tree.tree_items
			.iter()
			.map(|e| e.info().is_visible())
			.collect::<Vec<_>>()
	}

	#[test]
	fn test_expand() {
		let items = vec![
			Path::new("a/b/c"), //
			Path::new("a/d"),   //
		];

		//0 a/
		//1   b/
		//2     c
		//3   d

		let mut tree =
			FileTreeItems::new(&items, &BTreeSet::new()).unwrap();

		tree.collapse(1, false);

		let visibles = get_visible(&tree);

		assert_eq!(
			visibles,
			vec![
				true,  //
				true,  //
				false, //
				true,
			]
		);

		tree.expand(1, false);

		let visibles = get_visible(&tree);

		assert_eq!(
			visibles,
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
		let items = vec![
			Path::new("a/b/c"),  //
			Path::new("a/b2/d"), //
		];

		//0 a/
		//1   b/
		//2     c
		//3   b2/
		//4     d

		let mut tree =
			FileTreeItems::new(&items, &BTreeSet::new()).unwrap();

		tree.collapse(1, false);
		tree.collapse(0, false);

		assert_eq!(
			get_visible(&tree),
			vec![
				true,  //
				false, //
				false, //
				false, //
				false,
			]
		);

		tree.expand(0, false);

		assert_eq!(
			get_visible(&tree),
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
		let items = vec![
			Path::new("a/b"),  //
			Path::new("a2/c"), //
		];

		//0 a/
		//1   b
		//2 a2/
		//3   c

		let mut tree =
			FileTreeItems::new(&items, &BTreeSet::new()).unwrap();

		tree.collapse(0, false);

		let visibles = get_visible(&tree);

		assert_eq!(
			visibles,
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
		let items = vec![
			Path::new("a/b/c"), //
			Path::new("a/d"),   //
		];

		//0 a/
		//1   b/
		//2     c
		//3   d

		let mut tree =
			FileTreeItems::new(&items, &BTreeSet::new()).unwrap();

		tree.collapse(1, false);

		let visibles = get_visible(&tree);

		assert_eq!(
			visibles,
			vec![
				true,  //
				true,  //
				false, //
				true,
			]
		);

		tree.collapse(0, false);

		let visibles = get_visible(&tree);

		assert_eq!(
			visibles,
			vec![
				true,  //
				false, //
				false, //
				false,
			]
		);

		tree.expand(0, false);

		let visible = get_visible(&tree);

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
	fn test_show_element() {
		let items = vec![
			Path::new("a/b/c"),  //
			Path::new("a/b2/d"), //
			Path::new("a/b2/e"), //
		];

		//0 a/
		//1   b/
		//2     c
		//3   b2/
		//4   	d
		//5     e

		let mut tree =
			FileTreeItems::new(&items, &BTreeSet::new()).unwrap();

		tree.collapse(0, true);

		let res = tree.show_element(5).unwrap();
		assert_eq!(res, 4);
		assert!(tree.tree_items[3].kind().is_path());
		assert!(!tree.tree_items[3].kind().is_path_collapsed());

		assert_eq!(
			get_visible(&tree),
			vec![
				true,  //
				true,  //
				false, //
				true,  //
				true,  //
				true,
			]
		);
	}

	#[test]
	fn test_show_element_later_elements() {
		let items = vec![
			Path::new("a/b"), //
			Path::new("a/c"), //
		];

		//0 a/
		//1   b
		//2   c

		let mut tree =
			FileTreeItems::new(&items, &BTreeSet::new()).unwrap();

		tree.collapse(0, true);

		assert_eq!(
			get_visible(&tree),
			vec![
				true,  //
				false, //
				false, //
			]
		);

		let res = tree.show_element(1).unwrap();
		assert_eq!(res, 2);

		assert_eq!(
			get_visible(&tree),
			vec![
				true, //
				true, //
				true, //
			]
		);
	}

	#[test]
	fn test_show_element_downward_parent() {
		let items = vec![
			Path::new("a/b/c"), //
			Path::new("a/d"),   //
			Path::new("a/e"),   //
		];

		//0 a/
		//1   b/
		//2     c
		//3   d
		//4   e

		let mut tree =
			FileTreeItems::new(&items, &BTreeSet::new()).unwrap();

		tree.collapse(0, true);

		let res = tree.show_element(2).unwrap();
		assert_eq!(res, 4);

		assert_eq!(
			get_visible(&tree),
			vec![
				true, //
				true, //
				true, //
				true, //
				true, //
			]
		);
	}

	#[test]
	fn test_show_element_expand_visible_parent() {
		let items = vec![
			Path::new("a/b"), //
		];

		//0 a/
		//1   b

		let mut tree =
			FileTreeItems::new(&items, &BTreeSet::new()).unwrap();

		tree.collapse(0, true);

		assert_eq!(
			get_visible(&tree),
			vec![
				true,  //
				false, //
			]
		);

		let res = tree.show_element(1).unwrap();
		assert_eq!(res, 1);
		assert!(tree.tree_items[0].kind().is_path());
		assert!(!tree.tree_items[0].kind().is_path_collapsed());

		assert_eq!(
			get_visible(&tree),
			vec![
				true, //
				true, //
			]
		);
	}
}

#[cfg(test)]
mod test_merging {
	use super::*;
	use pretty_assertions::assert_eq;

	#[test]
	fn test_merge_simple() {
		let list = vec![Path::new("a/b/c")];
		let (mut items, paths) =
			FileTreeItems::create_items(&list, &BTreeSet::new())
				.unwrap();

		assert_eq!(items.len(), 3);

		FileTreeItems::fold_paths(&mut items, &paths);

		assert_eq!(items.len(), 2);
	}

	#[test]
	fn test_merge_simple2() {
		let list = vec![
			Path::new("a/b/c"), //
			Path::new("a/b/d"), //
		];
		let (mut items, paths) =
			FileTreeItems::create_items(&list, &BTreeSet::new())
				.unwrap();

		assert_eq!(paths.len(), 2);
		assert_eq!(*paths.get(&Path::new("a")).unwrap(), 1);
		assert_eq!(*paths.get(&Path::new("a/b")).unwrap(), 2);
		assert_eq!(items.len(), 4);

		FileTreeItems::fold_paths(&mut items, &paths);

		assert_eq!(items.len(), 3);
	}

	#[test]
	fn test_merge_indent() {
		let list = vec![
			Path::new("a/b/c/d"), //
			Path::new("a/e/f"),   //
		];

		//0:0 a/
		//1:1   b/c
		//2:2     d
		//3:1   e/
		//4:2     f

		let (mut items, paths) =
			FileTreeItems::create_items(&list, &BTreeSet::new())
				.unwrap();

		assert_eq!(items.len(), 6);

		assert_eq!(paths.len(), 4);
		assert_eq!(*paths.get(&Path::new("a")).unwrap(), 2);
		assert_eq!(*paths.get(&Path::new("a/b")).unwrap(), 1);
		assert_eq!(*paths.get(&Path::new("a/b/c")).unwrap(), 1);
		assert_eq!(*paths.get(&Path::new("a/e")).unwrap(), 1);

		FileTreeItems::fold_paths(&mut items, &paths);

		let indents: Vec<u8> =
			items.iter().map(|i| i.info().indent()).collect();
		assert_eq!(indents, vec![0, 1, 2, 1, 2]);
	}

	#[test]
	fn test_merge_single_paths() {
		let items = vec![
			Path::new("a/b/c"), //
			Path::new("a/b/d"), //
		];

		//0 a/b/
		//1   c
		//2   d

		let tree =
			FileTreeItems::new(&items, &BTreeSet::new()).unwrap();

		let mut it = tree
			.iterate(0, 10)
			.map(|(_, item)| item.info().full_path_str());

		assert_eq!(it.next().unwrap(), "a/b");
		assert_eq!(it.next().unwrap(), "a/b/c");
		assert_eq!(it.next().unwrap(), "a/b/d");
		assert_eq!(it.next(), None);
	}

	#[test]
	fn test_merge_nothing() {
		let items = vec![
			Path::new("a/b/c"),  //
			Path::new("a/b2/d"), //
		];

		//0 a/
		//1   b/
		//2     c
		//3   b2/
		//4     d

		let tree =
			FileTreeItems::new(&items, &BTreeSet::new()).unwrap();

		let mut it = tree
			.iterate(0, 10)
			.map(|(_, item)| item.info().full_path_str());

		assert_eq!(it.next().unwrap(), "a");
		assert_eq!(it.next().unwrap(), "a/b");
		assert_eq!(it.next().unwrap(), "a/b/c");
		assert_eq!(it.next().unwrap(), "a/b2");
		assert_eq!(it.next().unwrap(), "a/b2/d");
		assert_eq!(it.next(), None);
	}
}
