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

