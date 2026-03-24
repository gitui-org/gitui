use super::buffer::Buffer;
use super::chunk::{Chunk, Markers};
use super::oids::Oids;
use super::{ConnType, GraphRow};
use crate::sync::{CommitId, CommitInfo};
use im::Vector;
use std::collections::{HashMap, HashSet};

pub struct GraphWalker {
	pub buffer: Buffer,
	pub oids: Oids,
	pub branch_lane_map: HashMap<CommitId, usize>,
	pub mergers_map: HashMap<u32, u32>,
}

impl Default for GraphWalker {
	fn default() -> Self {
		Self::new()
	}
}

impl GraphWalker {
	pub fn new() -> Self {
		Self {
			buffer: Buffer::new(),
			oids: Oids::new(),
			branch_lane_map: HashMap::new(),
			mergers_map: HashMap::new(),
		}
	}

