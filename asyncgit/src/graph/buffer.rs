use super::chunk::{Chunk, Markers};
use im::Vector;
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub enum DeltaOp {
	Insert { index: usize, item: Option<Chunk> },
	Remove { index: usize },
	Replace { index: usize, new: Option<Chunk> },
}

#[derive(Clone, Debug)]
pub struct Delta(pub Vec<DeltaOp>);

const CHECKPOINT_INTERVAL: usize = 100;

pub struct Buffer {
	pub current: Vector<Option<Chunk>>,
	pub deltas: Vec<Delta>,
	pub checkpoints: BTreeMap<usize, Vector<Option<Chunk>>>,
	mergers: Vec<u32>,
	pending_delta: Vec<DeltaOp>,
}

impl Default for Buffer {
	fn default() -> Self {
		Self::new()
	}
}

