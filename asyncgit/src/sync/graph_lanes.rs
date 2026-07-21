//! Assign SourceTree-style lane glyphs to a newest-first commit list.

use super::graph_log::GraphCommit;
use super::CommitId;

/// One drawn cell in the lane column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphCell {
	/// Glyph to render.
	pub ch: char,
	/// Stable color index for this lane (cycle in the UI theme).
	pub color: usize,
}

/// Lane glyphs for a single commit row.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GraphRow {
	/// Cells left-to-right.
	pub cells: Vec<GraphCell>,
	/// Lane index of this commit's node.
	pub commit_lane: usize,
}

/// Compute lane rows for `commits` (must be newest-first, as from
/// [`super::graph_log::get_graph_commits`]).
#[must_use]
pub fn assign_lanes(commits: &[GraphCommit]) -> Vec<GraphRow> {
	let mut reserved: Vec<Option<CommitId>> = Vec::new();
	let mut colors: Vec<usize> = Vec::new();
	let mut next_color = 0_usize;
	let mut rows = Vec::with_capacity(commits.len());

	for commit in commits {
		let matching: Vec<usize> = reserved
			.iter()
			.enumerate()
			.filter_map(|(i, id)| {
				(*id == Some(commit.id)).then_some(i)
			})
			.collect();

		let commit_lane = matching.first().copied().unwrap_or_else(|| {
			open_lane(
				&mut reserved,
				&mut colors,
				&mut next_color,
				commit.id,
			)
		});

		// Ensure colors vec matches reserved length.
		while colors.len() < reserved.len() {
			colors.push(next_color);
			next_color = next_color.wrapping_add(1);
		}

		let parent_count = commit.parents.len();
		let mut cells = draw_cells(
			&reserved,
			&colors,
			commit_lane,
			&matching,
			parent_count,
		);

		// Update reserved lanes for following rows.
		for &lane in &matching {
			if let Some(slot) = reserved.get_mut(lane) {
				*slot = None;
			}
		}

		if let Some((first, rest)) = commit.parents.split_first() {
			let first_already = reserved
				.iter()
				.position(|id| *id == Some(*first));

			match first_already {
				Some(existing) if existing != commit_lane => {
					// Merge into an existing lane; free this one.
					if let Some(slot) = reserved.get_mut(commit_lane)
					{
						*slot = None;
					}
					draw_merge_connector(
						&mut cells,
						&colors,
						commit_lane,
						existing,
					);
				}
				_ => {
					if let Some(slot) = reserved.get_mut(commit_lane)
					{
						*slot = Some(*first);
					}
				}
			}

			for parent in rest {
				if reserved.iter().any(|id| *id == Some(*parent)) {
					continue;
				}
				let lane = open_lane(
					&mut reserved,
					&mut colors,
					&mut next_color,
					*parent,
				);
				draw_fork_connector(
					&mut cells,
					&colors,
					commit_lane,
					lane,
				);
			}
		}

		while reserved.last() == Some(&None) {
			reserved.pop();
			colors.pop();
		}

		while cells.last().is_some_and(|c| c.ch == ' ') {
			cells.pop();
		}

		rows.push(GraphRow {
			cells,
			commit_lane,
		});
	}

	rows
}

fn open_lane(
	reserved: &mut Vec<Option<CommitId>>,
	colors: &mut Vec<usize>,
	next_color: &mut usize,
	id: CommitId,
) -> usize {
	if let Some(free) = reserved.iter().position(Option::is_none) {
		reserved[free] = Some(id);
		free
	} else {
		let lane = reserved.len();
		reserved.push(Some(id));
		colors.push(*next_color);
		*next_color = next_color.wrapping_add(1);
		lane
	}
}

fn draw_cells(
	reserved: &[Option<CommitId>],
	colors: &[usize],
	commit_lane: usize,
	matching: &[usize],
	_parent_count: usize,
) -> Vec<GraphCell> {
	let width = reserved.len().max(commit_lane + 1);
	let mut cells = Vec::with_capacity(width * 2);

	for i in 0..width {
		let color = colors.get(i).copied().unwrap_or(0);
		let ch = if i == commit_lane {
			'●'
		} else if matching.contains(&i) {
			// Secondary merge into this commit (refined below).
			'╯'
		} else if reserved.get(i).is_some_and(Option::is_some) {
			'│'
		} else {
			' '
		};
		cells.push(GraphCell { ch, color });
		cells.push(GraphCell {
			ch: ' ',
			color: 0,
		});
	}

	// Horizontal connectors from secondary matching lanes to commit.
	for &lane in matching.iter().skip(1) {
		let color = colors.get(lane).copied().unwrap_or(0);
		fill_horizontal(&mut cells, colors, commit_lane, lane, color);
		let idx = lane * 2;
		if let Some(cell) = cells.get_mut(idx) {
			cell.ch = if lane > commit_lane { '╯' } else { '╰' };
			cell.color = color;
		}
	}

	cells
}

fn draw_merge_connector(
	cells: &mut Vec<GraphCell>,
	colors: &[usize],
	from: usize,
	to: usize,
) {
	let color = colors.get(to).copied().unwrap_or(0);
	ensure_width(cells, to.max(from) + 1);
	fill_horizontal(cells, colors, from, to, color);
}

fn draw_fork_connector(
	cells: &mut Vec<GraphCell>,
	colors: &[usize],
	from: usize,
	to: usize,
) {
	let color = colors.get(to).copied().unwrap_or(0);
	ensure_width(cells, to.max(from) + 1);
	fill_horizontal(cells, colors, from, to, color);
	let idx = to * 2;
	if let Some(cell) = cells.get_mut(idx) {
		cell.ch = if to > from { '╮' } else { '╭' };
		cell.color = color;
	}
}

fn ensure_width(cells: &mut Vec<GraphCell>, lanes: usize) {
	while cells.len() < lanes * 2 {
		cells.push(GraphCell {
			ch: ' ',
			color: 0,
		});
		cells.push(GraphCell {
			ch: ' ',
			color: 0,
		});
	}
}

fn fill_horizontal(
	cells: &mut [GraphCell],
	_colors: &[usize],
	from: usize,
	to: usize,
	color: usize,
) {
	let left = from.min(to);
	let right = from.max(to);
	for i in left..=right {
		let idx = i * 2;
		if idx >= cells.len() {
			break;
		}
		if i == from {
			continue;
		}
		if cells[idx].ch == ' ' || cells[idx].ch == '─' {
			cells[idx] = GraphCell { ch: '─', color };
			if idx + 1 < cells.len() {
				cells[idx + 1] = GraphCell { ch: '─', color };
			}
		}
	}
}

/// Render a row as a plain string (tests / debugging).
#[cfg(test)]
#[must_use]
pub fn row_to_string(row: &GraphRow) -> String {
	row.cells.iter().map(|c| c.ch).collect()
}

#[cfg(test)]
mod tests {
	use super::*;
	use git2::Oid;

	fn cid(n: u8) -> CommitId {
		let mut bytes = [0_u8; 20];
		bytes[19] = n;
		CommitId::new(Oid::from_bytes(&bytes).unwrap())
	}

	#[test]
	fn test_linear_history_single_lane() {
		let a = cid(1);
		let b = cid(2);
		let c = cid(3);
		let commits = vec![
			GraphCommit {
				id: a,
				parents: vec![b],
			},
			GraphCommit {
				id: b,
				parents: vec![c],
			},
			GraphCommit {
				id: c,
				parents: vec![],
			},
		];
		let rows = assign_lanes(&commits);
		assert_eq!(rows.len(), 3);
		assert!(row_to_string(&rows[0]).contains('●'));
		assert_eq!(rows[0].commit_lane, 0);
		assert_eq!(rows[1].commit_lane, 0);
		assert_eq!(rows[2].commit_lane, 0);
	}

	#[test]
	fn test_diverged_branches_use_two_lanes() {
		let tip_a = cid(1);
		let tip_b = cid(2);
		let base = cid(3);
		let commits = vec![
			GraphCommit {
				id: tip_a,
				parents: vec![base],
			},
			GraphCommit {
				id: tip_b,
				parents: vec![base],
			},
			GraphCommit {
				id: base,
				parents: vec![],
			},
		];
		let rows = assign_lanes(&commits);
		assert_eq!(rows.len(), 3);
		assert!(
			rows.iter().any(|r| r.commit_lane >= 1),
			"expected a second lane for diverged tips, got {:?}",
			rows.iter().map(|r| r.commit_lane).collect::<Vec<_>>()
		);
		assert!(row_to_string(&rows[2]).contains('●'));
	}
}
