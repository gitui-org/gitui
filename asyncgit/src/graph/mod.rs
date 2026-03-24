pub mod buffer;
pub mod chunk;
pub mod oids;
pub mod walker;

pub use walker::GraphWalker;

#[derive(Clone, Debug, PartialEq)]
pub enum ConnType {
	Vertical,
	VerticalDotted,
	CommitNormal,
	CommitBranch,
	CommitMerge,
	CommitStash,
	CommitUncommitted,
	MergeBridgeStart,
	MergeBridgeMid,
	MergeBridgeEnd,
	BranchDown,
	BranchUp,
	BranchUpRight,
}

#[derive(Clone, Debug, Default)]
pub struct GraphRow {
	/// Number of active lanes at this commit row
	pub lane_count: usize,

	/// Which lane index this commit sits on
	pub commit_lane: usize,

	/// Whether this is a merge commit (two parents)
	pub is_merge: bool,

	/// Whether this commit is a branch tip
	pub is_branch_tip: bool,

	/// Whether this commit has stash marker
	pub is_stash: bool,

	/// Connections emitted per lane:
	/// None = empty space
	/// Some((ConnType, color_index)) = draw this connector in this color
	pub lanes: Vec<Option<(ConnType, usize)>>,

	/// Horizontal merge bridge: if this commit merges rightward,
	/// (from_lane, to_lane) — the span to draw ─ ╭ ╮ across
	pub merge_bridge: Option<(usize, usize)>,

	/// Horizontal branch bridges: if this commit spawns branches,
	/// spans to draw ─ ╭ ╮ across
	pub branches: Vec<(usize, usize)>,
}
