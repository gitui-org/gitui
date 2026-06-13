use super::buffer::Buffer;
use super::chunk::{Chunk, Markers};
use super::oids::GraphOids;
use super::{
	to_lane_idx, ConnectionType, GraphRow, LaneIdx, MAX_LANE_COLORS,
};
use crate::sync::CommitId;
use std::collections::{HashMap, HashSet};

/// Get the lanes color index, which cycles through the ste palette.
fn lane_color(lane: usize) -> LaneIdx {
	to_lane_idx(lane % MAX_LANE_COLORS)
}

use bitflags::bitflags;

bitflags! {
	/// The neighboring cells a lane joins to. Overlapping lines
	/// are merged through this representation rather than
	/// a naive overwrite for readability.
	#[derive(Clone, Copy, Default)]
	struct Dirs: u8 {
		const UP = 0b0001;
		const DOWN = 0b0010;
		const LEFT = 0b0100;
		const RIGHT = 0b1000;
	}
}

impl Dirs {
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
fn conn_dirs(conn: ConnectionType) -> Option<Dirs> {
	Some(match conn {
		ConnectionType::Vertical | ConnectionType::VerticalDotted => {
			Dirs::UP | Dirs::DOWN
		}
		ConnectionType::MergeBridgeMid => Dirs::LEFT | Dirs::RIGHT,
		ConnectionType::MergeBridgeStart => Dirs::DOWN | Dirs::LEFT,
		ConnectionType::MergeBridgeEnd => Dirs::DOWN | Dirs::RIGHT,
		ConnectionType::BranchUp => Dirs::UP | Dirs::LEFT,
		ConnectionType::BranchUpRight => Dirs::UP | Dirs::RIGHT,
		ConnectionType::TeeLeft => Dirs::UP | Dirs::DOWN | Dirs::LEFT,
		ConnectionType::TeeRight => {
			Dirs::UP | Dirs::DOWN | Dirs::RIGHT
		}
		ConnectionType::TeeUp => Dirs::UP | Dirs::LEFT | Dirs::RIGHT,
		ConnectionType::TeeDown => {
			Dirs::DOWN | Dirs::LEFT | Dirs::RIGHT
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
const fn dirs_conn(dirs: Dirs, dotted: bool) -> ConnectionType {
	let up = dirs.contains(Dirs::UP);
	let down = dirs.contains(Dirs::DOWN);
	let left = dirs.contains(Dirs::LEFT);
	let right = dirs.contains(Dirs::RIGHT);
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
	cell: &mut Option<(ConnectionType, LaneIdx)>,
	add: Dirs,
	color: LaneIdx,
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
	pub merge_parents: HashMap<usize, usize>,
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

	pub fn process(&mut self, id: CommitId, parents: &[CommitId]) {
		let alias = self.oids.get_or_insert(&id);

		let mut mapped_parents =
			parents.iter().map(|p| self.oids.get_or_insert(p));

		// We make the executive and saddening decision to not support octo/mega merges
		// TUIs are simply a backwards medium for representing this complexity
		let first_parent = mapped_parents.next();
		let second_parent = mapped_parents.next();

		let chunk = Chunk {
			alias: Some(alias),
			parent_a: first_parent,
			parent_b: second_parent,
			marker: Markers::Commit,
		};

		second_parent.map(|b| self.merge_parents.insert(alias, b));

		if first_parent.is_some()
			&& second_parent.is_some()
			&& !self.buffer.current.iter().flatten().any(|commit| {
				commit.parent_a == second_parent
					&& commit.parent_b.is_none()
			}) {
			self.buffer.track_merge_commit(alias);
		}

		self.buffer.update(&chunk);
	}

	/// Number of commits already folded into the graph buffer.
	pub fn processed_commits(&self) -> usize {
		self.buffer.deltas.len()
	}

	pub fn compute_rows(
		&self,
		commit_range: &[CommitId],
		global_start: usize,
		branch_tips: &HashSet<CommitId>,
		stashes: &HashSet<CommitId>,
		head_id: Option<&CommitId>,
	) -> Vec<GraphRow> {
		if commit_range.is_empty() {
			return Vec::new();
		}

		// decompress one row before the range (when there is one) so
		// every row's predecessor state comes from the same replay
		let snap_start = global_start.saturating_sub(1);
		let end = global_start + commit_range.len() - 1;
		let snapshots = self.buffer.decompress(snap_start, end);
		let offset = global_start - snap_start;

		commit_range
			.iter()
			.enumerate()
			.map(|(index, commit_id)| {
				let place = index + offset;

				let current: &[Option<Chunk>] =
					snapshots.get(place).map_or(&[], Vec::as_slice);
				let previous = place
					.checked_sub(1)
					.and_then(|i| snapshots.get(i))
					.map(Vec::as_slice);

				self.render_row(
					commit_id,
					current,
					previous,
					branch_tips,
					stashes,
					head_id,
				)
			})
			.collect()
	}

	fn draw_merge_bridge(
		lanes: &mut [Option<(ConnectionType, LaneIdx)>],
		merge_bridge: Option<(usize, usize)>,
		commit_lane: usize,
		current: &[Option<Chunk>],
		previous: Option<&[Option<Chunk>]>,
	) {
		let Some((from, to)) = merge_bridge.filter(|(f, t)| f != t)
		else {
			return;
		};
		let target_lane = if from == commit_lane { to } else { from };

		// only draw the corner continuing upward when the target
		// lane already existed on the previous row; a brand-new
		// lane starts at this corner
		let continues_up = current[target_lane].is_some()
			&& previous.is_some_and(|prev| {
				prev.get(target_lane) == Some(&current[target_lane])
			});

		// replace the plain vertical fill_lanes drew for the
		// target lane with the precise corner/junction
		lanes[target_lane] = None;
		let target_dirs = {
			let mut d = Dirs::DOWN;
			if continues_up {
				d |= Dirs::UP;
			}
			if target_lane > commit_lane {
				d |= Dirs::LEFT;
			}
			if target_lane < commit_lane {
				d |= Dirs::RIGHT;
			}
			d
		};
		overlay_cell(
			&mut lanes[target_lane],
			target_dirs,
			lane_color(target_lane),
		);

		Self::draw_bridge_span(
			lanes,
			from,
			to,
			lane_color(target_lane),
		);
	}

	fn draw_branching_lanes(
		lanes: &mut Vec<Option<(ConnectionType, LaneIdx)>>,
		branching_lanes: &[usize],
		commit_lane: usize,
	) -> Vec<(LaneIdx, LaneIdx)> {
		let mut branches = Vec::new();
		for &branch_lane in branching_lanes {
			let from = std::cmp::min(branch_lane, commit_lane);
			let to = std::cmp::max(branch_lane, commit_lane);
			branches.push((to_lane_idx(from), to_lane_idx(to)));

			if lanes.len() <= to {
				lanes.resize(to + 1, None);
			}

			Self::draw_bridge_span(
				lanes,
				from,
				to,
				lane_color(branch_lane),
			);
			let branch_dirs = {
				let mut d = Dirs::UP;
				if branch_lane == to {
					d |= Dirs::LEFT;
				}
				if branch_lane == from {
					d |= Dirs::RIGHT;
				}
				d
			};
			overlay_cell(
				&mut lanes[branch_lane],
				branch_dirs,
				lane_color(branch_lane),
			);
		}
		branches
	}

	fn render_row(
		&self,
		commit_id: &CommitId,
		current: &[Option<Chunk>],
		previous: Option<&[Option<Chunk>]>,
		branch_tips: &HashSet<CommitId>,
		stashes: &HashSet<CommitId>,
		head_id: Option<&CommitId>,
	) -> GraphRow {
		let alias = self.oids.get(commit_id);
		let commit_lane = current
			.iter()
			.position(|c| {
				c.as_ref().is_some_and(|chunk| {
					alias.is_some() && chunk.alias == alias
				})
			})
			.unwrap_or(0);

		let parent_b_alias =
			alias.and_then(|a| self.merge_parents.get(&a).copied());

		let is_merge = parent_b_alias.is_some();
		let is_branch_tip = branch_tips.contains(commit_id);
		let is_stash = stashes.contains(commit_id);

		let branching_lanes: Vec<usize> = previous
			.into_iter()
			.flatten() // Unwrapping the optional, returning empty vec when None
			.enumerate()
			.filter(|(i, pc)| {
				pc.is_some()
					&& current.get(*i).is_none_or(Option::is_none)
			})
			.map(|(i, _)| i)
			.collect();

		let mut lanes = vec![None; current.len()];

		let merge_bridge = if is_merge && parent_b_alias.is_some() {
			current
				.iter()
				.position(|c| {
					c.as_ref().is_some_and(|chunk| {
						chunk.parent_a == parent_b_alias
					})
				})
				.map(|t| (commit_lane.min(t), commit_lane.max(t)))
		} else {
			None
		};

		self.fill_lanes(
			&mut lanes,
			current,
			alias,
			head_id,
			is_stash,
			is_merge,
			is_branch_tip,
		);

		Self::draw_merge_bridge(
			&mut lanes,
			merge_bridge,
			commit_lane,
			current,
			previous,
		);

		let branches = Self::draw_branching_lanes(
			&mut lanes,
			&branching_lanes,
			commit_lane,
		);

		GraphRow {
			lane_count: to_lane_idx(current.iter().flatten().count()),
			commit_lane: to_lane_idx(commit_lane),
			is_merge,
			is_branch_tip,
			is_stash,
			lanes,
			merge_bridge: merge_bridge
				.map(|(f, t)| (to_lane_idx(f), to_lane_idx(t))),
			branches,
		}
	}

	#[allow(clippy::too_many_arguments)]
	fn fill_lanes(
		&self,
		lanes: &mut [Option<(ConnectionType, LaneIdx)>],
		curr: &[Option<Chunk>],
		alias: Option<usize>,
		head_id: Option<&CommitId>,
		is_stash: bool,
		is_merge: bool,
		is_branch_tip: bool,
	) {
		for (lane_idx, chunk_item) in curr.iter().enumerate() {
			let Some(chunk) = chunk_item.as_ref() else {
				continue;
			};

			if alias.is_some() && chunk.alias == alias {
				let conn_type =
					match (is_stash, is_merge, is_branch_tip) {
						(true, _, _) => ConnectionType::CommitStash,
						(_, true, _) => ConnectionType::CommitMerge,
						(_, _, true) => ConnectionType::CommitBranch,
						_ => ConnectionType::CommitNormal,
					};

				lanes[lane_idx] =
					Some((conn_type, lane_color(lane_idx)));
			} else {
				let target_oid =
					head_id.and_then(|h| self.oids.get(h));

				let is_dotted = lane_idx == 0
					&& target_oid.is_some()
					&& (chunk.parent_a == target_oid
						|| chunk.parent_b == target_oid);

				let is_orphan = chunk.parent_a.is_none()
					&& chunk.parent_b.is_none();

				if is_orphan {
					continue;
				}

				let conn = if is_dotted {
					ConnectionType::VerticalDotted
				} else {
					ConnectionType::Vertical
				};

				lanes[lane_idx] = Some((conn, lane_color(lane_idx)));
			}
		}
	}

	/// Lay the horizontal run of a bridge over the lanes strictly
	/// between its two ends, merging with whatever each cell already
	/// shows.
	fn draw_bridge_span(
		lanes: &mut [Option<(ConnectionType, LaneIdx)>],
		from: usize,
		to: usize,
		color: LaneIdx,
	) {
		for lane in lanes.iter_mut().take(to).skip(from + 1) {
			overlay_cell(lane, Dirs::LEFT | Dirs::RIGHT, color);
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
