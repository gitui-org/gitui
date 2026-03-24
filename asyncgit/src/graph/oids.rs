use crate::sync::CommitId;
use std::collections::HashMap;

pub struct Oids {
	/// alias
	pub ids: Vec<CommitId>,

	/// CommitId to alias
	pub aliases: HashMap<CommitId, u32>,
}

impl Default for Oids {
	fn default() -> Self {
		Self::new()
	}
}

impl Oids {
	pub fn new() -> Self {
		Self {
			ids: Vec::new(),
			aliases: HashMap::new(),
		}
	}

	pub fn get_or_insert(&mut self, id: &CommitId) -> u32 {
		if let Some(&alias) = self.aliases.get(id) {
			return alias;
		}
		let alias = self.ids.len() as u32;
		self.ids.push(*id);
		self.aliases.insert(*id, alias);
		alias
	}

	pub fn get(&self, id: &CommitId) -> Option<u32> {
		self.aliases.get(id).copied()
	}
}
