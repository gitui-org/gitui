use super::*;
use crate::graph::harness::{oid, walk_generated, Scene, Shape};

#[test]
fn linear_history() {
	Scene::walk(&[(1, &[2]), (2, &[3]), (3, &[])])
		.assert_drawing(&["o", "o", "o"]);
}

#[test]
fn history_with_multiple_roots() {
	// Two independent commit chains, each terminating in its own
	// root (a commit with no parents), keep separate lanes. A root
	// draws a marker and no line below it, so handing its column
	// to the next history would stack two commit markers in one
	// column — which is exactly how a lane draws two consecutive
	// commits, and would read as joining the two histories.
	let scene =
		Scene::walk(&[(1, &[2]), (2, &[]), (3, &[4]), (4, &[])]);
	scene.assert_drawing(&["o", "o", "  o", "  o"]);
	scene.assert_fully_settled();
}

#[test]
fn simple_merge() {
	// 1 merges 3 into the line 1 → 2 → 4, 3 → 4
	Scene::walk(&[(1, &[2, 3]), (2, &[4]), (3, &[4]), (4, &[])])
		.assert_drawing(&["M━┓", "o ┃", "┃ o", "o━┛"]);
}

#[test]
fn merge_into_tracked_lane_continues_through_corner() {
	// 3's merge line joins lane 1 which keeps flowing to 5,
	// so the corner must be a junction (┫), not a dead end (┓)
	Scene::walk(&[
		(1, &[3]),
		(2, &[5]),
		(3, &[4, 5]),
		(4, &[6]),
		(5, &[6]),
		(6, &[]),
	])
	.assert_drawing(&["o", "┃ o", "M━┫", "o ┃", "┃ o", "o━┛"]);
}

#[test]
fn merging_a_shared_second_parent_preserves_the_other_lane() {
	// Commit 2 reuses commit 1's lane for parent 4. When 4 merges
	// parent 6, its bridge must join commit 2's continuing lane.
	Scene::walk(&[
		(1, &[4]),
		(2, &[6, 4]),
		(3, &[7]),
		(4, &[5, 6]),
		(5, &[8]),
		(6, &[8]),
		(7, &[8]),
		(8, &[]),
	])
	.assert_drawing(&[
		"o",
		"┣━M",
		"┃ ┃ o",
		"M━┫ ┃",
		"o ┃ ┃",
		"┃ o ┃",
		"┃ ┃ o",
		"o━┻━┛",
	]);
}

#[test]
fn merge_bridge_crosses_unrelated_lane() {
	// 3 (lane 2) merges into 1's line (lane 0) while 2's line
	// (lane 1) passes through: the crossed lane keeps its
	// vertical instead of being cut by the bridge
	Scene::walk(&[
		(1, &[4]),
		(2, &[5]),
		(3, &[6, 4]),
		(4, &[7]),
		(5, &[7]),
		(6, &[7]),
		(7, &[]),
	])
	.assert_drawing(&[
		"o",
		"┃ o",
		"┣━┃━M",
		"o ┃ ┃",
		"┃ o ┃",
		"┃ ┃ o",
		"o━┻━┛",
	]);
}

#[test]
fn overlapping_branch_bridges_keep_inner_corner() {
	// lanes 1 and 2 both close into the commit on lane 0; the
	// outer bridge passes through the inner corner (┻) instead
	// of erasing it
	Scene::walk(&[(1, &[4]), (2, &[4]), (3, &[4]), (4, &[])])
		.assert_drawing(&["o", "┃ o", "┃ ┃ o", "o━┻━┛"]);
}

#[test]
fn nested_merges_keep_every_lane_in_its_column() {
	// Commit 3 closes a branch while opening a merge of its own.
	// The lane reserved for commit 1's second parent is already
	// open in column 1, and has to stay there: moving it along to
	// make room next to commit 3 would leave its line with
	// nothing above it in the column it moved to, and would take
	// commit 2's closing bridge with it. The merge's own lane goes
	// at the right-hand end instead, and the bridge reaches it
	// across the lanes in between.
	Scene::walk(&[
		(1, &[3, 4]),
		(2, &[3]),
		(3, &[5, 6]),
		(4, &[5]),
		(5, &[7]),
		(6, &[7]),
		(7, &[]),
	])
	.assert_drawing(&[
		"M━┓",
		"┃ ┃ o",
		"M━┃━┻━┓",
		"┃ o   ┃",
		"o━┛   ┃",
		"┃     o",
		"o━━━━━┛",
	]);
}

#[test]
fn second_parent_lane_takes_a_column_left_blank_above() {
	// Commit 4 closes the lane in column 1, so by the time commit
	// 5 merges, the row above it draws nothing there. Starting the
	// second parent's lane in that column keeps the bridge one
	// step long instead of reaching past every lane still open.
	Scene::walk(&[
		(1, &[4]),
		(2, &[4]),
		(3, &[9]),
		(4, &[5]),
		(5, &[6, 7]),
		(6, &[9]),
		(7, &[9]),
		(9, &[]),
	])
	.assert_drawing(&[
		"o",
		"┃ o",
		"┃ ┃ o",
		"o━┛ ┃",
		"M━┓ ┃",
		"o ┃ ┃",
		"┃ o ┃",
		"o━┻━┛",
	]);
}

#[test]
fn second_parent_lane_skips_a_column_this_commit_freed() {
	// Commit 4 both closes the lane in column 1 and opens a merge.
	// Column 1 is vacant by the end of the step, but the row above
	// still draws commit 2's lane there and this row draws its
	// closing corner, so the merge's second parent cannot have it
	// — taking it would put a new lane's corner where that edge
	// ends and lose the edge.
	Scene::walk(&[
		(1, &[4]),
		(2, &[4]),
		(3, &[7]),
		(4, &[5, 6]),
		(5, &[8]),
		(6, &[8]),
		(7, &[8]),
		(8, &[]),
	])
	.assert_drawing(&[
		"o",
		"┃ o",
		"┃ ┃ o",
		"M━┻━┃━┓",
		"o   ┃ ┃",
		"┃   ┃ o",
		"┃   o ┃",
		"o━━━┻━┛",
	]);
}

#[test]
fn merge_bridges_to_the_lane_its_second_parent_already_has() {
	// Commit 2 reuses the lane already heading for 5. Commit 3 must
	// likewise reuse commit 2's lane for 6, even though 2 is a merge.
	Scene::walk(&[
		(1, &[5]),
		(2, &[6, 5]),
		(3, &[7, 6]),
		(5, &[8]),
		(6, &[8]),
		(7, &[8]),
		(8, &[]),
	])
	.assert_drawing(&[
		"o",
		"┣━M",
		"┃ ┣━M",
		"o ┃ ┃",
		"┃ o ┃",
		"┃ ┃ o",
		"o━┻━┛",
	]);
}

#[test]
fn a_bridge_crossing_another_bridges_junction_keeps_it() {
	// Commit 4 does two things at once: it merges commit 6, whose
	// lane is already open in column 1, and it closes the branch
	// in column 2. The branch's bridge has to reach across column
	// 1, which is where the merge's bridge ends. That cell joins
	// up, down, left and right, and there is no way to draw it as
	// a lane with something passing behind — drawing it as a plain
	// vertical would take the merge edge out of the picture.
	Scene::walk(&[
		(1, &[4]),
		(2, &[6]),
		(3, &[4]),
		(4, &[5, 6]),
		(5, &[8]),
		(6, &[8]),
		(8, &[]),
	])
	.assert_drawing(&[
		"o",
		"┃ o",
		"┃ ┃ o",
		"M━╋━┛",
		"o ┃",
		"┃ o",
		"o━┛",
	]);
}

#[test]
fn skewed_parent_before_child_leaves_no_phantom_lane() {
	// Commit 2 is walked before commit 1, which lists it as a
	// parent. The edge up to it cannot be drawn, and must not open
	// a lane that waits for a commit the walk has gone past. The
	// marker still says merge, because commit 1 is one.
	Scene::walk(&[(2, &[3]), (1, &[2, 3]), (3, &[])])
		.assert_drawing(&["o", "┃ M", "o━┛"]);
}

#[test]
fn complete_walk_settles_all_lanes() {
	// Both parents of 1 appear before it in the walk (clock
	// skew). After a complete walk no lane may still wait on a
	// parent.
	let scene =
		Scene::walk(&[(2, &[4]), (3, &[4]), (1, &[2, 3]), (4, &[])]);
	scene.assert_drawing(&["o", "┃ o", "┃ ┃ M", "o━┛"]);
	scene.assert_fully_settled();
}

#[test]
fn duplicate_parent_entries_draw_one_edge() {
	// A commit object may name the same parent twice. There is one
	// edge to draw, and the second entry must not reserve a lane
	// for a commit that the first entry's lane already receives.
	Scene::walk(&[(1, &[2, 2]), (2, &[3]), (3, &[])])
		.assert_drawing(&["M", "o", "o"]);
}

#[test]
fn octopus_merge_draws_its_first_two_parents() {
	// Support stops at two parents. The third parent's commit
	// still appears, on a lane of its own with no edge into it,
	// which is the honest picture of an edge this graph cannot
	// show — not a lane left waiting for something.
	let scene = Scene::walk(&[
		(1, &[2, 3, 4]),
		(2, &[5]),
		(3, &[5]),
		(4, &[5]),
		(5, &[]),
	]);
	scene.assert_drawing(&["M━┓", "o ┃", "┃ o", "┃ ┃ o", "o━┻━┛"]);
	scene.assert_fully_settled();
}

#[test]
fn a_commit_naming_itself_as_a_parent_opens_no_lane() {
	let scene = Scene::walk(&[(1, &[1, 2]), (2, &[])]);
	scene.assert_drawing(&["M", "o"]);
	scene.assert_fully_settled();
}

#[test]
fn unwalked_parents_keep_their_lanes_open() {
	// The log view shows a window onto a longer history, so a lane
	// heading for a commit below the window stays open and draws
	// off the bottom of the picture.
	let scene = Scene::walk(&[(1, &[8]), (2, &[9])]);
	scene.assert_drawing(&["o", "┃ o"]);
	assert_eq!(scene.open_lane_count(), 2);
}

#[test]
fn head_lane_is_dotted_in_any_column() {
	// HEAD is not guaranteed to sit on the leftmost lane, so the
	// dotted segment has to follow the lane heading for it.
	Scene::walk_marked(
		&[(1, &[3]), (2, &[4]), (3, &[5]), (4, &[5]), (5, &[])],
		&[],
		&[],
		Some(4),
	)
	.assert_drawing(&["o", "┃ o", "o ╏", "┃ o", "o━┛"]);
}

#[test]
fn merging_head_dots_only_the_lane_heading_for_head() {
	Scene::walk_marked(
		&[
			(1, &[4]),
			(2, &[5, 4]),
			(3, &[6]),
			(4, &[6]),
			(5, &[6]),
			(6, &[]),
		],
		&[],
		&[],
		Some(4),
	)
	.assert_drawing(&[
		"o",
		"┣━M",
		"╏ ┃ o",
		"o ┃ ┃",
		"┃ o ┃",
		"o━┻━┛",
	]);
}

#[test]
fn branch_tip_and_stash_markers_leave_the_lines_alone() {
	Scene::walk_marked(
		&[(1, &[2]), (2, &[3]), (3, &[])],
		&[1],
		&[3],
		None,
	)
	.assert_drawing(&["*", "o", "*"]);
}

#[test]
fn crossed_lane_keeps_own_color() {
	let scene = Scene::walk(&[
		(1, &[4]),
		(2, &[5]),
		(3, &[6, 4]),
		(4, &[7]),
		(5, &[7]),
		(6, &[7]),
		(7, &[]),
	]);

	// row of commit 3: lane 1 is crossed by the merge bridge but
	// keeps both its vertical glyph and its own lane color
	let crossed = scene.rows[2].lanes[1]
		.expect("crossed lane should not be empty");
	assert_eq!(crossed.0, ConnectionType::Vertical);
	assert_eq!(crossed.1, lane_color(1));
}

#[test]
fn branch_bridge_crosses_unrelated_lane_keeps_own_color() {
	// 5 is the fork point for both 1 and 3
	let scene = Scene::walk(&[
		(1, &[5]),
		(2, &[6]),
		(3, &[5]),
		(5, &[8]),
		(6, &[8]),
		(8, &[]),
	]);
	scene.assert_drawing(&[
		"o",
		"┃ o",
		"┃ ┃ o",
		"o━┃━┛",
		"┃ o",
		"o━┛",
	]);

	// row of commit 5: lane 1 is crossed by the branch bridge but
	// keeps both its vertical glyph and its own lane color (its visual identity)
	let crossed = scene.rows[3].lanes[1]
		.expect("crossed lane should not be empty");
	assert_eq!(crossed.0, ConnectionType::Vertical);
	assert_eq!(crossed.1, lane_color(1));
}

#[test]
fn windows_decompress_across_checkpoint_boundaries() {
	// Long enough to take several lane-state checkpoints, which is
	// the path the log view uses for every scroll. The window
	// comparison itself is part of every scene's invariants.
	let scene = walk_generated(7, 250, Shape::MergeHeavy);
	assert!(
		scene.walker.buffer.checkpoints.len() >= 2,
		"expected the walk to take checkpoints"
	);
}

#[test]
fn a_wide_history_reports_every_column() {
	// One lane per commit, none of them ever closing. A column
	// index has to survive being reported: clamping it would
	// relabel every lane past the clamp as the same column and
	// put their edges in the wrong place.
	const LANES: usize = 300;

	let history = (1..=LANES)
		.map(|commit| (commit, vec![10_000 + commit]))
		.collect::<Vec<_>>();
	let borrowed = history
		.iter()
		.map(|(commit, parents)| (*commit, parents.as_slice()))
		.collect::<Vec<_>>();

	let scene = Scene::walk(&borrowed);
	let last = &scene.rows[LANES - 1];

	assert_eq!(last.lane_count(), LANES);
	assert_eq!(usize::from(last.commit_lane), LANES - 1);
	assert_eq!(scene.open_lane_count(), LANES);
}

#[test]
fn rows_requested_beyond_the_walk_draw_nothing() {
	let mut walker = GraphWalker::new();
	walker.process(oid(1), &[oid(2)]);

	let rows = walker.compute_rows(
		&[oid(2), oid(3)],
		1,
		&HashSet::new(),
		&HashSet::new(),
		None,
	);

	assert_eq!(rows.len(), 2);
	assert!(rows.iter().all(|row| row.lanes.is_empty()));
}

#[test]
fn generated_histories_hold_every_invariant() {
	// Scene::walk holds each of these to every invariant, so this
	// is the broad sweep behind the drawings above: the shapes
	// cover long-lived branches, short topics, disjoint roots,
	// mainlines that skip over the branches they merge, and the
	// parent lists Git permits but a naive walk mishandles.
	//
	// Long enough that each history crosses a lane-state
	// checkpoint, and that the shapes reach the states a short
	// history never gets into.
	const COMMITS: usize = 120;
	const SEEDS: u64 = 10;

	for shape in Shape::ALL {
		for seed in 0..SEEDS {
			eprintln!("graph topology: {shape:?} seed {seed}");
			walk_generated(seed, COMMITS, shape);
		}
	}
}
