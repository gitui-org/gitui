use super::*;

#[test]
#[should_panic(expected = "cannot be followed to its parent")]
fn readback_rejects_a_missing_merge_junction() {
	let scene =
		Scene::walk(&[(1, &[2, 3]), (2, &[4]), (3, &[4]), (4, &[])]);
	let mut cells = scene.cells();
	cells[0][2] = '┃';
	scene.assert_picture_preserves_history(&Picture::new(&cells));
}

#[test]
#[should_panic(expected = "which share no ancestry")]
fn readback_rejects_a_join_between_independent_histories() {
	let scene =
		Scene::walk(&[(1, &[3]), (2, &[4]), (3, &[]), (4, &[])]);
	let mut cells = scene.cells();
	cells[1][0] = '┣';
	cells[1][1] = '━';
	scene.assert_picture_preserves_history(&Picture::new(&cells));
}

#[test]
#[should_panic(expected = "row 0 awaits the wrong parents")]
fn frontier_rejects_a_missing_parent_outside_the_window() {
	let mut scene = Scene::walk(&[(1, &[2])]);
	scene.snapshots[0][0] = Some(LaneSlot::Settled {
		alias: scene.alias(0),
	});
	scene.assert_frontier_matches_the_walk();
}

#[test]
#[should_panic(expected = "row 1 awaits the wrong parents")]
fn frontier_rejects_a_lane_waiting_for_a_walked_parent() {
	let mut scene = Scene::walk(&[(1, &[2]), (2, &[])]);
	scene.snapshots[1] = scene.snapshots[0].clone();
	scene.assert_frontier_matches_the_walk();
}

#[test]
#[should_panic(
	expected = "lane states 99..100 differ from the recorded walk"
)]
fn windows_reject_a_corrupted_checkpoint() {
	let mut scene = walk_generated(7, 120, Shape::Entangled);
	scene
		.walker
		.buffer
		.checkpoints
		.get_mut(&99)
		.expect("the first checkpoint")
		.clear();
	scene.assert_windows_decompress_exactly();
}

#[test]
#[should_panic(expected = "row 0 draws the wrong commit marker")]
fn rows_reject_a_merge_drawn_as_an_ordinary_commit() {
	let mut scene = Scene::walk(&[(1, &[2, 3])]);
	scene.rows[0].lanes[0] =
		Some((ConnectionType::CommitNormal, 0.into()));
	scene.assert_rows_are_well_formed();
}
