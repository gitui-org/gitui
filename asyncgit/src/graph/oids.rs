use crate::sync::CommitId;
use std::collections::HashMap;

/// mapping of `CommitId` to a numeric alias
pub struct GraphOids(HashMap<CommitId, usize>);

impl Default for GraphOids {
	fn default() -> Self {
		Self::new()
	}
}

impl GraphOids {
	///
	pub fn new() -> Self {
		Self(HashMap::new())
	}

	///
	pub fn get_or_insert(&mut self, id: &CommitId) -> usize {
		if let Some(&alias) = self.0.get(id) {
			return alias;
		}

		let alias = self.0.len();
		self.0.insert(*id, alias);
		alias
	}

	///
	pub fn get(&self, id: &CommitId) -> Option<usize> {
		self.0.get(id).copied()
	}
}
