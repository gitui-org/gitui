#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Markers {
	Uncommitted,
	Commit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chunk {
	pub alias: Option<usize>,
	pub parent_a: Option<usize>,
	pub parent_b: Option<usize>,
	pub marker: Markers,
}
