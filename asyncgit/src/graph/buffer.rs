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

	pub fn update(&mut self, new_chunk: Chunk) {
		self.pending_delta.clear();

		let mut found_idx = None;
		if new_chunk.alias.is_some() {
			for (i, c) in self.current.iter().enumerate() {
				if let Some(c) = c {
					if c.parent_a == new_chunk.alias {
						found_idx = Some(i);
						break;
					}
				}
			}
		}

		if let Some(idx) = found_idx {
			self.record_replace(idx, Some(new_chunk.clone()));
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
				let mut changed = false;

				if new_chunk.alias.is_some()
					&& c.parent_a == new_chunk.alias
				{
					c.parent_a = None;
					changed = true;
				}
				if new_chunk.alias.is_some()
					&& c.parent_b == new_chunk.alias
				{
					c.parent_b = None;
					changed = true;
				}

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
					.map_or(false, |chunk| chunk.alias == Some(alias))
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
					self.record_insert(
						self.current.len(),
						Some(new_lane),
					);
				}
			}
		}

		loop {
			if let Some(last) = self.current.last() {
				if last.is_none() {
					self.record_remove(self.current.len() - 1);
					continue;
				}
			}
			break;
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
		let (current_index, mut state) = self
			.checkpoints
			.range(..=start)
			.next_back()
			.map(|(&i, s)| (Some(i), s.clone()))
			.unwrap_or((None, Vector::new()));

		let mut history =
			Vec::with_capacity(end.saturating_sub(start) + 1);

		if let Some(index) = current_index {
			if index >= start && index <= end {
				history.push(state.clone());
			}
		}

		let loop_start = current_index.map(|i| i + 1).unwrap_or(0);

		for delta_index in loop_start..=end {
			if let Some(delta) = self.deltas.get(delta_index) {
				self.apply_delta_to_state(&mut state, delta);

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
		&self,
		state: &mut Vector<Option<Chunk>>,
		delta: &Delta,
	) {
		for op in &delta.0 {
			match op {
				DeltaOp::Insert { index, item } => {
					state.insert(*index, item.clone())
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
