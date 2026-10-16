use super::chunk::{lane_awaiting_parent, LaneSlot};
use super::{CommitAlias, UnwalkedAlias};
use std::collections::BTreeMap;

/// A single mutation of the lane state, recorded while processing one
/// commit.
///
/// Columns stay fixed while a lane is open. Only the right-hand end
/// may grow or shrink; an interior column can only be replaced in place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeltaOp {
	/// Put `slot` into the existing column `index`.
	Set {
		index: usize,
		slot: Option<LaneSlot>,
	},
	/// Append a new rightmost column, holding `slot`.
	Push { slot: LaneSlot },
	/// Drop the rightmost column, which is always vacant.
	Pop,
}

impl DeltaOp {
	fn apply(self, state: &mut Vec<Option<LaneSlot>>) {
		match self {
			Self::Set { index, slot } => state[index] = slot,
			Self::Push { slot } => state.push(Some(slot)),
			Self::Pop => {
				let removed = state.pop();
				debug_assert_eq!(removed, Some(None));
			}
		}
	}
}

/// All lane-state mutations caused by processing a single commit.
/// Applying a `Delta` to the lane state of row `n` yields the lane
/// state of row `n + 1`.
#[derive(Clone, Debug)]
pub struct Delta(pub Vec<DeltaOp>);

const CHECKPOINT_INTERVAL: usize = 100;

/// Delta-compressed history of the graph's lane state.
///
/// Stores the latest lane state and the deltas that produced it.
/// Periodic checkpoints bound the cost of reconstructing a window
/// with [`Self::decompress`].
#[derive(Default)]
pub struct Buffer {
	/// Lane state after the most recently processed commit.
	pub current: Vec<Option<LaneSlot>>,

	/// One [`Delta`] per processed commit, in the order of the walk.
	pub deltas: Vec<Delta>,

	/// Full lane-state snapshots taken every `CHECKPOINT_INTERVAL`
	/// commits, keyed by delta index, for reducing decompression cost.
	pub checkpoints: BTreeMap<usize, Vec<Option<LaneSlot>>>,

	/// The [`DeltaOp`]s recorded so far for the commit in flight.
	pending_delta: Vec<DeltaOp>,
}

impl Buffer {
	/// Place a commit and route its optional second parent, reusing
	/// an existing lane before reserving another column.
	pub fn update(
		&mut self,
		alias: CommitAlias,
		first_parent: Option<UnwalkedAlias>,
		second_parent: Option<UnwalkedAlias>,
	) {
		let slot = first_parent
			.map_or(LaneSlot::Settled { alias }, |parent| {
				LaneSlot::Flowing { alias, parent }
			});
		let placement = self.place_chunk(slot, alias);
		// Reserve before closing lanes: their closing corners still
		// occupy this row, so their columns cannot host a new lane yet.
		if let Some(parent) = second_parent {
			if lane_awaiting_parent(&self.current, *parent).is_none()
			{
				self.reserve_lane_right_of(placement, parent);
			}
		}
		self.consume_alias_in_other_chunks(alias, placement);
		self.commit_delta();
	}

	fn place_chunk(
		&mut self,
		new_chunk: LaneSlot,
		alias: CommitAlias,
	) -> usize {
		// Prefer a lane whose current occupant awaits this chunk's
		// commit as its primary parent, then the leftmost column the
		// row above leaves blank, and only then a new column. All
		// three land the chunk somewhere no open lane is drawn, so
		// nothing already on screen has to move. Settled root columns
		// remain occupied: stacking an unrelated commit directly below
		// a root would look like a parent edge.
		let target = lane_awaiting_parent(&self.current, alias)
			.or_else(|| self.first_empty_lane())
			.unwrap_or(self.current.len());

		if target < self.current.len() {
			self.record(DeltaOp::Set {
				index: target,
				slot: Some(new_chunk),
			});
		} else {
			self.record(DeltaOp::Push { slot: new_chunk });
		}
		target
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

			if self.current[index]
				.is_some_and(|slot| slot.awaits() == Some(alias))
			{
				self.record(DeltaOp::Set { index, slot: None });
			}
		}
	}

	/// Open the lane that a merge's second parent will arrive in,
	/// somewhere to the right of the merge itself.
	///
	/// The nearest column the row above leaves blank wins, which keeps
	/// the bridge short; failing that the lane goes on the right-hand
	/// end. This runs before closing incoming lanes, so a column being
	/// closed on this row cannot be reused for the merge's corner.
	///
	/// Columns to the left are left alone even when they are free.
	/// Reusing them would be just as sound, but it would put a merged
	/// branch on the far side of the history it was merged into, and
	/// `place_chunk` claims those columns for new branch tips anyway.
	fn reserve_lane_right_of(
		&mut self,
		merge_lane: usize,
		parent: UnwalkedAlias,
	) {
		let slot = LaneSlot::Reserved { parent };
		let reusable = (merge_lane + 1..self.current.len())
			.find(|&index| self.current[index].is_none());

		match reusable {
			Some(index) => self.record(DeltaOp::Set {
				index,
				slot: Some(slot),
			}),
			None => self.record(DeltaOp::Push { slot }),
		}
	}

	fn commit_delta(&mut self) {
		while matches!(self.current.last(), Some(None)) {
			self.record(DeltaOp::Pop);
		}

		self.deltas
			.push(Delta(std::mem::take(&mut self.pending_delta)));

		let step = self.deltas.len();
		if step.is_multiple_of(CHECKPOINT_INTERVAL) {
			self.checkpoints.insert(step - 1, self.current.clone());
		}
	}

	fn record(&mut self, op: DeltaOp) {
		op.apply(&mut self.current);
		self.pending_delta.push(op);
	}

	pub fn decompress(
		&self,
		start: usize,
		end: usize,
	) -> Vec<Vec<Option<LaneSlot>>> {
		let end = end.min(self.deltas.len().saturating_sub(1));
		if self.deltas.is_empty() || start > end {
			return Vec::new();
		}
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
				for &op in &delta.0 {
					op.apply(&mut state);
				}

				if delta_index >= start {
					history.push(state.clone());
				}
			} else {
				break;
			}
		}

		history
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn decompression_clips_requests_to_recorded_rows() {
		let mut buffer = Buffer::default();
		let empty: Vec<Vec<Option<LaneSlot>>> = Vec::new();
		assert_eq!(buffer.decompress(0, usize::MAX), empty);
		buffer.update(0.into(), None, None);
		assert_eq!(
			buffer.decompress(0, usize::MAX),
			vec![buffer.current.clone()]
		);
		assert_eq!(buffer.decompress(1, usize::MAX), empty);
		assert_eq!(buffer.decompress(1, 0), empty);
	}
}
