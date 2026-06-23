use super::buffer::Buffer;
use super::chunk::{Chunk, Markers};
use super::oids::GraphOids;
use super::{
	AliasId, ConnectionType, GraphRow, LaneIndex, MAX_LANE_COLORS,
};
use crate::sync::CommitId;
use core::cmp::Ordering;
use std::collections::{HashMap, HashSet};

/// Get the lanes color index, which cycles through the ste palette.
fn lane_color(lane: usize) -> LaneIndex {
	LaneIndex::from(lane % MAX_LANE_COLORS)
}

use bitflags::bitflags;

bitflags! {
	/// The neighboring cells a lane joins to. Overlapping lines
	/// are merged through this representation rather than
	/// a naive overwrite for readability.
	#[derive(Clone, Copy, Default)]
	struct Directions: u8 {
		const UP = 0b0001;
		const DOWN = 0b0010;
		const LEFT = 0b0100;
		const RIGHT = 0b1000;
	}
}

impl Directions {
	#[allow(clippy::missing_const_for_fn)]
	fn merge(self, other: Self) -> Self {
		Self::from_bits_retain(self.bits() | other.bits())
	}

	#[allow(clippy::missing_const_for_fn)]
	fn vertical(self) -> bool {
		self.intersects(Self::UP | Self::DOWN)
	}
}

/// The sub network of an existing connection glyph
/// `None` represents commit markers, which are never drawn over.
fn conn_dirs(conn: ConnectionType) -> Option<Directions> {
	Some(match conn {
		ConnectionType::Vertical | ConnectionType::VerticalDotted => {
			Directions::UP | Directions::DOWN
		}
		ConnectionType::MergeBridgeMid => {
			Directions::LEFT | Directions::RIGHT
		}
		ConnectionType::MergeBridgeStart => {
			Directions::DOWN | Directions::LEFT
		}
		ConnectionType::MergeBridgeEnd => {
			Directions::DOWN | Directions::RIGHT
		}
		ConnectionType::BranchUp => Directions::UP | Directions::LEFT,
		ConnectionType::BranchUpRight => {
			Directions::UP | Directions::RIGHT
		}
		ConnectionType::TeeLeft => {
			Directions::UP | Directions::DOWN | Directions::LEFT
		}
		ConnectionType::TeeRight => {
			Directions::UP | Directions::DOWN | Directions::RIGHT
		}
		ConnectionType::TeeUp => {
			Directions::UP | Directions::LEFT | Directions::RIGHT
		}
		ConnectionType::TeeDown => {
			Directions::DOWN | Directions::LEFT | Directions::RIGHT
		}
		ConnectionType::CommitNormal
		| ConnectionType::CommitBranch
		| ConnectionType::CommitMerge
		| ConnectionType::CommitStash
		| ConnectionType::CommitUncommitted => return None,
	})
}

/// Determine the glyph for a cell's connectivity.
/// Vertical lines take precedence in crossed cells.
/// Yet the horizontal bridge continues in
/// the spacer columns either side, so we retain wholeness.
const fn dirs_conn(dirs: Directions, dotted: bool) -> ConnectionType {
	let up = dirs.contains(Directions::UP);
	let down = dirs.contains(Directions::DOWN);
	let left = dirs.contains(Directions::LEFT);
	let right = dirs.contains(Directions::RIGHT);
	match (up, down, left, right) {
		(true, true, true, false) => ConnectionType::TeeLeft,
		(true, true, false, true) => ConnectionType::TeeRight,
		(true, false, true, true) => ConnectionType::TeeUp,
		(false, true, true, true) => ConnectionType::TeeDown,
		(true, false, true, false) => ConnectionType::BranchUp,
		(true, false, false, true) => ConnectionType::BranchUpRight,
		(false, true, true, false) => {
			ConnectionType::MergeBridgeStart
		}
		(false, true, false, true) => ConnectionType::MergeBridgeEnd,
		(false, false, _, _) => ConnectionType::MergeBridgeMid,
		(true, true, _, _)
		| (true | false, false | true, false, false) => {
			if dotted {
				ConnectionType::VerticalDotted
			} else {
				ConnectionType::Vertical
			}
		}
	}
}

/// Draw `add` into a cell, merging with whatever is already there.
/// The line with a vertical component is the chosen way with color
/// ensuring lanes stay visually continuous
fn overlay_cell(
	cell: &mut Option<(ConnectionType, LaneIndex)>,
	add: Directions,
	color: LaneIndex,
) {
	if let Some((conn, existing_color)) = cell {
		if let Some(existing) = conn_dirs(*conn) {
			let is_dotted =
				matches!(conn, ConnectionType::VerticalDotted);

			let resolved_color =
				if existing.vertical() || !add.vertical() {
					*existing_color
				} else {
					color
				};

			*cell = Some((
				dirs_conn(existing.merge(add), is_dotted),
				resolved_color,
			));
		}
	} else {
		*cell = Some((dirs_conn(add, false), color));
	}
}

pub struct GraphWalker {
	pub buffer: Buffer,
	pub oids: GraphOids,
	pub branch_lane_map: HashMap<CommitId, usize>,

	/// Maps a merge commit's alias to the alias of its second parent.
	pub merge_parents: HashMap<AliasId, AliasId>,
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
			oids: GraphOids::new(),
			branch_lane_map: HashMap::new(),
			merge_parents: HashMap::new(),
		}
	}

	pub fn process(
		&mut self,
		commit_id: CommitId,
		parents: &[CommitId],
	) {
		let commit_alias = self.oids.get_or_insert(&commit_id);

		let mut mapped_parents = parents
			.iter()
			.map(|parent_id| self.oids.get_or_insert(parent_id));

		// We explicitly cap support at 2 parents, ignoring octo/mega merges.
		let first_parent = mapped_parents.next();
		let second_parent = mapped_parents.next();

		let chunk = Chunk {
			alias: Some(commit_alias),
			parent_a: first_parent,
			parent_b: second_parent,
			marker: Markers::Commit,
		};

		if let Some(second) = second_parent {
			self.merge_parents.insert(commit_alias, second);

			if first_parent.is_some()
				&& !self.is_redundant_merge_track(second)
			{
				self.buffer.track_merge_commit(commit_alias);
			}
		}

		self.buffer.update(&chunk);
	}

	/// Number of commits already folded into the graph buffer.
	pub const fn processed_commits(&self) -> usize {
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

		// Decompress one row before the range to establish predecessor state
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
		lanes: &mut [Option<(ConnectionType, LaneIndex)>],
		merge_bridge: Option<(usize, usize)>,
		commit_lane: usize,
		current_snapshot: &[Option<Chunk>],
		previous_snapshot: Option<&[Option<Chunk>]>,
	) {
		let Some((source_lane, target_lane)) = merge_bridge else {
			return;
		};
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
		lanes[destination_lane] = None;
		overlay_cell(
			&mut lanes[destination_lane],
			target_directions,
			connection_color,
		);

		Self::draw_bridge_span(
			lanes,
			source_lane,
			target_lane,
			connection_color,
		);
	}

	fn draw_branching_lanes(
		lanes: &mut Vec<Option<(ConnectionType, LaneIndex)>>,
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
				overlay_cell(
					&mut lanes[branch_lane],
					branch_directions,
					connection_color,
				);

				(
					LaneIndex::from(start_lane),
					LaneIndex::from(end_lane),
				)
			})
			.collect()
	}

	/// Checks if tracking a merge commit would be redundant based on current buffer state.
	fn is_redundant_merge_track(
		&self,
		target_parent: AliasId,
	) -> bool {
		self.buffer.current.iter().flatten().any(|commit| {
			commit.parent_a == Some(target_parent)
				&& commit.parent_b.is_none()
		})
	}

	/// Determines if a lane should draw an upward-connecting corner.
	fn lane_continues_upwards(
		target_lane: usize,
		current_snapshot: &[Option<Chunk>],
		previous_snapshot: Option<&[Option<Chunk>]>,
	) -> bool {
		let exists_in_current =
			current_snapshot.get(target_lane).is_some();
		let matches_previous =
			previous_snapshot.is_some_and(|previous| {
				previous.get(target_lane)
					== current_snapshot.get(target_lane)
			});

		exists_in_current && matches_previous
	}

	/// Uses `Ordering` to elegantly map spatial relationships to visual bitmasks.
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
		lanes: &mut Vec<Option<(ConnectionType, LaneIndex)>>,
		required_index: usize,
	) {
		if lanes.len() <= required_index {
			lanes.resize(required_index + 1, None);
		}
	}

	pub fn render_row(
		&self,
		commit_id: &CommitId,
		current_snapshot: &[Option<Chunk>],
		previous_snapshot: Option<&[Option<Chunk>]>,
		branch_tips: &HashSet<CommitId>,
		stashes: &HashSet<CommitId>,
		head_id: Option<&CommitId>,
	) -> GraphRow {
		let commit_alias = self.oids.get(commit_id);
		let head_alias = head_id.and_then(|id| self.oids.get(id));
		let second_parent_alias = commit_alias.and_then(|alias| {
			self.merge_parents.get(&alias).copied()
		});

		let commit_lane =
			Self::find_commit_lane(current_snapshot, commit_alias);
		let is_merge = second_parent_alias.is_some();
		let is_branch_tip = branch_tips.contains(commit_id);
		let is_stash = stashes.contains(commit_id);

		let branching_lanes = Self::find_branching_lanes(
			current_snapshot,
			previous_snapshot,
		);

		let merge_bridge =
			second_parent_alias.and_then(|parent_alias| {
				Self::calculate_merge_bridge(
					current_snapshot,
					commit_lane,
					parent_alias,
				)
			});

		let mut lanes: Vec<Option<(ConnectionType, LaneIndex)>> =
			current_snapshot
				.iter()
				.enumerate()
				.map(|(lane_index, chunk_option)| {
					let chunk = chunk_option.as_ref()?; // Returns None early if the chunk is missing

					if commit_alias.is_some()
						&& chunk.alias == commit_alias
					{
						let connection =
							Self::determine_commit_connection(
								is_stash,
								is_merge,
								is_branch_tip,
							);
						return Some((
							connection,
							lane_color(lane_index),
						));
					}

					Self::determine_passthrough_connection(
						chunk, lane_index, head_alias,
					)
					.map(|connection| {
						(connection, lane_color(lane_index))
					})
				})
				.collect();

		Self::draw_merge_bridge(
			&mut lanes,
			merge_bridge,
			commit_lane,
			current_snapshot,
			previous_snapshot,
		);

		let branches = Self::draw_branching_lanes(
			&mut lanes,
			&branching_lanes,
			commit_lane,
		);

		let active_lane_count =
			current_snapshot.iter().flatten().count();

		GraphRow {
			lane_count: LaneIndex::from(active_lane_count),
			commit_lane: LaneIndex::from(commit_lane),
			is_merge,
			is_branch_tip,
			is_stash,
			lanes,
			merge_bridge: merge_bridge.map(|(source, target)| {
				(LaneIndex::from(source), LaneIndex::from(target))
			}),
			branches,
		}
	}

	/// Locates the primary lane for the current commit.
	fn find_commit_lane(
		current_snapshot: &[Option<Chunk>],
		commit_alias: Option<AliasId>,
	) -> usize {
		let Some(target_alias) = commit_alias else {
			return 0;
		};

		current_snapshot
			.iter()
			.position(|chunk_option| {
				chunk_option.as_ref().is_some_and(|chunk| {
					chunk.alias == Some(target_alias)
				})
			})
			.unwrap_or(0)
	}

	/// Computes the span (min, max) between the commit's lane and its second parent's lane.
	fn calculate_merge_bridge(
		current_snapshot: &[Option<Chunk>],
		commit_lane: usize,
		second_parent_alias: AliasId,
	) -> Option<(usize, usize)> {
		current_snapshot
			.iter()
			.position(|chunk_option| {
				chunk_option.as_ref().is_some_and(|chunk| {
					chunk.parent_a == Some(second_parent_alias)
				})
			})
			.map(|target_lane| {
				(
					commit_lane.min(target_lane),
					commit_lane.max(target_lane),
				)
			})
	}

	/// Identifies lanes that existed in the previous row but terminated before the current row.
	fn find_branching_lanes(
		current_snapshot: &[Option<Chunk>],
		previous_snapshot: Option<&[Option<Chunk>]>,
	) -> Vec<usize> {
		let Some(previous) = previous_snapshot else {
			return Vec::new();
		};

		previous
			.iter()
			.enumerate()
			.filter(|(index, previous_chunk)| {
				previous_chunk.is_some()
					&& current_snapshot
						.get(*index)
						.is_none_or(Option::is_none)
			})
			.map(|(index, _)| index)
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
		chunk: &Chunk,
		lane_index: usize,
		head_alias: Option<AliasId>,
	) -> Option<ConnectionType> {
		let is_orphan =
			chunk.parent_a.is_none() && chunk.parent_b.is_none();

		if is_orphan {
			return None;
		}

		let is_dotted = lane_index == 0
			&& head_alias.is_some()
			&& (chunk.parent_a == head_alias
				|| chunk.parent_b == head_alias);

		if is_dotted {
			Some(ConnectionType::VerticalDotted)
		} else {
			Some(ConnectionType::Vertical)
		}
	}

	/// Lay the horizontal run of a bridge over the lanes strictly
	/// between its two ends, merging with whatever each cell already
	/// shows.
	fn draw_bridge_span(
		lanes: &mut [Option<(ConnectionType, LaneIndex)>],
		from: usize,
		to: usize,
		color: LaneIndex,
	) {
		for lane in lanes.iter_mut().take(to).skip(from + 1) {
			overlay_cell(
				lane,
				Directions::LEFT | Directions::RIGHT,
				color,
			);
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn id(n: usize) -> CommitId {
		CommitId::from_str_unchecked(&format!("{n:040x}"))
			.expect("valid oid")
	}

	fn sym(conn: ConnectionType) -> char {
		match conn {
			ConnectionType::Vertical => '┃',
			ConnectionType::VerticalDotted => '╏',
			ConnectionType::CommitNormal => 'o',
			ConnectionType::CommitBranch => '*',
			ConnectionType::CommitMerge => 'M',
			ConnectionType::CommitStash => '*',
			ConnectionType::CommitUncommitted => '+',
			ConnectionType::MergeBridgeStart => '┓',
			ConnectionType::MergeBridgeMid => '━',
			ConnectionType::MergeBridgeEnd => '┏',
			ConnectionType::BranchUp => '┛',
			ConnectionType::BranchUpRight => '┗',
			ConnectionType::TeeLeft => '┫',
			ConnectionType::TeeRight => '┣',
			ConnectionType::TeeUp => '┻',
			ConnectionType::TeeDown => '┳',
		}
	}

	/// Render a row the way the UI does: one glyph per lane plus a
	/// spacer that carries a bridge's horizontal run.
	fn row_to_string(row: &GraphRow) -> String {
		let mut out = String::new();
		for (lane_index, conn) in row.lanes.iter().enumerate() {
			out.push(conn.map_or(' ', |(c, _)| sym(c)));

			let in_bridge = row
				.merge_bridge
				.into_iter()
				.chain(row.branches.iter().copied())
				.any(|(from, to)| {
					lane_index >= usize::from(from)
						&& lane_index < usize::from(to)
				});
			out.push(if in_bridge { '━' } else { ' ' });
		}
		out.trim_end().to_string()
	}

	/// Walk `history` (newest first, `(commit, parents)`) and render
	/// every row.
	fn render(history: &[(usize, &[usize])]) -> Vec<String> {
		let mut walker = GraphWalker::new();
		let ids: Vec<CommitId> =
			history.iter().map(|(c, _)| id(*c)).collect();

		for (commit, parents) in history {
			let parents: Vec<CommitId> =
				parents.iter().map(|p| id(*p)).collect();
			walker.process(id(*commit), &parents);
		}

		walker
			.compute_rows(
				&ids,
				0,
				&HashSet::new(),
				&HashSet::new(),
				None,
			)
			.iter()
			.map(row_to_string)
			.collect()
	}

	#[test]
	fn linear_history() {
		let rows = render(&[(1, &[2]), (2, &[3]), (3, &[])]);
		assert_eq!(rows, vec!["o", "o", "o"]);
	}

	#[test]
	fn simple_merge() {
		// 1 merges 3 into the line 1 → 2 → 4, 3 → 4
		let rows =
			render(&[(1, &[2, 3]), (2, &[4]), (3, &[4]), (4, &[])]);
		assert_eq!(rows, vec!["M━┓", "o ┃", "┃ o", "o━┛"]);
	}

	#[test]
	fn merge_into_tracked_lane_continues_through_corner() {
		// 3's merge line joins lane 1 which keeps flowing to 5,
		// so the corner must be a junction (┫), not a dead end (┓)
		let rows = render(&[
			(1, &[3]),
			(2, &[5]),
			(3, &[4, 5]),
			(4, &[6]),
			(5, &[6]),
			(6, &[]),
		]);
		assert_eq!(
			rows,
			vec!["o", "┃ o", "M━┫", "o ┃", "┃ o", "o━┛"]
		);
	}

	#[test]
	fn merge_bridge_crosses_unrelated_lane() {
		// 3 (lane 2) merges into 1's line (lane 0) while 2's line
		// (lane 1) passes through: the crossed lane keeps its
		// vertical instead of being cut by the bridge
		let rows = render(&[
			(1, &[4]),
			(2, &[5]),
			(3, &[6, 4]),
			(4, &[7]),
			(5, &[7]),
			(6, &[7]),
			(7, &[]),
		]);
		assert_eq!(
			rows,
			vec![
				"o",
				"┃ o",
				"┣━┃━M",
				"o ┃ ┃",
				"┃ o ┃",
				"┃ ┃ o",
				"o━┻━┛",
			]
		);
	}

	#[test]
	fn overlapping_branch_bridges_keep_inner_corner() {
		// lanes 1 and 2 both close into the commit on lane 0; the
		// outer bridge passes through the inner corner (┻) instead
		// of erasing it
		let rows =
			render(&[(1, &[4]), (2, &[4]), (3, &[4]), (4, &[])]);
		assert_eq!(rows, vec!["o", "┃ o", "┃ ┃ o", "o━┻━┛"]);
	}

	#[test]
	fn merge_and_branch_bridges_overlap() {
		// commit 3 closes a branch from lane 2 while opening a merge
		// to lane 3, crossing lane 1: every line stays continuous
		let rows = render(&[
			(1, &[3, 4]),
			(2, &[3]),
			(3, &[5, 6]),
			(4, &[5]),
			(5, &[7]),
			(6, &[7]),
			(7, &[]),
		]);
		assert_eq!(
			rows,
			vec![
				"M━┓",
				"┃ ┃ o",
				"M━┃━┻━┓",
				"┃ o   ┃",
				"o━┛   ┃",
				"┃     o",
				"o━━━━━┛",
			]
		);
	}

	#[test]
	fn crossed_lane_keeps_own_color() {
		let rows = &[
			(1usize, &[4usize][..]),
			(2, &[5]),
			(3, &[6, 4]),
			(4, &[7]),
			(5, &[7]),
			(6, &[7]),
			(7, &[]),
		];
		let mut walker = GraphWalker::new();
		let ids: Vec<CommitId> =
			rows.iter().map(|(c, _)| id(*c)).collect();
		for (commit, parents) in rows {
			let parents: Vec<CommitId> =
				parents.iter().map(|p| id(*p)).collect();
			walker.process(id(*commit), &parents);
		}
		let computed = walker.compute_rows(
			&ids,
			0,
			&HashSet::new(),
			&HashSet::new(),
			None,
		);

		// row of commit 3: lane 1 is crossed by the merge bridge but
		// keeps both its vertical glyph and its own lane color
		let crossed = computed[2].lanes[1]
			.expect("crossed lane should not be empty");
		assert_eq!(crossed.0, ConnectionType::Vertical);
		assert_eq!(crossed.1, lane_color(1));
	}
}
