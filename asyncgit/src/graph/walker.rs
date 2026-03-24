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

	pub fn process(&mut self, commit: &CommitInfo) {
		let alias = Some(self.oids.get_or_insert(&commit.id));
		let parent_a =
			commit.parents.get(0).map(|p| self.oids.get_or_insert(p));
		let parent_b =
			commit.parents.get(1).map(|p| self.oids.get_or_insert(p));

		let chunk = Chunk {
			alias,
			parent_a,
			parent_b,
			marker: Markers::Commit,
		};

		if let (Some(a), Some(b)) = (alias, parent_b) {
			self.mergers_map.insert(a, b);
		}

		if parent_a.is_some() && parent_b.is_some() {
			let already_tracked =
				self.buffer.current.iter().any(|c| {
					if let Some(c) = c {
						c.parent_a == parent_b && c.parent_b.is_none()
					} else {
						false
					}
				});
			if !already_tracked {
				self.buffer.merger(alias.unwrap());
			}
		}

		self.buffer.update(chunk);
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
				c.as_ref().map_or(false, |chunk| {
					alias.is_some() && chunk.alias == alias
				})
			})
			.unwrap_or(0);

		let parent_b_alias =
			alias.and_then(|a| self.mergers_map.get(&a).cloned());

		let is_merge = parent_b_alias.is_some();
		let is_branch_tip = branch_tips.contains(commit_id);
		let is_stash = stashes.contains(commit_id);

		let branching_lanes: Vec<usize> = prev
			.into_iter()
			.flatten() // Unwrapping the optional, returning empty vec when None
			.enumerate()
			.filter(|(i, pc)| {
				pc.is_some()
					&& curr.get(*i).map_or(true, |c| c.is_none())
			})
			.map(|(i, _)| i)
			.collect();

		let mut lanes = vec![None; curr.len()];

		let merge_bridge = is_merge
			.then(|| {
				let target_lane = curr.iter().position(|c| {
					c.as_ref().map_or(false, |chunk| {
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

		for (lane_idx, chunk_item) in curr.iter().enumerate() {
			if chunk_item.is_none() {
				if branching_lanes.contains(&lane_idx) {
					lanes[lane_idx] =
						Some((ConnType::BranchUp, lane_idx % 16));
				}
				continue;
			}

			let chunk = chunk_item.as_ref().unwrap();

			if alias.is_some() && chunk.alias == alias {
				// basically a from impl inline here
				let conn_type =
					match (is_stash, is_merge, is_branch_tip) {
						(true, _, _) => ConnType::CommitStash,
						(_, true, _) => ConnType::CommitMerge,
						(_, _, true) => ConnType::CommitBranch,
						_ => ConnType::CommitNormal,
					};

				lanes[lane_idx] = Some((conn_type, lane_idx % 16));
			} else {
				let is_dotted = head_id
					.and_then(|h| self.oids.get(h))
					.is_some_and(|ha| {
						chunk.parent_a == Some(ha)
							|| chunk.parent_b == Some(ha)
					}) && lane_idx == 0;

				let is_orphan = chunk.parent_a.is_none()
					&& chunk.parent_b.is_none();

				let conn = match (is_dotted, is_orphan) {
					(true, _) => ConnType::VerticalDotted,
					(_, true) => continue,
					_ => ConnType::Vertical,
				};

				lanes[lane_idx] = Some((conn, lane_idx % 16));
			}
		}

		if let Some((from, to)) = merge_bridge {
			for bridge_lane in (from + 1)..to {
				lanes[bridge_lane] = Some((
					ConnType::MergeBridgeMid,
					commit_lane % 16,
				));
			}
			if to > commit_lane {
				lanes[to] = Some((
					ConnType::MergeBridgeStart,
					commit_lane % 16,
				));
			} else if from < commit_lane {
				lanes[from] = Some((
					ConnType::MergeBridgeEnd,
					commit_lane % 16,
				));
			}
		}

		let mut branches = Vec::new();
		for &branch_lane in &branching_lanes {
			let from = std::cmp::min(branch_lane, commit_lane);
			let to = std::cmp::max(branch_lane, commit_lane);
			branches.push((from, to));

			if lanes.len() <= to {
				lanes.resize(to + 1, None);
			}

			for bridge_lane in (from + 1)..to {
				lanes[bridge_lane] = Some((
					ConnType::MergeBridgeMid,
					branch_lane % 16,
				));
			}
			if to > commit_lane {
				lanes[to] =
					Some((ConnType::BranchUp, branch_lane % 16));
			} else if from < commit_lane {
				lanes[from] =
					Some((ConnType::BranchUpRight, branch_lane % 16));
			}
		}

		GraphRow {
			lane_count: curr.iter().filter(|c| c.is_some()).count(),
			commit_lane,
			is_merge,
			is_branch_tip,
			is_stash,
			lanes,
			merge_bridge,
			branches,
		}
	}
}
