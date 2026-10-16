//! Test scaffolding for the commit graph.
//!
//! A graph test used to be a golden drawing: walk a history, render it
//! to ASCII, compare against a picture written by hand. That catches
//! cosmetic drift, but on its own it cannot tell a correct graph from a
//! wrong one, because the cheapest way to make a failing golden test
//! pass is to paste in whatever the renderer now produces. A regression
//! then arrives already blessed, with a confident comment explaining
//! the new picture.
//!
//! So [`Scene::walk`] holds every history to the invariants below
//! before any test gets to look at it, and a golden drawing is left to
//! do what a drawing is actually good at: saying which of the legal
//! pictures we want.
//!
//! The invariants come in two kinds. Some are about the lane state —
//! a lane keeps its column, a delta only addresses columns that exist,
//! a window of rows rebuilds to what the whole walk drew. The sharpest
//! ones are not: [`readback`] reads the finished picture back into the
//! history a person would read off the screen, using nothing but the
//! meaning of the box-drawing characters, and that is compared against
//! the parent lists Git handed us. A renderer cannot satisfy those by
//! being self-consistent.

mod generated;
mod readback;
mod tests;

pub use generated::{walk_generated, Shape};

use super::buffer::DeltaOp;
use super::chunk::LaneSlot;
use super::{CommitAlias, ConnectionType, GraphRow, GraphWalker};
use crate::sync::CommitId;
use readback::{side_name, Cursor, Picture};
use std::collections::{BTreeSet, HashMap, HashSet};

/// How many parents a commit can have an edge drawn for.
const DRAWN_PARENTS: usize = 2;

/// Above this many rows, window checks sample offsets instead of
/// trying every one.
const EXHAUSTIVE_WINDOW_ROWS: usize = 64;

/// The commit id test histories use for the commit named `n`.
pub fn oid(n: usize) -> CommitId {
	CommitId::from_str_unchecked(&format!("{n:040x}"))
		.expect("valid oid")
}

/// One walked history: the rows it draws, and the lane state behind
/// them.
pub struct Scene {
	pub walker: GraphWalker,
	pub ids: Vec<CommitId>,
	pub parents: Vec<Vec<CommitId>>,
	pub rows: Vec<GraphRow>,
	pub snapshots: Vec<Vec<Option<LaneSlot>>>,
	row_of: HashMap<CommitId, usize>,
	branch_tips: HashSet<CommitId>,
	stashes: HashSet<CommitId>,
	head: Option<CommitId>,
}

impl Scene {
	/// Walk `history`, newest commit first, with commits named by
	/// small integers: `(commit, parents)`.
	pub fn walk(history: &[(usize, &[usize])]) -> Self {
		Self::walk_marked(history, &[], &[], None)
	}

	/// [`Self::walk`] with branch tips, stashes and `HEAD` named by the
	/// same integers.
	pub fn walk_marked(
		history: &[(usize, &[usize])],
		branch_tips: &[usize],
		stashes: &[usize],
		head: Option<usize>,
	) -> Self {
		let named = history.iter().map(|(commit, parents)| {
			(oid(*commit), parents.iter().copied().map(oid).collect())
		});

		Self::walk_ids(
			named,
			branch_tips.iter().copied().map(oid).collect(),
			stashes.iter().copied().map(oid).collect(),
			head.map(oid),
		)
	}

	/// Walk a history already expressed as commit ids.
	pub fn walk_ids(
		history: impl IntoIterator<Item = (CommitId, Vec<CommitId>)>,
		branch_tips: HashSet<CommitId>,
		stashes: HashSet<CommitId>,
		head: Option<CommitId>,
	) -> Self {
		let mut walker = GraphWalker::new();
		let mut ids = Vec::new();
		let mut parents = Vec::new();
		let mut row_of = HashMap::new();
		let mut snapshots = Vec::new();

		for (commit, commit_parents) in history {
			walker.process(commit, &commit_parents);
			snapshots.push(walker.buffer.current.clone());
			row_of.entry(commit).or_insert(ids.len());
			ids.push(commit);
			parents.push(commit_parents);
		}

		let rows = walker.compute_rows(
			&ids,
			0,
			&branch_tips,
			&stashes,
			head.as_ref(),
		);

		let scene = Self {
			walker,
			ids,
			parents,
			rows,
			snapshots,
			row_of,
			branch_tips,
			stashes,
			head,
		};
		scene.assert_invariants();
		scene
	}

	/// The picture as the log view draws it: one glyph per lane column,
	/// plus the spacer that carries a bridge's horizontal run.
	fn cells(&self) -> Vec<Vec<char>> {
		self.rows
			.iter()
			.map(|row| {
				row.columns()
					.flatten()
					.flat_map(|cell| {
						cell.map_or(" ", |(connection, _)| {
							connection.symbol()
						})
						.chars()
					})
					.collect()
			})
			.collect()
	}

	/// The drawn rows, trailing blanks trimmed.
	pub fn drawing(&self) -> Vec<String> {
		self.cells()
			.iter()
			.map(|cells| {
				cells.iter().collect::<String>().trim_end().to_owned()
			})
			.collect()
	}

	/// Assert the rows draw exactly `expected`.
	pub fn assert_drawing(&self, expected: &[&str]) {
		assert_eq!(
			self.drawing(),
			expected,
			"\ndrawn:\n{}",
			self.picture()
		);
	}

	/// Assert no lane is still waiting on a parent, which is how a
	/// history whose every parent was walked has to end.
	pub fn assert_fully_settled(&self) {
		for slot in self.walker.buffer.current.iter().flatten() {
			assert!(
				matches!(slot, LaneSlot::Settled { .. }),
				"lane still waiting on a parent after a complete \
				 walk: {slot:?}\n{}",
				self.picture()
			);
		}
	}

	/// How many columns are still open at the end of the walk.
	pub fn open_lane_count(&self) -> usize {
		self.walker
			.buffer
			.current
			.iter()
			.flatten()
			.filter(|slot| slot.awaits().is_some())
			.count()
	}

	/// The drawing with row numbers, for failure messages.
	pub fn picture(&self) -> String {
		self.drawing()
			.iter()
			.enumerate()
			.map(|(index, row)| format!("{index:>4}  {row}"))
			.collect::<Vec<_>>()
			.join("\n")
	}

	fn alias(&self, index: usize) -> CommitAlias {
		self.walker
			.oids
			.get(&self.ids[index])
			.expect("every walked commit has an alias")
	}

	/// Where the row's commit marker sits in the drawing.
	fn marker(&self, index: usize) -> Cursor {
		(index, usize::from(self.rows[index].commit_lane) * 2)
	}

	fn slot(&self, index: usize, column: usize) -> Option<LaneSlot> {
		self.snapshots.get(index)?.get(column).copied().flatten()
	}

	/// The parents this row was entitled to draw an edge to: the first
	/// two distinct parents the walk had not already passed.
	///
	/// This policy is stated independently of the walker's parent
	/// selection. Each row supports at most one merge bridge; repeated,
	/// self-naming, and already-walked parents must not open lanes.
	fn drawn_parents(&self, index: usize) -> Vec<CommitId> {
		let id = &self.ids[index];
		let mut drawn: Vec<CommitId> = Vec::new();

		for parent in &self.parents[index] {
			if drawn.len() == DRAWN_PARENTS {
				break;
			}
			let walked_already = self
				.row_of
				.get(parent)
				.is_some_and(|row| *row <= index);
			if parent == id || walked_already {
				continue;
			}
			if !drawn.contains(parent) {
				drawn.push(*parent);
			}
		}

		drawn
	}

	/// Rows of the parents this row draws an edge to, for the parents
	/// that are themselves in the walked window.
	fn drawn_parent_rows(&self, index: usize) -> Vec<usize> {
		self.drawn_parents(index)
			.iter()
			.filter_map(|parent| self.row_of.get(parent).copied())
			.collect()
	}
}

/// The invariants every scene is held to.
impl Scene {
	fn assert_invariants(&self) {
		self.assert_snapshots_track_the_walk();
		self.assert_frontier_matches_the_walk();
		self.assert_delta_stream_is_well_formed();
		self.assert_columns_hold_still();
		self.assert_rows_are_well_formed();
		self.assert_windows_decompress_exactly();

		self.assert_picture_preserves_history(&Picture::new(
			&self.cells(),
		));
	}

	fn assert_picture_preserves_history(&self, picture: &Picture) {
		let joined = (0..self.rows.len())
			.map(|index| {
				picture.commits_joined_to(self.marker(index))
			})
			.collect::<Vec<_>>();

		self.assert_no_dangling_lines(picture);
		self.assert_every_parent_edge_is_drawn(&joined);
		self.assert_unrelated_histories_stay_apart(&joined);
		self.assert_stacked_commits_are_parent_and_child();
	}

	/// One snapshot per walked commit, ending in the live lane state.
	fn assert_snapshots_track_the_walk(&self) {
		assert_eq!(
			self.snapshots.len(),
			self.ids.len(),
			"the walk lost snapshots"
		);
		assert_eq!(
			self.walker.processed_commit_count(),
			self.ids.len(),
			"the buffer holds a delta count the walk did not produce"
		);
		if let Some(last) = self.ids.len().checked_sub(1) {
			assert_eq!(
				self.walker.buffer.decompress(0, last), self.snapshots,
				"delta replay differs from the states recorded during the walk"
			);
		}

		if let Some(last) = self.snapshots.last() {
			assert_eq!(
				last, &self.walker.buffer.current,
				"the last recorded snapshot is not the live state"
			);
		}
	}

	/// The raw history determines which parents are still ahead of
	/// each row. Every one needs a lane, and no lane may await anything
	/// else, including a parent the walk has already passed.
	fn assert_frontier_matches_the_walk(&self) {
		let mut expected = HashSet::new();
		for (index, snapshot) in self.snapshots.iter().enumerate() {
			expected.remove(&self.alias(index));
			for parent in self.drawn_parents(index) {
				expected.insert(
					self.walker
						.oids
						.get(&parent)
						.expect("a drawable parent has an alias"),
				);
			}
			let actual = snapshot
				.iter()
				.flatten()
				.filter_map(LaneSlot::awaits)
				.collect::<HashSet<_>>();
			assert_eq!(
				actual,
				expected,
				"row {index} awaits the wrong parents\n{}",
				self.picture()
			);
		}
	}

	/// Every recorded operation addresses a column that exists, and a
	/// column is only ever dropped once it is vacant.
	///
	/// Replaying the stream is what makes the delta buffer's promise
	/// testable. The three operations cannot express a sideways move
	/// at all; this checks that nothing in the stream leans on a
	/// column which was never opened, which is the other way a replay
	/// could drift from the walk.
	fn assert_delta_stream_is_well_formed(&self) {
		let mut columns: Vec<bool> = Vec::new();

		for (index, delta) in
			self.walker.buffer.deltas.iter().enumerate()
		{
			for op in &delta.0 {
				match *op {
					DeltaOp::Set {
						index: column,
						slot,
					} => {
						assert!(
							column < columns.len(),
							"delta {index} sets column {column} of \
							 {}",
							columns.len()
						);
						columns[column] = slot.is_some();
					}
					DeltaOp::Push { .. } => columns.push(true),
					DeltaOp::Pop => {
						assert_eq!(
							columns.pop(),
							Some(false),
							"delta {index} dropped a column that \
							 still held a lane"
						);
					}
				}
			}
		}
	}

	/// A lane keeps its column for as long as it stays open.
	///
	/// Rows are drawn one at a time and joined to their neighbours by
	/// column, so a lane that changed column between two rows would be
	/// two line segments that do not meet. The only lanes allowed to
	/// leave a column are the ones the row's own commit closes, by
	/// being the parent they were waiting for.
	fn assert_columns_hold_still(&self) {
		for index in 1..self.snapshots.len() {
			let alias = self.alias(index);

			for column in 0..self.snapshots[index - 1].len() {
				let Some(awaited) = self
					.slot(index - 1, column)
					.and_then(|slot| slot.awaits())
				else {
					continue;
				};
				let now = self.slot(index, column);

				if awaited == alias {
					assert!(
						now.is_none()
							|| now.and_then(|slot| slot.alias())
								== Some(alias),
						"row {index} took over column {column} with \
						 a lane that is not its own commit\n{}",
						self.picture()
					);
					continue;
				}

				assert_eq!(
					now.and_then(|slot| slot.awaits()),
					Some(awaited),
					"a lane waiting for {awaited:?} left column \
					 {column} between rows {} and {index}\n{}",
					index - 1,
					self.picture()
				);
			}
		}
	}

	/// Each row's own bookkeeping agrees with what it drew.
	fn assert_rows_are_well_formed(&self) {
		for (index, row) in self.rows.iter().enumerate() {
			let width = row.lanes.len();

			let markers = row
				.lanes
				.iter()
				.enumerate()
				.filter(|(_, cell)| {
					cell.is_some_and(|(conn, _)| conn.is_commit())
				})
				.map(|(column, _)| column)
				.collect::<Vec<_>>();
			assert_eq!(
				markers,
				vec![usize::from(row.commit_lane)],
				"row {index} draws its commit somewhere other than \
				 its own lane\n{}",
				self.picture()
			);

			assert_eq!(
				row.is_merge,
				self.parents[index].len() > 1,
				"row {index} disagrees with Git about being a merge"
			);
			assert_eq!(
				row.is_branch_tip,
				self.branch_tips.contains(&self.ids[index]),
				"row {index} disagrees about being a branch tip"
			);
			assert_eq!(
				row.is_stash,
				self.stashes.contains(&self.ids[index]),
				"row {index} disagrees about being a stash"
			);
			let expected_marker =
				if self.stashes.contains(&self.ids[index]) {
					ConnectionType::CommitStash
				} else if self.parents[index].len() > 1 {
					ConnectionType::CommitMerge
				} else if self.branch_tips.contains(&self.ids[index])
				{
					ConnectionType::CommitBranch
				} else {
					ConnectionType::CommitNormal
				};
			assert_eq!(
				row.lanes[usize::from(row.commit_lane)]
					.map(|(connection, _)| connection),
				Some(expected_marker),
				"row {index} draws the wrong commit marker"
			);

			for (from, to) in row
				.merge_bridge
				.into_iter()
				.chain(row.branches.iter().copied())
			{
				let (from, to) = (usize::from(from), usize::from(to));
				assert!(from < to, "row {index} has an empty bridge");
				assert!(
					to < width,
					"row {index} bridges past its own width"
				);
				assert!(
					from == usize::from(row.commit_lane)
						|| to == usize::from(row.commit_lane),
					"row {index} has a bridge with neither end at \
					 its commit"
				);
			}
		}
	}

	/// No line in the picture just stops.
	///
	/// Every line end has to point at a cell that takes it up. This is
	/// the local form of the column invariant, read off the drawing
	/// rather than the lane state: a lane that moved sideways leaves a
	/// vertical hanging in the column it left and another one with
	/// nothing above it in the column it moved to.
	fn assert_no_dangling_lines(&self, picture: &Picture) {
		let dangling = picture
			.dangling_ends()
			.iter()
			.map(|&((row, column), side)| {
				format!(
					"row {row} column {column} runs {} into nothing",
					side_name(side)
				)
			})
			.collect::<Vec<_>>();

		assert!(
			dangling.is_empty(),
			"{}\n{}",
			dangling.join("\n"),
			self.picture()
		);
	}

	/// Every edge the walk was entitled to draw can be followed in the
	/// picture.
	///
	/// This is the invariant a golden drawing cannot give us. It reads
	/// the finished characters back into a history and insists that
	/// each commit really does reach each of its parents through the
	/// lines on screen, which is the whole claim the graph column
	/// makes.
	fn assert_every_parent_edge_is_drawn(
		&self,
		joined: &[BTreeSet<Cursor>],
	) {
		for (index, reached) in joined.iter().enumerate() {
			for parent in self.drawn_parent_rows(index) {
				assert!(
					reached.contains(&self.marker(parent)),
					"row {index} cannot be followed to its parent \
					 on row {parent}\n{}",
					self.picture()
				);
			}
		}
	}

	/// The picture never joins two commits from histories that share
	/// no ancestry.
	///
	/// Lanes are shared on purpose — a merge bridges into a lane that
	/// already leads to the same parent instead of opening a second
	/// one — so a drawing legitimately joins commits that are not
	/// directly related. What it must never do is run a line between
	/// two unrelated histories, which is what reusing a column too
	/// eagerly looks like on screen.
	fn assert_unrelated_histories_stay_apart(
		&self,
		joined: &[BTreeSet<Cursor>],
	) {
		let components = self.ancestry_components();

		for (index, reached) in joined.iter().enumerate() {
			for &(row, _) in reached {
				assert_eq!(
					components[index],
					components[row],
					"the picture joins row {index} to row {row}, \
					 which share no ancestry\n{}",
					self.picture()
				);
			}
		}
	}

	/// Two commits drawn one directly above the other in a column read
	/// as joined, because a lane draws no line between consecutive
	/// commits on it. So they have to be parent and child.
	fn assert_stacked_commits_are_parent_and_child(&self) {
		for index in 1..self.rows.len() {
			if self.rows[index - 1].commit_lane
				!= self.rows[index].commit_lane
			{
				continue;
			}

			assert!(
				self.drawn_parent_rows(index - 1).contains(&index),
				"rows {} and {index} are drawn stacked in column \
				 {:?} but are not parent and child\n{}",
				index - 1,
				self.rows[index].commit_lane,
				self.picture()
			);
		}
	}

	/// Reading a window of rows draws the same thing as reading the
	/// whole walk and slicing it.
	///
	/// The log view only ever asks for the rows it can show, so every
	/// window is rebuilt from the delta buffer and the nearest
	/// checkpoint. Short histories check every offset; longer ones
	/// sample, which still puts a checkpoint boundary inside the
	/// window for any history long enough to have one.
	fn assert_windows_decompress_exactly(&self) {
		let total = self.ids.len();
		let stride = total.div_ceil(EXHAUSTIVE_WINDOW_ROWS).max(1);

		for length in [1, 2, 3, 7, total] {
			if length == 0 || length > total {
				continue;
			}

			let mut starts = (0..=total - length)
				.step_by(stride)
				.collect::<BTreeSet<_>>();
			starts.insert(total - length);
			for &checkpoint in self.walker.buffer.checkpoints.keys() {
				for start in
					checkpoint.saturating_sub(1)..=checkpoint + 2
				{
					if start + length <= total {
						starts.insert(start);
					}
				}
			}

			for start in starts {
				let end = start + length;
				assert_eq!(
					self.walker.buffer.decompress(start, end - 1),
					self.snapshots[start..end],
					"lane states {start}..{end} differ from the recorded walk"
				);
				let window = self.walker.compute_rows(
					&self.ids[start..end],
					start,
					&self.branch_tips,
					&self.stashes,
					self.head.as_ref(),
				);

				assert_eq!(
					window,
					self.rows[start..end],
					"rows {start}..{end} drew differently on their \
					 own\n{}",
					self.picture()
				);
			}
		}
	}

	/// A component id per row, joining each commit to the parents it
	/// draws an edge to.
	///
	/// Two children of the same unwalked parent land in one component
	/// as well: they share ancestry the window does not show, and the
	/// picture is allowed to route both through one lane.
	fn ancestry_components(&self) -> Vec<usize> {
		let mut parent_keys: HashMap<CommitId, usize> =
			HashMap::new();
		let mut union = Union::new(self.rows.len());

		for index in 0..self.rows.len() {
			for parent in self.drawn_parents(index) {
				if let Some(row) = self.row_of.get(&parent) {
					union.join(index, *row);
					continue;
				}

				let next = union.len();
				let key = *parent_keys.entry(parent).or_insert(next);
				if key == next {
					union.grow();
				}
				union.join(index, key);
			}
		}

		(0..self.rows.len()).map(|row| union.root(row)).collect()
	}
}

/// Disjoint-set forest over row indices.
struct Union(Vec<usize>);

impl Union {
	fn new(size: usize) -> Self {
		Self((0..size).collect())
	}

	fn len(&self) -> usize {
		self.0.len()
	}

	fn grow(&mut self) {
		self.0.push(self.0.len());
	}

	fn root(&mut self, mut node: usize) -> usize {
		while self.0[node] != node {
			let parent = self.0[node];
			self.0[node] = self.0[parent];
			node = self.0[node];
		}
		node
	}

	fn join(&mut self, left: usize, right: usize) {
		let (left, right) = (self.root(left), self.root(right));
		if left != right {
			self.0[left] = right;
		}
	}
}
