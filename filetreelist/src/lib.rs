// #![forbid(missing_docs)]
#![forbid(unsafe_code)]
#![deny(
	mismatched_lifetime_syntaxes,
	unused_imports,
	unused_must_use,
	dead_code,
	unstable_name_collisions,
	unused_assignments
)]
#![deny(clippy::all, clippy::perf, clippy::nursery, clippy::pedantic)]
#![deny(clippy::expect_used)]
#![deny(clippy::filetype_is_file)]
#![deny(clippy::cargo)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::panic)]
#![deny(clippy::match_like_matches_macro)]
#![deny(clippy::needless_update)]
#![allow(
	clippy::module_name_repetitions,
	clippy::must_use_candidate,
	clippy::missing_errors_doc,
	clippy::empty_docs
)]

mod error;
mod filetree;
mod filetreeitems;
mod item;
mod tree_iter;
mod treeitems_iter;

pub use crate::{
	filetree::FileTree,
	filetree::MoveSelection,
	item::{FileTreeItem, TreeItemInfo},
};
