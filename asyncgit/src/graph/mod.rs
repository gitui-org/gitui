pub mod buffer;
mod cell;
pub mod chunk;
pub mod oids;
pub mod walker;

#[cfg(test)]
mod harness;

pub use walker::GraphWalker;

/// The maximum number of colors to use for graph lanes
pub const MAX_LANE_COLORS: usize = 16;

/// A lane's column within a row, or its slot in the color palette.
/// Column indices are lossless even when the graph is wider than a terminal.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LaneIndex(usize);

/// Numeric alias assigned to each commit in the graph.
///
/// The alias is a dense integer index created by [`GraphOids`](super::oids::GraphOids)
/// that avoids storing full [`CommitId`](crate::sync::CommitId)s inside the lane state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct CommitAlias(usize);

/// An alias for a commit that had not yet been walked when the value
/// was minted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnwalkedAlias(CommitAlias);

impl std::ops::Deref for UnwalkedAlias {
	type Target = CommitAlias;
	fn deref(&self) -> &CommitAlias {
		&self.0
	}
}

impl std::ops::Deref for CommitAlias {
	type Target = usize;
	fn deref(&self) -> &usize {
		&self.0
	}
}

impl From<usize> for CommitAlias {
	fn from(v: usize) -> Self {
		Self(v)
	}
}

impl From<usize> for LaneIndex {
	fn from(lane: usize) -> Self {
		Self(lane)
	}
}

impl From<LaneIndex> for usize {
	fn from(lane: LaneIndex) -> Self {
		lane.0
	}
}

/// The type of connection between nodes in the graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionType {
	Vertical,
	VerticalDotted,
	CommitNormal,
	CommitBranch,
	CommitMerge,
	CommitStash,
	CommitUncommitted,
	/// a bridge turning down into a lane that starts here
	MergeBridgeStart,
	/// a bridge passing over an empty lane slot
	MergeBridgeMid,
	/// a bridge turning down, lane starting to its right
	MergeBridgeEnd,
	/// a lane from above turning left into a commit
	BranchUp,
	///  a lane from above turning right into a commit
	BranchUpRight,
	/// a continuing lane absorbing a bridge from its left
	TeeLeft,
	/// a continuing lane absorbing a bridge from its right
	TeeRight,
	/// a lane ending from above while a bridge passes through
	TeeUp,
	/// a lane starting downward while a bridge passes through
	TeeDown,
	/// a lane a bridge joins at a cell another bridge runs through
	Cross,
}

impl ConnectionType {
	/// The terminal glyph used by both the log view and graph readback tests.
	pub const fn symbol(self) -> &'static str {
		match self {
			Self::Vertical => "┃",
			Self::VerticalDotted => "╏",
			Self::CommitNormal => "o",
			Self::CommitBranch | Self::CommitStash => "*",
			Self::CommitMerge => "M",
			Self::CommitUncommitted => "+",
			Self::MergeBridgeStart => "┓",
			Self::MergeBridgeMid => "━",
			Self::MergeBridgeEnd => "┏",
			Self::BranchUp => "┛",
			Self::BranchUpRight => "┗",
			Self::TeeLeft => "┫",
			Self::TeeRight => "┣",
			Self::TeeUp => "┻",
			Self::TeeDown => "┳",
			Self::Cross => "╋",
		}
	}

	/// Whether this is a commit marker rather than a line.
	///
	/// A marker carries no line of its own and is never drawn over:
	/// whether a line reaches it from above, or leaves it below, is a
	/// fact about the commit's lane and not about the glyph.
	pub const fn is_commit(self) -> bool {
		matches!(
			self,
			Self::CommitNormal
				| Self::CommitBranch
				| Self::CommitMerge
				| Self::CommitStash
				| Self::CommitUncommitted
		)
	}
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GraphRow {
	/// Which lane index this commit sits on
	pub commit_lane: LaneIndex,

	/// Whether Git reports more than one parent for this commit
	pub is_merge: bool,

	/// Whether this commit is a branch tip
	pub is_branch_tip: bool,

	/// Whether this commit has stash marker
	pub is_stash: bool,

	/// Connections emitted per lane:
	/// None = empty space
	/// Some((ConnectionType, `color_index`)) = draw this connector in this color
	pub lanes: Vec<GraphCell>,

	/// Horizontal merge bridge: if this commit merges rightward,
	/// (`from_lane`, `to_lane`) — the span to draw ─ ╭ ╮ across
	pub merge_bridge: Option<(LaneIndex, LaneIndex)>,

	/// Horizontal branch bridges: if this commit spawns branches,
	/// spans to draw ─ ╭ ╮ across
	pub branches: Vec<(LaneIndex, LaneIndex)>,
}

/// A glyph and its lane color, or an empty terminal cell.
pub type GraphCell = Option<(ConnectionType, LaneIndex)>;

impl GraphRow {
	/// Number of lane columns, including gaps between open lanes.
	pub const fn lane_count(&self) -> usize {
		self.lanes.len()
	}

	/// Two terminal cells per lane: its connector and the horizontal
	/// bridge segment (or blank) immediately to its right.
	pub fn columns(
		&self,
	) -> impl Iterator<Item = [GraphCell; 2]> + '_ {
		self.lanes.iter().enumerate().map(|(lane, &cell)| {
			let spacer = self
				.merge_bridge
				.into_iter()
				.chain(self.branches.iter().copied())
				.rfind(|&(from, to)| {
					(usize::from(from)..usize::from(to))
						.contains(&lane)
				})
				.map(|(from, to)| {
					let target = if from == self.commit_lane {
						to
					} else {
						from
					};
					(
						ConnectionType::MergeBridgeMid,
						LaneIndex::from(
							usize::from(target) % MAX_LANE_COLORS,
						),
					)
				});
			[cell, spacer]
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn lane_indices_preserve_columns_beyond_u16() {
		for column in [usize::from(u16::MAX) + 1, usize::MAX] {
			assert_eq!(usize::from(LaneIndex::from(column)), column);
		}
	}
}
