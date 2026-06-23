pub mod buffer;
pub mod chunk;
pub mod oids;
pub mod walker;

pub use walker::GraphWalker;

/// The maximum number of colors to use for graph lanes
pub const MAX_LANE_COLORS: usize = 16;

// Yes, there are repositories where this is exceeded
// Are they very rare? Yes.
// On most terminals can more than 256 lanes even be represneted usefully? Not really.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LaneIndex(u8);

/// Numeric alias assigned to each commit in the graph.
///
/// The alias is a dense integer index created by [`GraphOids`](super::oids::GraphOids)
/// that avoids storing full [`CommitId`](crate::sync::CommitId)s inside the lane state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct AliasId(usize);

impl std::ops::Deref for AliasId {
	type Target = usize;
	fn deref(&self) -> &usize {
		&self.0
	}
}

impl From<usize> for AliasId {
	fn from(v: usize) -> Self {
		Self(v)
	}
}

impl From<usize> for LaneIndex {
	fn from(lane: usize) -> Self {
		Self(u8::try_from(lane).unwrap_or(u8::MAX))
	}
}

impl From<LaneIndex> for usize {
	fn from(lane: LaneIndex) -> Self {
		lane.0 as usize
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
}

#[derive(Clone, Debug, Default)]
pub struct GraphRow {
	/// Number of active lanes at this commit row
	pub lane_count: LaneIndex,

	/// Which lane index this commit sits on
	pub commit_lane: LaneIndex,

	/// Whether this is a merge commit (two parents)
	pub is_merge: bool,

	/// Whether this commit is a branch tip
	pub is_branch_tip: bool,

	/// Whether this commit has stash marker
	pub is_stash: bool,

	/// Connections emitted per lane:
	/// None = empty space
	/// Some((ConnectionType, `color_index`)) = draw this connector in this color
	pub lanes: Vec<Option<(ConnectionType, LaneIndex)>>,

	/// Horizontal merge bridge: if this commit merges rightward,
	/// (`from_lane`, `to_lane`) — the span to draw ─ ╭ ╮ across
	pub merge_bridge: Option<(LaneIndex, LaneIndex)>,

	/// Horizontal branch bridges: if this commit spawns branches,
	/// spans to draw ─ ╭ ╮ across
	pub branches: Vec<(LaneIndex, LaneIndex)>,
}
