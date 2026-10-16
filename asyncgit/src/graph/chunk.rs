use super::{CommitAlias, UnwalkedAlias};

/// A lane's occupant at one point in the walk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaneSlot {
	/// A walked commit flowing down to its first parent.
	Flowing {
		alias: CommitAlias,
		parent: UnwalkedAlias,
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
			Self::Flowing { alias, .. } | Self::Settled { alias } => {
				Some(*alias)
			}
			Self::Reserved { .. } => None,
		}
	}

	/// The parent whose placement this lane waits on; the lane stays
	/// open (drawing a vertical line) until that commit is walked.
	pub fn awaits(&self) -> Option<CommitAlias> {
		match self {
			Self::Flowing { parent, .. }
			| Self::Reserved { parent } => Some(**parent),
			Self::Settled { .. } => None,
		}
	}
}

/// The first lane already heading for `parent`. Placement, reservation,
/// and bridge routing must all choose the same lane.
pub(super) fn lane_awaiting_parent(
	lanes: &[Option<LaneSlot>],
	parent: CommitAlias,
) -> Option<usize> {
	lanes.iter().position(|slot| {
		slot.is_some_and(|slot| slot.awaits() == Some(parent))
	})
}
