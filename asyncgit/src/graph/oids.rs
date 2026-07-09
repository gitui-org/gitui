use super::CommitAlias;
use crate::sync::CommitId;
use std::collections::HashMap;

/// mapping of `CommitId` to a numeric alias
#[derive(Default)]
pub struct GraphOids(HashMap<CommitId, CommitAlias>);

impl GraphOids {
	/// Create an empty alias map.
	pub fn new() -> Self {
		Self::default()
	}

	/// Get the alias for `id`, assigning a new one if it doesn't exist yet.
	pub fn get_or_insert(&mut self, id: &CommitId) -> CommitAlias {
		if let Some(&alias) = self.0.get(id) {
			return alias;
		}

		let alias = CommitAlias::from(self.0.len());
		self.0.insert(*id, alias);
		alias
	}

	/// Look up the alias for `id`, returning `None` if not found.
	pub fn get(&self, id: &CommitId) -> Option<CommitAlias> {
		self.0.get(id).copied()
	}
}
