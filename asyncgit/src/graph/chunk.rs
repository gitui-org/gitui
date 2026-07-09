use super::{CommitAlias, UnwalkedAlias};

/// A lane's occupant at one point in the walk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LaneSlot {
	/// A walked commit flowing down to its first parent.
	Flowing {
		alias: CommitAlias,
		parent: UnwalkedAlias,
	},

	/// A flowing lane that still owes a bridge to a second parent
	FlowingMerge {
		alias: CommitAlias,
		parent: UnwalkedAlias,
		second: UnwalkedAlias,
	},

	/// A root commit: draws its node, nothing continues below.
	Settled { alias: CommitAlias },

	/// Reserved for a merge's second parent that hasn't been reached
	/// by the walk yet. Once that parent is processed, the
	/// reservation is replaced by the parent's own slot.
	Reserved { parent: UnwalkedAlias },
}

impl LaneSlot {
	/// The commit this lane currently belongs to, `None` for a
	/// reserved placeholder.
	pub const fn alias(&self) -> Option<CommitAlias> {
		match self {
			Self::Flowing { alias, .. }
			| Self::FlowingMerge { alias, .. }
			| Self::Settled { alias } => Some(*alias),
			Self::Reserved { .. } => None,
		}
	}

	/// The parent whose placement this lane waits on; the lane stays
	/// open (drawing a vertical line) until that commit is walked.
	pub const fn awaits(&self) -> Option<CommitAlias> {
		match self {
			Self::Flowing { parent, .. }
			| Self::FlowingMerge { parent, .. }
			| Self::Reserved { parent } => Some(parent.get()),
			Self::Settled { .. } => None,
		}
	}

	/// The pending second parent of a merge that hasn't been given
	/// its own lane.
	pub const fn second(&self) -> Option<CommitAlias> {
		match self {
			Self::FlowingMerge { second, .. } => Some(second.get()),
			Self::Flowing { .. }
			| Self::Settled { .. }
			| Self::Reserved { .. } => None,
		}
	}
}
