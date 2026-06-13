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
pub type LaneIdx = u8;

/// Convert a lane position into the compact [`LaneIdx`]
/// representation. This way we can keep full granularity when computing,
/// but not when storing.
pub(crate) fn to_lane_idx(lane: usize) -> LaneIdx {
	LaneIdx::try_from(lane).unwrap_or(LaneIdx::MAX)
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
	pub lane_count: LaneIdx,

	/// Which lane index this commit sits on
	pub commit_lane: LaneIdx,

	/// Whether this is a merge commit (two parents)
	pub is_merge: bool,

	/// Whether this commit is a branch tip
	pub is_branch_tip: bool,

	/// Whether this commit has stash marker
	pub is_stash: bool,

	/// Connections emitted per lane:
	/// None = empty space
	/// Some((ConnectionType, `color_index`)) = draw this connector in this color
	pub lanes: Vec<Option<(ConnectionType, LaneIdx)>>,

	/// Horizontal merge bridge: if this commit merges rightward,
	/// (`from_lane`, `to_lane`) — the span to draw ─ ╭ ╮ across
	pub merge_bridge: Option<(LaneIdx, LaneIdx)>,

	/// Horizontal branch bridges: if this commit spawns branches,
	/// spans to draw ─ ╭ ╮ across
	pub branches: Vec<(LaneIdx, LaneIdx)>,
}
