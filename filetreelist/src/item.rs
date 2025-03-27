use crate::error::Result;
use std::path::{Path, PathBuf};

/// holds the information shared among all `FileTreeItem` in a `FileTree`
#[derive(Debug, Clone)]
pub struct TreeItemInfo {
	/// indent level
	indent: u8,
	/// currently visible depending on the folder collapse states
	visible: bool,
	/// contains this paths last component and folded up paths added to it
	/// if this is `None` nothing was folding into here
	folded: Option<PathBuf>,
	/// the full path
	full_path: PathBuf,
}

impl TreeItemInfo {
	///
	pub const fn new(indent: u8, full_path: PathBuf) -> Self {
		Self {
			indent,
			visible: true,
			folded: None,
			full_path,
		}
	}

	///
	pub const fn is_visible(&self) -> bool {
		self.visible
	}

	///
	//TODO: remove
	pub fn full_path_str(&self) -> &str {
		self.full_path.to_str().unwrap_or_default()
	}

	///
	pub fn full_path(&self) -> &Path {
		self.full_path.as_path()
	}

	/// like `path` but as `&str`
	pub fn path_str(&self) -> &str {
		self.path().as_os_str().to_str().unwrap_or_default()
	}

	/// returns the last component of `full_path`
	/// or the last components plus folded up children paths
	pub fn path(&self) -> &Path {
		self.folded.as_ref().map_or_else(
			|| {
				Path::new(
					self.full_path
						.components()
						.next_back()
						.and_then(|c| c.as_os_str().to_str())
						.unwrap_or_default(),
				)
			},
			PathBuf::as_path,
		)
	}

	///
	pub const fn indent(&self) -> u8 {
		self.indent
	}

	///
	pub fn unindent(&mut self) {
		self.indent = self.indent.saturating_sub(1);
	}

	pub fn set_visible(&mut self, visible: bool) {
		self.visible = visible;
	}
}

/// attribute used to indicate the collapse/expand state of a path item
#[derive(PartialEq, Eq, Debug, Copy, Clone)]
pub struct PathCollapsed(pub bool);

/// `FileTreeItem` can be of two kinds
#[derive(PartialEq, Eq, Debug, Clone)]
pub enum FileTreeItemKind {
	Path(PathCollapsed),
	File,
}

impl FileTreeItemKind {
	pub const fn is_path(&self) -> bool {
		matches!(self, Self::Path(_))
	}

	pub const fn is_path_collapsed(&self) -> bool {
		match self {
			Self::Path(collapsed) => collapsed.0,
			Self::File => false,
		}
	}
}

/// `FileTreeItem` can be of two kinds: see `FileTreeItem` but shares an info
#[derive(Debug, Clone)]
pub struct FileTreeItem {
	info: TreeItemInfo,
	kind: FileTreeItemKind,
}

impl FileTreeItem {
	pub fn new_file(path: &Path) -> Result<Self> {
		let item_path = PathBuf::from(path);

		let indent = u8::try_from(
			item_path.ancestors().count().saturating_sub(2),
		)?;

		Ok(Self {
			info: TreeItemInfo::new(indent, item_path),
			kind: FileTreeItemKind::File,
		})
	}

	pub fn new_path(path: &Path, collapsed: bool) -> Result<Self> {
		let indent =
			u8::try_from(path.ancestors().count().saturating_sub(2))?;

		Ok(Self {
			info: TreeItemInfo::new(indent, path.to_owned()),
			kind: FileTreeItemKind::Path(PathCollapsed(collapsed)),
		})
	}

	///
	pub fn fold(&mut self, next: Self) {
		if let Some(folded) = self.info.folded.as_mut() {
			*folded = folded.join(next.info.path());
		} else {
			self.info.folded =
				Some(self.info.path().join(next.info.path()));
		}

		self.info.full_path = next.info.full_path;
	}

	///
	pub const fn info(&self) -> &TreeItemInfo {
		&self.info
	}

	///
	pub fn info_mut(&mut self) -> &mut TreeItemInfo {
		&mut self.info
	}

	///
	pub const fn kind(&self) -> &FileTreeItemKind {
		&self.kind
	}

	/// # Panics
	/// panics if self is not a path
	pub fn collapse_path(&mut self) {
		assert!(self.kind.is_path());
		self.kind = FileTreeItemKind::Path(PathCollapsed(true));
	}

	/// # Panics
	/// panics if self is not a path
	pub fn expand_path(&mut self) {
		assert!(self.kind.is_path());
		self.kind = FileTreeItemKind::Path(PathCollapsed(false));
	}

	///
	pub fn hide(&mut self) {
		self.info.visible = false;
	}

	///
	pub fn show(&mut self) {
		self.info.visible = true;
	}
}

impl Eq for FileTreeItem {}

impl PartialEq for FileTreeItem {
	fn eq(&self, other: &Self) -> bool {
		self.info.full_path.eq(&other.info.full_path)
	}
}

impl PartialOrd for FileTreeItem {
	fn partial_cmp(
		&self,
		other: &Self,
	) -> Option<std::cmp::Ordering> {
		Some(self.cmp(other))
	}
}

impl Ord for FileTreeItem {
	fn cmp(&self, other: &Self) -> std::cmp::Ordering {
		self.info.path().cmp(other.info.path())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use pretty_assertions::assert_eq;

	#[test]
	fn test_smoke() {
		let mut a =
			FileTreeItem::new_path(Path::new("a"), false).unwrap();

		assert_eq!(a.info.full_path_str(), "a");
		assert_eq!(a.info.path_str(), "a");

		let b =
			FileTreeItem::new_path(Path::new("a/b"), false).unwrap();
		a.fold(b);

		assert_eq!(a.info.full_path_str(), "a/b");
		assert_eq!(
			&a.info.folded.as_ref().unwrap(),
			&Path::new("a/b")
		);
		assert_eq!(a.info.path(), Path::new("a/b"));
	}
}
