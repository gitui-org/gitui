use super::buffer::Buffer;
use super::cell::{Cell, Directions};
use super::chunk::{lane_awaiting_parent, LaneSlot};
use super::oids::GraphOids;
use super::{
	CommitAlias, ConnectionType, GraphRow, LaneIndex, UnwalkedAlias,
	MAX_LANE_COLORS,
};
use crate::sync::CommitId;
use core::cmp::Ordering;
use std::collections::HashSet;

/// Get the lane's color index, which cycles through the color palette.
fn lane_color(lane: usize) -> LaneIndex {
	LaneIndex::from(lane % MAX_LANE_COLORS)
}

/// What the walk learned about one commit, indexed by its
/// [`CommitAlias`]. Aliases are dense `0..n`, so the facts live in a
/// flat vector instead of a map.
#[derive(Clone, Copy, Debug, Default)]
struct CommitFacts {
	/// Already folded into the buffer; cannot be awaited by a new lane.
	processed: bool,

	/// Derived from Git's parent count, independently of drawable edges.
	is_merge: bool,

	/// The drawable second parent at the far end of the merge bridge.
	/// It may share a lane that already leads to the same parent.
	second_parent: Option<CommitAlias>,
}

#[derive(Default)]
pub struct GraphWalker {
	pub buffer: Buffer,
	pub oids: GraphOids,

	/// What the walk knows about each commit, indexed by alias.
	facts: Vec<CommitFacts>,
}

impl GraphWalker {
	pub fn new() -> Self {
		Self::default()
	}

	fn facts(&self, alias: CommitAlias) -> CommitFacts {
		self.facts.get(*alias).copied().unwrap_or_default()
	}

	fn facts_mut(&mut self, alias: CommitAlias) -> &mut CommitFacts {
		let index = *alias;
		if self.facts.len() <= index {
			self.facts.resize(index + 1, CommitFacts::default());
		}
		&mut self.facts[index]
	}

	/// Mint the drawable alias for a parent commit, or `None` if the
	/// walk already passed it.
	fn mint_drawable_parent(
		&mut self,
		id: &CommitId,
	) -> Option<UnwalkedAlias> {
		let alias = self.oids.get_or_insert(id);
		(!self.facts(alias).processed).then_some(UnwalkedAlias(alias))
	}

	/// The parents this commit draws an edge to: the first two
	/// distinct parents the walk has not already passed.
	///
	/// A row has at most one merge bridge. Duplicated, self-naming,
	/// and already-walked parents must not leave uncloseable lanes.
	fn drawable_parents(
		&mut self,
		parents: &[CommitId],
	) -> [Option<UnwalkedAlias>; 2] {
		let mut drawn: [Option<UnwalkedAlias>; 2] = [None, None];
		let mut found = 0;

		for parent_id in parents {
			if found == drawn.len() {
				break;
			}

			let Some(parent) = self.mint_drawable_parent(parent_id)
			else {
				continue;
			};

			let repeated = drawn[..found]
				.iter()
				.flatten()
				.any(|seen| **seen == *parent);
			if repeated {
				continue;
			}

			drawn[found] = Some(parent);
			found += 1;
		}

		drawn
	}

	pub fn process(
		&mut self,
		commit_id: CommitId,
		parents: &[CommitId],
	) {
		let commit_alias = self.oids.get_or_insert(&commit_id);

		// Mark the commit walked before resolving its parents, so that
		// a parent entry naming the commit itself is turned down like
		// any other already-walked parent instead of opening a lane
		// that waits for the commit which just closed it.
		self.facts_mut(commit_alias).processed = true;

		let [first_parent, second_parent] =
			self.drawable_parents(parents);

		let second = second_parent.map(|second| *second);
		let facts = self.facts_mut(commit_alias);
		facts.is_merge = parents.len() > 1;
		facts.second_parent = second;

		self.buffer
			.update(commit_alias, first_parent, second_parent);
	}

	/// Number of commits already folded into the graph buffer.
	pub const fn processed_commit_count(&self) -> usize {
		self.buffer.deltas.len()
	}

	pub fn compute_rows(
		&self,
		commit_range: &[CommitId],
		global_start_index: usize,
		branch_tips: &HashSet<CommitId>,
		stashes: &HashSet<CommitId>,
		head_id: Option<&CommitId>,
	) -> Vec<GraphRow> {
		if commit_range.is_empty() {
			return Vec::new();
		}

		// Include a predecessor for bridge routing. At the beginning
		// of the walk there is no predecessor, so the offset is zero.
		let snapshot_start_index =
			global_start_index.saturating_sub(1);
		let snapshot_end_index =
			global_start_index + commit_range.len() - 1;
		let snapshots = self
			.buffer
			.decompress(snapshot_start_index, snapshot_end_index);
		let index_offset = global_start_index - snapshot_start_index;

		commit_range
			.iter()
			.enumerate()
			.map(|(range_index, commit_id)| {
				let snapshot_index = range_index + index_offset;

				let current_snapshot = snapshots
					.get(snapshot_index)
					.map(Vec::as_slice)
					.unwrap_or_default();

				let previous_snapshot = snapshot_index
					.checked_sub(1)
					.and_then(|index| snapshots.get(index))
					.map(Vec::as_slice);

				self.render_row(
					commit_id,
					current_snapshot,
					previous_snapshot,
					branch_tips,
					stashes,
					head_id,
				)
			})
			.collect()
	}

	fn draw_merge_bridge(
		lanes: &mut [Cell],
		merge_bridge: Option<(usize, usize)>,
		commit_lane: usize,
		current_snapshot: &[Option<LaneSlot>],
		previous_snapshot: Option<&[Option<LaneSlot>]>,
	) {
		let Some((source_lane, target_lane)) = merge_bridge else {
			return;
		};
		// `drawable_parents` keeps the two parents distinct, so the
		// search in `calculate_merge_bridge` cannot land back on the
		// merge's own lane. The guard stays because the alternative
		// is overwriting the commit marker with a zero-length span.
		if source_lane == target_lane {
			return;
		}

		let destination_lane = if source_lane == commit_lane {
			target_lane
		} else {
			source_lane
		};
		let connection_color = lane_color(destination_lane);

		let continues_upwards = Self::lane_continues_upwards(
			destination_lane,
			current_snapshot,
			previous_snapshot,
		);

		let target_directions = Self::calculate_merge_directions(
			commit_lane,
			destination_lane,
			continues_upwards,
		);

		// Replace the plain vertical fill with the precise corner/junction
		lanes[destination_lane] =
			Cell::line(target_directions, connection_color, false);

		Self::draw_bridge_span(
			lanes,
			source_lane,
			target_lane,
			connection_color,
		);
	}

	fn draw_branching_lanes(
		lanes: &mut Vec<Cell>,
		branching_lanes: &[usize],
		commit_lane: usize,
	) -> Vec<(LaneIndex, LaneIndex)> {
		branching_lanes
			.iter()
			.map(|&branch_lane| {
				let start_lane =
					std::cmp::min(branch_lane, commit_lane);
				let end_lane =
					std::cmp::max(branch_lane, commit_lane);

				Self::ensure_lane_capacity(lanes, end_lane);

				let connection_color = lane_color(branch_lane);
				Self::draw_bridge_span(
					lanes,
					start_lane,
					end_lane,
					connection_color,
				);

				let branch_directions =
					Self::calculate_branch_directions(
						branch_lane,
						start_lane,
						end_lane,
					);
				lanes[branch_lane]
					.overlay(branch_directions, connection_color);

				(
					LaneIndex::from(start_lane),
					LaneIndex::from(end_lane),
				)
			})
			.collect()
	}

	/// Whether the lane in `target_lane` is the same lane the row above
	/// had there, which is what lets a bridge end draw a junction into
	/// it instead of a corner.
	fn lane_continues_upwards(
		target_lane: usize,
		current_snapshot: &[Option<LaneSlot>],
		previous_snapshot: Option<&[Option<LaneSlot>]>,
	) -> bool {
		let Some(previous_snapshot) = previous_snapshot else {
			return false;
		};

		let occupant = |snapshot: &[Option<LaneSlot>]| {
			snapshot.get(target_lane).copied().flatten()
		};

		let current = occupant(current_snapshot);
		current.is_some() && occupant(previous_snapshot) == current
	}

	/// Directions at the receiving end of a merge bridge.
	fn calculate_merge_directions(
		commit_lane: usize,
		target_lane: usize,
		continues_upwards: bool,
	) -> Directions {
		let mut directions = Directions::DOWN;

		if continues_upwards {
			directions |= Directions::UP;
		}

		match target_lane.cmp(&commit_lane) {
			Ordering::Greater => directions |= Directions::LEFT,
			Ordering::Less => directions |= Directions::RIGHT,
			Ordering::Equal => {}
		}

		directions
	}

	fn calculate_branch_directions(
		branch_lane: usize,
		start_lane: usize,
		end_lane: usize,
	) -> Directions {
		let mut directions = Directions::UP;

		if branch_lane == end_lane {
			directions |= Directions::LEFT;
		}
		if branch_lane == start_lane {
			directions |= Directions::RIGHT;
		}

		directions
	}

	fn ensure_lane_capacity(
		lanes: &mut Vec<Cell>,
		required_index: usize,
	) {
		if lanes.len() <= required_index {
			lanes.resize(required_index + 1, Cell::default());
		}
	}

	pub fn render_row(
		&self,
		commit_id: &CommitId,
		current_snapshot: &[Option<LaneSlot>],
		previous_snapshot: Option<&[Option<LaneSlot>]>,
		branch_tips: &HashSet<CommitId>,
		stashes: &HashSet<CommitId>,
		head_id: Option<&CommitId>,
	) -> GraphRow {
		let commit_alias = self.oids.get(commit_id);
		let head_alias = head_id.and_then(|id| self.oids.get(id));
		let facts = commit_alias
			.map_or_else(CommitFacts::default, |alias| {
				self.facts(alias)
			});

		let commit_lane =
			Self::find_commit_lane(current_snapshot, commit_alias);
		let is_merge = facts.is_merge;
		let is_branch_tip = branch_tips.contains(commit_id);
		let is_stash = stashes.contains(commit_id);

		let branching_lanes = Self::find_branching_lanes(
			current_snapshot,
			previous_snapshot,
		);

		let merge_bridge =
			facts.second_parent.and_then(|parent_alias| {
				commit_lane.and_then(|cl| {
					Self::calculate_merge_bridge(
						current_snapshot,
						cl,
						parent_alias,
					)
				})
			});

		let mut lanes: Vec<Cell> = current_snapshot
			.iter()
			.enumerate()
			.map(|(lane_index, chunk_option)| {
				let Some(chunk) = chunk_option else {
					return Cell::default();
				};

				if commit_alias.is_some()
					&& chunk.alias() == commit_alias
				{
					let connection =
						Self::determine_commit_connection(
							is_stash,
							is_merge,
							is_branch_tip,
						);
					return Cell::commit(
						connection,
						lane_color(lane_index),
					);
				}

				Self::determine_passthrough_connection(
					chunk,
					head_alias,
					lane_color(lane_index),
				)
			})
			.collect();

		let branches = commit_lane.map_or_else(Vec::new, |cl| {
			Self::draw_merge_bridge(
				&mut lanes,
				merge_bridge,
				cl,
				current_snapshot,
				previous_snapshot,
			);

			Self::draw_branching_lanes(
				&mut lanes,
				&branching_lanes,
				cl,
			)
		});

		GraphRow {
			commit_lane: LaneIndex::from(commit_lane.unwrap_or(0)),
			is_merge,
			is_branch_tip,
			is_stash,
			lanes: lanes.into_iter().map(Cell::finish).collect(),
			merge_bridge: merge_bridge.map(|(source, target)| {
				(LaneIndex::from(source), LaneIndex::from(target))
			}),
			branches,
		}
	}

	/// Locates the primary lane for the current commit.
	///
	/// Returns `None` if `commit_alias` is `None` or no matching lane is found.
	fn find_commit_lane(
		current_snapshot: &[Option<LaneSlot>],
		commit_alias: Option<CommitAlias>,
	) -> Option<usize> {
		let target_alias = commit_alias?;

		current_snapshot.iter().position(|slot| {
			slot.as_ref().is_some_and(|chunk| {
				chunk.alias() == Some(target_alias)
			})
		})
	}

	/// Computes the span (min, max) between the commit's lane and its second parent's lane.
	fn calculate_merge_bridge(
		current_snapshot: &[Option<LaneSlot>],
		commit_lane: usize,
		second_parent_alias: CommitAlias,
	) -> Option<(usize, usize)> {
		lane_awaiting_parent(current_snapshot, second_parent_alias)
			.map(|target_lane| {
				(
					commit_lane.min(target_lane),
					commit_lane.max(target_lane),
				)
			})
	}

	/// Columns whose lane drew a line into the row above and has no
	/// occupant now: each one closed into this row's commit and needs
	/// a bridge drawn across to its lane.
	///
	/// A column the row above held a root commit in is not one of
	/// them. [`LaneSlot::Settled`] draws a marker and no line
	/// downward, so there would be nothing above for the bridge to
	/// join, and drawing one would invent an edge between two
	/// unrelated histories.
	fn find_branching_lanes(
		current_snapshot: &[Option<LaneSlot>],
		previous_snapshot: Option<&[Option<LaneSlot>]>,
	) -> Vec<usize> {
		let Some(previous) = previous_snapshot else {
			return Vec::new();
		};

		previous
			.iter()
			.enumerate()
			.filter_map(|(index, previous_chunk)| {
				(previous_chunk
					.is_some_and(|chunk| chunk.awaits().is_some())
					&& current_snapshot
						.get(index)
						.is_none_or(Option::is_none))
				.then_some(index)
			})
			.collect()
	}

	/// Determines the correct node type for the active commit lane.
	const fn determine_commit_connection(
		is_stash: bool,
		is_merge: bool,
		is_branch_tip: bool,
	) -> ConnectionType {
		match (is_stash, is_merge, is_branch_tip) {
			(true, _, _) => ConnectionType::CommitStash,
			(_, true, _) => ConnectionType::CommitMerge,
			(_, _, true) => ConnectionType::CommitBranch,
			_ => ConnectionType::CommitNormal,
		}
	}

	/// Determines the correct vertical line style for non-commit passthrough lanes.
	fn determine_passthrough_connection(
		chunk: &LaneSlot,
		head_alias: Option<CommitAlias>,
		color: LaneIndex,
	) -> Cell {
		// A settled lane draws nothing below its commit.
		if matches!(chunk, LaneSlot::Settled { .. }) {
			return Cell::default();
		}

		// Dot the segment of any lane, not just lane 0, that is
		// heading toward HEAD: HEAD is not guaranteed to sit on the
		// leftmost lane, e.g. once earlier lanes have closed and a
		// later one has taken their place.
		let is_dotted =
			head_alias.is_some() && chunk.awaits() == head_alias;

		Cell::line(
			Directions::UP | Directions::DOWN,
			color,
			is_dotted,
		)
	}

	/// Lay the horizontal run of a bridge over the lanes strictly
	/// between its two ends, merging with whatever each cell already
	/// shows.
	///
	/// These cells are passed *through*, never joined — a bridge's two
	/// ends are drawn by the callers — so the run is recorded as a
	/// crossing. That is what keeps an unrelated lane drawn whole
	/// underneath it, and what stops it from swallowing a junction
	/// another bridge left in the same cell.
	fn draw_bridge_span(
		lanes: &mut [Cell],
		from: usize,
		to: usize,
		color: LaneIndex,
	) {
		for lane in lanes.iter_mut().take(to).skip(from + 1) {
			lane.overlay(Directions::CROSS, color);
		}
	}
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod real_repository;
