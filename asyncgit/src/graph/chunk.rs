use super::AliasId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Markers {
	Uncommitted,
	Commit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chunk {
	pub alias: Option<AliasId>,
	pub parent_a: Option<AliasId>,
	pub parent_b: Option<AliasId>,
	pub marker: Markers,
}
