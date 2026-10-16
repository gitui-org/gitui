//! Reads a drawn graph back into the history it depicts.
//!
//! Everything else in these tests would share the renderer's own idea
//! of what a glyph means, so a mistake in that table would be agreed
//! with rather than caught. This module starts from the characters
//! instead. `┫` joins up, down and left because that is what BOX
//! DRAWINGS HEAVY VERTICAL AND LEFT *is*, not because the walker says
//! so, and the table below is the only thing this module is told.
//!
//! What comes out is the history a person reads off the screen. Two
//! rules make that reading precise:
//!
//! - A commit marker is a node. Lines meet there and paths end there,
//!   so two commits are joined only by a run of line cells with no
//!   other commit on it. Two markers drawn one directly above the
//!   other in a column are joined as well, since that is how a lane
//!   with no line between consecutive commits is drawn.
//! - A plain vertical is transparent to a horizontal run: `o━┃━┛`
//!   reads as a bridge passing *behind* an unrelated lane, not as a
//!   junction with it. A run that really joins a lane is drawn as one
//!   of the tees, so entering a vertical sideways can only leave it
//!   sideways.

use std::collections::{BTreeSet, HashSet, VecDeque};

/// The side of a cell a line leaves by.
pub const UP: u8 = 1 << 0;
pub const DOWN: u8 = 1 << 1;
pub const LEFT: u8 = 1 << 2;
pub const RIGHT: u8 = 1 << 3;

/// Row and column of a cell in the drawing.
pub type Cursor = (usize, usize);

pub const fn side_name(side: u8) -> &'static str {
	match side {
		UP => "up",
		DOWN => "down",
		LEFT => "left",
		_ => "right",
	}
}

const fn opposite(side: u8) -> u8 {
	match side {
		UP => DOWN,
		DOWN => UP,
		LEFT => RIGHT,
		_ => LEFT,
	}
}

/// What a character is, going by the character alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cell {
	/// A commit. Lines meet here, and a path through the drawing ends
	/// here.
	Node,
	/// A line joining exactly these sides.
	Joint(u8),
	/// A plain vertical, which a horizontal run passes behind without
	/// joining.
	Crossing,
	/// Nothing drawn.
	Blank,
}

fn classify(cell: char) -> Cell {
	match cell {
		'o' | 'M' | '*' | '@' | '+' => Cell::Node,
		'┃' | '╏' => Cell::Crossing,
		'━' => Cell::Joint(LEFT | RIGHT),
		'┓' => Cell::Joint(DOWN | LEFT),
		'┏' => Cell::Joint(DOWN | RIGHT),
		'┛' => Cell::Joint(UP | LEFT),
		'┗' => Cell::Joint(UP | RIGHT),
		'┫' => Cell::Joint(UP | DOWN | LEFT),
		'┣' => Cell::Joint(UP | DOWN | RIGHT),
		'┻' => Cell::Joint(UP | LEFT | RIGHT),
		'┳' => Cell::Joint(DOWN | LEFT | RIGHT),
		'╋' => Cell::Joint(UP | DOWN | LEFT | RIGHT),
		// A blank, and anything the log view does not draw as part of
		// the graph, carries no line.
		_ => Cell::Blank,
	}
}

/// A rectangular picture: one `char` per cell, every row the same
/// width.
pub struct Picture(Vec<Vec<char>>);

impl Picture {
	/// Pad `rows` into a rectangle.
	///
	/// Padding matters: a line arriving in a column that the row above
	/// does not even reach is exactly the failure this module is here
	/// to find, so the short row has to be filled with blanks rather
	/// than treated as absent.
	pub fn new(rows: &[Vec<char>]) -> Self {
		let width =
			rows.iter().map(Vec::len).max().unwrap_or_default();
		Self(
			rows.iter()
				.map(|row| {
					let mut row = row.clone();
					row.resize(width, ' ');
					row
				})
				.collect(),
		)
	}

	fn at(&self, (row, column): Cursor) -> char {
		self.0
			.get(row)
			.and_then(|row| row.get(column))
			.copied()
			.unwrap_or(' ')
	}

	fn neighbour(
		&self,
		(row, column): Cursor,
		side: u8,
	) -> Option<Cursor> {
		let next = match side {
			UP => (row.checked_sub(1)?, column),
			DOWN => (row + 1, column),
			LEFT => (row, column.checked_sub(1)?),
			_ => (row, column + 1),
		};

		(next.0 < self.0.len()
			&& next.1 < self.0.first().map_or(0, Vec::len))
		.then_some(next)
	}

	/// Whether a line arriving from `from` is taken up by `cell`.
	fn accepts(cell: char, from: u8) -> bool {
		match classify(cell) {
			// A commit takes a line from any side, and a vertical
			// takes one from any side too: from above or below it
			// continues the lane, from either side it passes behind.
			Cell::Node | Cell::Crossing => true,
			Cell::Joint(sides) => sides & from != 0,
			Cell::Blank => false,
		}
	}

	/// The cell a line leaving `cursor` by `side` arrives in, if there
	/// is one there to take it.
	fn step(&self, cursor: Cursor, side: u8) -> Option<Cursor> {
		let next = self.neighbour(cursor, side)?;
		Self::accepts(self.at(next), opposite(side)).then_some(next)
	}

	/// Which commits the drawing joins to the commit at `start`.
	pub fn commits_joined_to(
		&self,
		start: Cursor,
	) -> BTreeSet<Cursor> {
		let mut found = BTreeSet::new();
		let mut seen: HashSet<(Cursor, u8)> = HashSet::new();
		let mut queue: VecDeque<(Cursor, u8)> = VecDeque::new();

		// A commit sends a line out of all four sides; which of them
		// go anywhere is decided by the cell on the other side.
		for side in [UP, DOWN, LEFT, RIGHT] {
			if let Some(next) = self.step(start, side) {
				queue.push_back((next, opposite(side)));
			}
		}

		while let Some((cursor, entered)) = queue.pop_front() {
			if !seen.insert((cursor, entered)) {
				continue;
			}

			let onward = match classify(self.at(cursor)) {
				Cell::Node => {
					if cursor != start {
						found.insert(cursor);
					}
					continue;
				}
				Cell::Blank => continue,
				// Sideways in, sideways out; a run that joined this
				// lane would have been drawn as a tee.
				Cell::Crossing => opposite(entered),
				Cell::Joint(sides) => sides & !entered,
			};

			for side in [UP, DOWN, LEFT, RIGHT] {
				if onward & side == 0 {
					continue;
				}
				if let Some(next) = self.step(cursor, side) {
					queue.push_back((next, opposite(side)));
				}
			}
		}

		found
	}

	/// Every line end that points at a cell with nothing to take it.
	///
	/// Lines may continue off the top and bottom of a history window.
	/// A horizontal line must end inside the picture.
	pub fn dangling_ends(&self) -> Vec<(Cursor, u8)> {
		let mut dangling = Vec::new();

		for (row, cells) in self.0.iter().enumerate() {
			for (column, cell) in cells.iter().enumerate() {
				let sides = match classify(*cell) {
					Cell::Joint(sides) => sides,
					// Only the lane is a line end; the sideways
					// transparency is not one.
					Cell::Crossing => UP | DOWN,
					Cell::Node | Cell::Blank => continue,
				};

				for side in [UP, DOWN, LEFT, RIGHT] {
					if sides & side == 0 {
						continue;
					}
					let cursor = (row, column);
					if matches!(side, UP | DOWN)
						&& self.neighbour(cursor, side).is_none()
					{
						continue;
					}
					if self.step(cursor, side).is_none() {
						dangling.push((cursor, side));
					}
				}
			}
		}

		dangling
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::graph::ConnectionType;

	fn picture(rows: &[&str]) -> Picture {
		Picture::new(
			&rows
				.iter()
				.map(|row| row.chars().collect())
				.collect::<Vec<_>>(),
		)
	}

	#[test]
	fn renderer_symbols_are_single_recognized_cells() {
		use ConnectionType::{
			BranchUp, BranchUpRight, CommitBranch, CommitMerge,
			CommitNormal, CommitStash, CommitUncommitted, Cross,
			MergeBridgeEnd, MergeBridgeMid, MergeBridgeStart,
			TeeDown, TeeLeft, TeeRight, TeeUp, Vertical,
			VerticalDotted,
		};
		for connection in [
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
			BranchUp,
			BranchUpRight,
			TeeLeft,
			TeeRight,
			TeeUp,
			TeeDown,
			Cross,
		] {
			let glyphs =
				connection.symbol().chars().collect::<Vec<_>>();
			assert_eq!(
				glyphs.len(),
				1,
				"{connection:?} occupies more than one cell"
			);
			let cell = classify(glyphs[0]);
			assert_ne!(
				cell,
				Cell::Blank,
				"unrecognized glyph for {connection:?}"
			);
			assert_eq!(cell == Cell::Node, connection.is_commit());
		}
	}

	#[test]
	fn crossing_preserves_two_separate_paths() {
		let graph = picture(&["  o", "o━┃━o", "  o"]);
		assert_eq!(
			graph.commits_joined_to((0, 2)),
			BTreeSet::from([(2, 2)])
		);
		assert_eq!(
			graph.commits_joined_to((1, 0)),
			BTreeSet::from([(1, 4)])
		);
		assert_eq!(graph.dangling_ends(), Vec::<(Cursor, u8)>::new());
	}

	#[test]
	fn junction_joins_all_four_paths() {
		let graph = picture(&["  o", "o━╋━o", "  o"]);
		assert_eq!(
			graph.commits_joined_to((0, 2)),
			BTreeSet::from([(1, 0), (1, 4), (2, 2)])
		);
		assert_eq!(graph.dangling_ends(), Vec::<(Cursor, u8)>::new());
	}

	#[test]
	fn commits_stop_paths() {
		let graph = picture(&["o", "o", "o"]);
		assert_eq!(
			graph.commits_joined_to((0, 0)),
			BTreeSet::from([(1, 0)])
		);
	}

	#[test]
	fn only_vertical_paths_can_leave_the_window() {
		assert_eq!(
			picture(&["┃"]).dangling_ends(),
			Vec::<(Cursor, u8)>::new()
		);
		assert_eq!(
			picture(&["━"]).dangling_ends(),
			vec![((0, 0), LEFT), ((0, 0), RIGHT)]
		);
		assert_eq!(
			picture(&["o", "  ┃"]).dangling_ends(),
			vec![((1, 2), UP)]
		);
	}
}
