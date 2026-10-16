use super::{ConnectionType, GraphCell, LaneIndex};

use bitflags::bitflags;

bitflags! {
	/// The neighboring cells a lane joins to. Overlapping lines
	/// are merged through this representation rather than
	/// a naive overwrite for readability.
	///
	/// [`Self::LEFT`] and [`Self::RIGHT`] are a horizontal run that
	/// *joins* this cell; [`Self::CROSS`] is one that runs past it
	/// without joining. Keeping the two apart is what lets a bridge be
	/// drawn behind an unrelated lane, while a bridge that really ends
	/// on a lane still gets its junction even when a third line
	/// crosses the same cell.
	#[derive(Clone, Copy, Default)]
	pub(super) struct Directions: u8 {
		const UP = 0b0_0001;
		const DOWN = 0b0_0010;
		const LEFT = 0b0_0100;
		const RIGHT = 0b0_1000;
		const CROSS = 0b1_0000;
	}
}

impl Directions {
	fn vertical(self) -> bool {
		self.intersects(Self::UP | Self::DOWN)
	}
}

/// Accumulate geometry before choosing a glyph: a vertical glyph hides
/// a crossing, so converting it back to directions would lose that fact
/// before a later overlay could turn the cell into a junction.
#[derive(Clone, Copy, Default)]
pub(super) struct Cell {
	directions: Directions,
	color: LaneIndex,
	marker: Option<ConnectionType>,
	dotted: bool,
}

impl Cell {
	pub const fn line(
		directions: Directions,
		color: LaneIndex,
		dotted: bool,
	) -> Self {
		Self {
			directions,
			color,
			marker: None,
			dotted,
		}
	}

	pub const fn commit(
		marker: ConnectionType,
		color: LaneIndex,
	) -> Self {
		Self {
			directions: Directions::empty(),
			color,
			marker: Some(marker),
			dotted: false,
		}
	}

	/// Preserve commit markers and the color of the vertical lane.
	pub fn overlay(
		&mut self,
		directions: Directions,
		color: LaneIndex,
	) {
		if self.marker.is_some() {
			return;
		}
		if self.directions.is_empty()
			|| (!self.directions.vertical() && directions.vertical())
		{
			self.color = color;
		}
		self.directions |= directions;
	}

	pub const fn finish(self) -> GraphCell {
		let connection = match self.marker {
			Some(marker) => marker,
			None if self.directions.is_empty() => return None,
			None => {
				directions_to_connection(self.directions, self.dotted)
			}
		};
		Some((connection, self.color))
	}
}

const fn vertical_connection(dotted: bool) -> ConnectionType {
	if dotted {
		ConnectionType::VerticalDotted
	} else {
		ConnectionType::Vertical
	}
}

/// Synthesize a connection glyph from direction components.
const fn directions_to_connection(
	dirs: Directions,
	dotted: bool,
) -> ConnectionType {
	let up = dirs.contains(Directions::UP);
	let down = dirs.contains(Directions::DOWN);
	let joins_left = dirs.contains(Directions::LEFT);
	let joins_right = dirs.contains(Directions::RIGHT);
	let crossed = dirs.contains(Directions::CROSS);

	// A lane running straight through the cell. A bridge that merely
	// crosses it is drawn behind it, which keeps the lane readable and
	// is the whole reason a crossing is tracked apart from a join. A
	// bridge that ends on the lane needs a junction instead, and when
	// the cell is both joined and crossed there is nothing left to
	// hide the join behind, so it takes the full cross. Drawing that
	// case as a plain vertical, which is all a single horizontal bit
	// could do, loses the merge edge out of the picture entirely.
	if up && down {
		return match (joins_left, joins_right, crossed) {
			(false, false, _) => vertical_connection(dotted),
			(true, false, false) => ConnectionType::TeeLeft,
			(false, true, false) => ConnectionType::TeeRight,
			_ => ConnectionType::Cross,
		};
	}

	// Below here the cell has at most one vertical end, so a crossing
	// and a join on the same side are drawn alike.
	let left = joins_left || crossed;
	let right = joins_right || crossed;

	match (up, down, left, right) {
		(true, _, true, true) => ConnectionType::TeeUp,
		(_, true, true, true) => ConnectionType::TeeDown,
		(true, _, true, false) => ConnectionType::BranchUp,
		(true, _, false, true) => ConnectionType::BranchUpRight,
		(_, true, true, false) => ConnectionType::MergeBridgeStart,
		(_, true, false, true) => ConnectionType::MergeBridgeEnd,
		(false, false, _, _) => ConnectionType::MergeBridgeMid,
		(true | false, _, false, false) => {
			vertical_connection(dotted)
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn a_join_and_crossing_preserve_the_junction_in_either_order() {
		for overlays in [
			[Directions::CROSS, Directions::LEFT],
			[Directions::LEFT, Directions::CROSS],
		] {
			let mut cell = Cell::line(
				Directions::UP | Directions::DOWN,
				0.into(),
				false,
			);
			for directions in overlays {
				cell.overlay(directions, 1.into());
			}
			assert_eq!(
				cell.finish(),
				Some((ConnectionType::Cross, 0.into()))
			);
		}
	}

	#[test]
	fn a_crossing_preserves_a_dotted_lane_and_its_color() {
		let mut cell = Cell::line(
			Directions::UP | Directions::DOWN,
			2.into(),
			true,
		);
		cell.overlay(Directions::CROSS, 1.into());
		assert_eq!(
			cell.finish(),
			Some((ConnectionType::VerticalDotted, 2.into()))
		);
	}

	#[test]
	fn overlays_preserve_commit_markers() {
		let mut cell =
			Cell::commit(ConnectionType::CommitMerge, 2.into());
		cell.overlay(Directions::CROSS | Directions::UP, 1.into());
		assert_eq!(
			cell.finish(),
			Some((ConnectionType::CommitMerge, 2.into()))
		);
	}
}
