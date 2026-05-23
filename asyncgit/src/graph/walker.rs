use super::buffer::Buffer;
use super::chunk::{Chunk, Markers};
use super::oids::GraphOids;
use super::{ConnectionType, GraphRow, MAX_LANE_COLORS};
use crate::sync::{CommitId, CommitInfo};
use im::Vector;
use std::collections::{HashMap, HashSet};

pub struct GraphWalker {
	pub buffer: Buffer,
	pub oids: GraphOids,
	pub branch_lane_map: HashMap<CommitId, usize>,
	pub mergers_map: HashMap<usize, usize>,
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
			mergers_map: HashMap::new(),
		}
	}

	pub fn process(&mut self, commit: &CommitInfo) {
		let alias = self.oids.get_or_insert(&commit.id);
		let parent_a = commit
			.parents
			.first()
			.map(|p| self.oids.get_or_insert(p));
		let parent_b =
			commit.parents.get(1).map(|p| self.oids.get_or_insert(p));

		let chunk = Chunk {
			alias: Some(alias),
			parent_a,
			parent_b,
			marker: Markers::Commit,
		};

		if let Some(b) = parent_b {
			self.mergers_map.insert(alias, b);
		}

		if parent_a.is_some() && parent_b.is_some() {
			let already_tracked =
				self.buffer.current.iter().any(|c| {
					c.as_ref().is_some_and(|c| {
						c.parent_a == parent_b && c.parent_b.is_none()
					})
				});
			if !already_tracked {
				self.buffer.merger(alias);
			}
		}

		self.buffer.update(&chunk);
	}

	pub fn snapshot_at(
		&self,
		global_idx: usize,
	) -> Vector<Option<Chunk>> {
		self.buffer
			.decompress(global_idx, global_idx)
			.into_iter()
			.next()
			.unwrap_or_default()
	}

	pub fn compute_rows(
		&self,
		commit_range: &[CommitId],
		global_start: usize,
		branch_tips: &HashSet<CommitId>,
		stashes: &HashSet<CommitId>,
		head_id: Option<&CommitId>,
	) -> Vec<GraphRow> {
		let end = global_start + commit_range.len().saturating_sub(1);
		let snapshots = self.buffer.decompress(global_start, end);

		commit_range
			.iter()
			.enumerate()
			.map(|(index, commit_id)| {
				let curr =
					snapshots.get(index).cloned().unwrap_or_default();
				let prev = if index > 0 {
					snapshots.get(index - 1).cloned()
				} else if global_start > 0 {
					Some(self.snapshot_at(global_start - 1))
				} else {
					None
				};

				self.render_row(
					commit_id,
					&curr,
					prev.as_ref(),
					branch_tips,
					stashes,
					head_id,
				)
			})
			.collect()
	}

	fn render_row(
		&self,
		commit_id: &CommitId,
		curr: &Vector<Option<Chunk>>,
		prev: Option<&Vector<Option<Chunk>>>,
		branch_tips: &HashSet<CommitId>,
		stashes: &HashSet<CommitId>,
		head_id: Option<&CommitId>,
	) -> GraphRow {
		let alias = self.oids.get(commit_id);
		let commit_lane = curr
			.iter()
			.position(|c| {
				c.as_ref().is_some_and(|chunk| {
					alias.is_some() && chunk.alias == alias
				})
			})
			.unwrap_or(0);

		let parent_b_alias =
			alias.and_then(|a| self.mergers_map.get(&a).copied());

		let is_merge = parent_b_alias.is_some();
		let is_branch_tip = branch_tips.contains(commit_id);
		let is_stash = stashes.contains(commit_id);

		let branching_lanes: Vec<usize> = prev
			.into_iter()
			.flatten() // Unwrapping the optional, returning empty vec when None
			.enumerate()
			.filter(|(i, pc)| {
				pc.is_some()
					&& curr.get(*i).is_none_or(Option::is_none)
			})
			.map(|(i, _)| i)
			.collect();

		let mut lanes = vec![None; curr.len()];

		let merge_bridge = is_merge
			.then(|| {
				let target_lane = curr.iter().position(|c| {
					c.as_ref().is_some_and(|chunk| {
						parent_b_alias.is_some()
							&& chunk.parent_a == parent_b_alias
					})
				});
				target_lane.map(|t| {
					if t > commit_lane {
						(commit_lane, t)
					} else {
						(t, commit_lane)
					}
				})
			})
			.flatten();

		self.fill_lanes(
			&mut lanes,
			curr,
			alias,
			head_id,
			is_stash,
			is_merge,
			is_branch_tip,
			&branching_lanes,
		);

		if let Some((from, to)) = merge_bridge {
			let target_lane =
				if from == commit_lane { to } else { from };
			Self::draw_bridge(
				&mut lanes,
				from,
				to,
				commit_lane,
				target_lane,
				ConnectionType::MergeBridgeMid,
				ConnectionType::MergeBridgeStart,
				ConnectionType::MergeBridgeEnd,
			);
		}

		let mut branches = Vec::new();
		for &branch_lane in &branching_lanes {
			let from = std::cmp::min(branch_lane, commit_lane);
			let to = std::cmp::max(branch_lane, commit_lane);
			branches.push((from, to));

			if lanes.len() <= to {
				lanes.resize(to + 1, None);
			}

			Self::draw_bridge(
				&mut lanes,
				from,
				to,
				branch_lane,
				branch_lane,
				ConnectionType::MergeBridgeMid,
				ConnectionType::BranchUp,
				ConnectionType::BranchUpRight,
			);
		}

		GraphRow {
			lane_count: curr.iter().flatten().count(),
			commit_lane,
			is_merge,
			is_branch_tip,
			is_stash,
			lanes,
			merge_bridge,
			branches,
		}
	}

	#[allow(clippy::too_many_arguments)]
	fn fill_lanes(
		&self,
		lanes: &mut [Option<(ConnectionType, usize)>],
		curr: &Vector<Option<Chunk>>,
		alias: Option<usize>,
		head_id: Option<&CommitId>,
		is_stash: bool,
		is_merge: bool,
		is_branch_tip: bool,
		branching_lanes: &[usize],
	) {
		for (lane_idx, chunk_item) in curr.iter().enumerate() {
			let Some(chunk) = chunk_item.as_ref() else {
				if branching_lanes.contains(&lane_idx) {
					lanes[lane_idx] =
						Some((ConnectionType::BranchUp, lane_idx % MAX_LANE_COLORS));
				}
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

				lanes[lane_idx] = Some((conn_type, lane_idx % MAX_LANE_COLORS));
			} else {
				let is_dotted = head_id
					.and_then(|h| self.oids.get(h))
					.is_some_and(|ha| {
						chunk.parent_a == Some(ha)
							|| chunk.parent_b == Some(ha)
					}) && lane_idx == 0;

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

				lanes[lane_idx] = Some((conn, lane_idx % MAX_LANE_COLORS));
			}
		}
	}

	fn draw_bridge(
		lanes: &mut [Option<(ConnectionType, usize)>],
		from: usize,
		to: usize,
		color_lane: usize,
		corner_lane: usize,
		mid: ConnectionType,
		corner_right: ConnectionType,
		corner_left: ConnectionType,
	) {
		for lane in lanes.iter_mut().take(to).skip(from + 1) {
			match lane {
				Some((
					ConnectionType::Vertical | ConnectionType::VerticalDotted,
					_,
				)) => {
					*lane = Some((ConnectionType::Cross, color_lane % MAX_LANE_COLORS));
				}
				_ => {
					*lane = Some((mid, color_lane % MAX_LANE_COLORS));
				}
			}
		}

		if corner_lane == to {
			let new_corner = match (lanes[to], corner_right) {
				(Some((ConnectionType::MergeBridgeStart, _)), ConnectionType::BranchUp) => ConnectionType::BranchUpMergeStart,
				_ => corner_right,
			};
			lanes[to] = Some((new_corner, color_lane % MAX_LANE_COLORS));
		} else if corner_lane == from {
			let new_corner = match (lanes[from], corner_left) {
				(Some((ConnectionType::MergeBridgeEnd, _)), ConnectionType::BranchUpRight) => ConnectionType::BranchUpRightMergeEnd,
				_ => corner_left,
			};
			lanes[from] = Some((new_corner, color_lane % MAX_LANE_COLORS));
		}
	}
}
