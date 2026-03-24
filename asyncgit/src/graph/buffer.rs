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

impl Buffer {
	pub fn new() -> Self {
		Self {
			current: Vector::new(),
			deltas: Vec::new(),
			checkpoints: BTreeMap::new(),
			mergers: Vec::new(),
			pending_delta: Vec::new(),
		}
	}

	pub fn merger(&mut self, alias: u32) {
		self.mergers.push(alias);
	}

	pub fn update(&mut self, new_chunk: &Chunk) {
		self.pending_delta.clear();

		let mut empty_lanes: Vec<usize> = self
			.current
			.iter()
			.enumerate()
			.filter_map(|(i, c)| c.is_none().then_some(i))
			.collect();

		// sort descending so we can pop the lowest index first
		empty_lanes.sort_unstable_by(|a, b| b.cmp(a));

		let found_idx = if new_chunk.alias.is_some() {
			self.current.iter().enumerate().find_map(|(i, c)| {
				c.as_ref().and_then(|c| {
					(c.parent_a == new_chunk.alias).then_some(i)
				})
			})
		} else {
			None
		};

		if let Some(idx) = found_idx {
			self.record_replace(idx, Some(new_chunk.clone()));
		} else if let Some(empty_idx) = empty_lanes.pop() {
			self.record_replace(empty_idx, Some(new_chunk.clone()));
		} else {
			self.record_insert(
				self.current.len(),
				Some(new_chunk.clone()),
			);
		}

		let current_length = self.current.len();
		for index in 0..current_length {
			if Some(index) == found_idx {
				continue;
			}
			if found_idx.is_none() && index == current_length - 1 {
				continue;
			}

			if let Some(mut c) = self.current[index].clone() {
				let changed = new_chunk.alias.is_some_and(|alias| {
					let a = c.parent_a == Some(alias);
					let b = c.parent_b == Some(alias);
					if a {
						c.parent_a = None;
					}
					if b {
						c.parent_b = None;
					}
					a || b
				});

				if changed {
					if c.parent_a.is_none() && c.parent_b.is_none() {
						self.record_replace(index, None);
					} else {
						self.record_replace(index, Some(c));
					}
				}
			}
		}

		while let Some(alias) = self.mergers.pop() {
			if let Some(index) = self.current.iter().position(|c| {
				c.as_ref()
					.is_some_and(|chunk| chunk.alias == Some(alias))
			}) {
				if let Some(mut c) = self.current[index].clone() {
					let parent_b = c.parent_b;
					c.parent_b = None;
					self.record_replace(index, Some(c));

					let new_lane = Chunk {
						alias: None,
						parent_a: parent_b,
						parent_b: None,
						marker: Markers::Commit,
					};

					if let Some(empty_idx) = empty_lanes.pop() {
						self.record_replace(
							empty_idx,
							Some(new_lane),
						);
					} else {
						self.record_insert(
							self.current.len(),
							Some(new_lane),
						);
					}
				}
			}
		}

		while self.current.last().is_some_and(Option::is_none) {
			self.record_remove(self.current.len() - 1);
		}

		let delta = Delta(self.pending_delta.clone());
		self.deltas.push(delta);

		let current_step = self.deltas.len();
		if current_step > 0 && current_step % CHECKPOINT_INTERVAL == 0
		{
			self.checkpoints
				.insert(current_step - 1, self.current.clone());
		}
	}

	fn record_replace(&mut self, index: usize, new: Option<Chunk>) {
		self.pending_delta.push(DeltaOp::Replace {
			index,
			new: new.clone(),
		});
		self.current.set(index, new);
	}

	fn record_insert(&mut self, index: usize, item: Option<Chunk>) {
		self.pending_delta.push(DeltaOp::Insert {
			index,
			item: item.clone(),
		});
		self.current.insert(index, item);
	}

	fn record_remove(&mut self, index: usize) {
		self.pending_delta.push(DeltaOp::Remove { index });
		self.current.remove(index);
	}

	pub fn decompress(
		&self,
		start: usize,
		end: usize,
	) -> Vec<Vector<Option<Chunk>>> {
		let (current_index, mut state) =
			self.checkpoints.range(..=start).next_back().map_or_else(
				|| (None, Vector::new()),
				|(&i, s)| (Some(i), s.clone()),
			);

		let mut history =
			Vec::with_capacity(end.saturating_sub(start) + 1);

		if let Some(index) = current_index {
			if index >= start && index <= end {
				history.push(state.clone());
			}
		}

		let loop_start = current_index.map_or(0, |i| i + 1);

		for delta_index in loop_start..=end {
			if let Some(delta) = self.deltas.get(delta_index) {
				Self::apply_delta_to_state(&mut state, delta);

				if delta_index >= start {
					history.push(state.clone());
				}
			} else {
				break;
			}
		}

		history
	}

	fn apply_delta_to_state(
		state: &mut Vector<Option<Chunk>>,
		delta: &Delta,
	) {
		for op in &delta.0 {
			match op {
				DeltaOp::Insert { index, item } => {
					state.insert(*index, item.clone());
				}
				DeltaOp::Remove { index } => {
					state.remove(*index);
				}
				DeltaOp::Replace { index, new } => {
					state.set(*index, new.clone());
				}
			}
		}
	}
}
