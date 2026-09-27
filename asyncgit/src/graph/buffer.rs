use super::chunk::LaneSlot;
use super::CommitAlias;
use std::collections::BTreeMap;

/// A single mutation of the lane state, recorded while processing one
/// commit.
#[derive(Clone, Debug)]
pub enum DeltaOp {
	Insert {
		index: usize,
		item: Option<LaneSlot>,
	},
	Remove {
		index: usize,
	},
	Replace {
		index: usize,
		new: Option<LaneSlot>,
	},
}

/// All lane-state mutations caused by processing a single commit.
/// Applying a `Delta` to the lane state of row `n` yields the lane
/// state of row `n + 1`.
#[derive(Clone, Debug)]
pub struct Delta(pub Vec<DeltaOp>);

const CHECKPOINT_INTERVAL: usize = 100;

/// Delta-compressed history of the graph's lane state.
///
/// While walking the log top-down, every commit mutates the set of
/// active lanes (a `Vec<Option<LaneSlot>>`, one slot per lane). So, storign a
/// full copy of that state for each commit is a waste. This
/// buffer preserves ONLY the latest state PLUS the list of [`Delta`]s that
/// produced it.
/// Use [`Buffer::decompress`] to get the complete version.
#[derive(Default)]
pub struct Buffer {
	/// Lane state after the most recently processed commit.
	pub current: Vec<Option<LaneSlot>>,

	/// One [`Delta`] per processed commit, in the order of the walk.
	pub deltas: Vec<Delta>,

	/// Full lane-state snapshots taken every `CHECKPOINT_INTERVAL`
	/// commits, keyed by delta index, for reducing decompression cost.
	pub checkpoints: BTreeMap<usize, Vec<Option<LaneSlot>>>,

	/// Aliases of merge commits whose second parent still needs a new
	/// lane.
	pub merge_commits: Vec<CommitAlias>,

	/// Scratch list of the [`DeltaOp`]s recorded for processing commit
	pub pending_delta: Vec<DeltaOp>,
}

impl Buffer {
	/// Remember `alias` as a merge commit whose second parent must be
	/// given its own lane.
	pub fn track_merge_commit(&mut self, alias: CommitAlias) {
		self.merge_commits.push(alias);
	}

	pub fn update(&mut self, new_chunk: &LaneSlot) {
		let placement_index = self.place_chunk(new_chunk);

		if let Some(alias) = new_chunk.alias() {
			self.consume_alias_in_other_chunks(
				alias,
				placement_index,
			);
		}

		self.flush_merge_commits();
		self.commit_delta();
	}

	fn place_chunk(&mut self, new_chunk: &LaneSlot) -> usize {
		// Prefer a lane whose current occupant awaits this chunk's
		// commit as its primary parent.
		let target = self
			.find_lane_awaiting_parent(new_chunk.alias())
			.or_else(|| self.first_empty_lane())
			.unwrap_or(self.current.len());

		if target < self.current.len() {
			self.record_replace(target, Some(*new_chunk));
		} else {
			self.record_insert(target, Some(*new_chunk));
		}
		target
	}

	fn find_lane_awaiting_parent(
		&self,
		alias: Option<CommitAlias>,
	) -> Option<usize> {
		alias.and_then(|alias| {
			self.current.iter().position(|slot| {
				slot.as_ref().is_some_and(|chunk| {
					chunk.awaits() == Some(alias)
				})
			})
		})
	}

	fn first_empty_lane(&self) -> Option<usize> {
		self.current.iter().position(Option::is_none)
	}

	fn consume_alias_in_other_chunks(
		&mut self,
		alias: CommitAlias,
		skip_index: usize,
	) {
		for index in 0..self.current.len() {
			if index == skip_index {
				continue;
			}

			// `LaneSlot` is `Copy`; matching by value lets us read
			// the slot out before `record_replace` mutates `current`.
			let new_chunk = match self.current[index] {
				Some(
					LaneSlot::Flowing { parent, .. }
					| LaneSlot::FlowingMerge { parent, .. }
					| LaneSlot::Reserved { parent },
				) if *parent == alias => {
					// The awaited parent was JUST placed. Close the
					// lane. The pending second parent is dropped
					// with it.
					None
				}
				Some(LaneSlot::FlowingMerge {
					alias: merge_alias,
					parent,
					second,
				}) if *second == alias => {
					// The second parent was just placed
					// bridge resolves and the lane flows
					Some(LaneSlot::Flowing {
						alias: merge_alias,
						parent,
					})
				}
				_ => continue,
			};

			self.record_replace(index, new_chunk);
		}
	}

	fn flush_merge_commits(&mut self) {
		while let Some(alias) = self.merge_commits.pop() {
			// Search for an occupied slot that matches the target alias.
			// If found, extract its index and a clone of the chunk.
			let Some((index, chunk)) =
				self.current.iter().enumerate().find_map(
					|(index, slot)| {
						let chunk = *slot.as_ref()?;
						(chunk.alias() == Some(alias))
							.then_some((index, chunk))
					},
				)
			else {
				continue;
			};

			// Only a merge still owing its second parent needs a
			// lane split off; anything else is a no-op.
			let LaneSlot::FlowingMerge {
				alias: merge_alias,
				parent,
				second,
			} = chunk
			else {
				continue;
			};

			self.record_replace(
				index,
				Some(LaneSlot::Flowing {
					alias: merge_alias,
					parent,
				}),
			);

			// Always append the merge's second-parent lane to
			// the end instead of reusing an existing empty slot,
			// so the new visual column does not collapse
			// spatial ordering of lanes already in existence.
			self.record_insert(
				self.current.len(),
				Some(LaneSlot::Reserved { parent: second }),
			);
		}
	}

	fn commit_delta(&mut self) {
		while matches!(self.current.last(), Some(None)) {
			let last = self.current.len() - 1;
			self.record_remove(last);
		}

		self.deltas
			.push(Delta(std::mem::take(&mut self.pending_delta)));

		let step = self.deltas.len();
		if step.is_multiple_of(CHECKPOINT_INTERVAL) {
			self.checkpoints.insert(step - 1, self.current.clone());
		}
	}

	fn record_replace(
		&mut self,
		index: usize,
		new: Option<LaneSlot>,
	) {
		self.pending_delta.push(DeltaOp::Replace { index, new });
		self.current[index] = new;
	}

	fn record_insert(
		&mut self,
		index: usize,
		item: Option<LaneSlot>,
	) {
		self.pending_delta.push(DeltaOp::Insert { index, item });
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
	) -> Vec<Vec<Option<LaneSlot>>> {
		let (current_index, mut state) =
			self.checkpoints.range(..=start).next_back().map_or_else(
				|| (None, Vec::new()),
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
		state: &mut Vec<Option<LaneSlot>>,
		delta: &Delta,
	) {
		for op in &delta.0 {
			match op {
				DeltaOp::Insert { index, item } => {
					state.insert(*index, *item);
				}
				DeltaOp::Remove { index } => {
					state.remove(*index);
				}
				DeltaOp::Replace { index, new } => {
					state[*index].clone_from(new);
				}
			}
		}
	}
}
