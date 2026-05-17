//! Detect TODO/FIXME markers in commits that would be pushed.

use super::{commit_files::get_commit_diff, repository::repo, CommitId, RepoPath};
use crate::error::Result;
use git2::{BranchType, Diff, Oid, Repository};
use scopetime::scope_time;

/// Location of a TODO/FIXME marker in a commit to be pushed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PushTodoMarker {
	///
	pub commit: CommitId,
	///
	pub commit_short: String,
	///
	pub file: String,
	///
	pub line: u32,
	///
	pub kind: String,
}

///
pub fn find_push_todo_markers(
	repo_path: &RepoPath,
	branch: &str,
) -> Result<Vec<PushTodoMarker>> {
	scope_time!("find_push_todo_markers");

	let repo = repo(repo_path)?;
	let local_branch = repo.find_branch(branch, BranchType::Local)?;
	let upstream_oid = local_branch
		.upstream()
		.ok()
		.and_then(|upstream| {
			upstream
				.into_reference()
				.peel_to_commit()
				.ok()
				.map(|commit| commit.id())
		});
	let local_oid = local_branch.into_reference().peel_to_commit()?.id();

	let commit_ids = if let Some(upstream_oid) = upstream_oid {
		commits_in_range(&repo, upstream_oid, local_oid)?
	} else {
		vec![CommitId::new(local_oid)]
	};

	let mut markers = Vec::new();
	for id in commit_ids {
		let commit = repo.find_commit(id.into())?;
		let short = commit.id().to_string()[..7.min(commit.id().to_string().len())]
			.to_string();
		let diff = get_commit_diff(&repo, id, None, None, None)?;
		collect_todo_markers_from_diff(&diff, id, short, &mut markers);
	}

	Ok(markers)
}

///
pub fn format_push_todo_markers(markers: &[PushTodoMarker]) -> String {
	const LIMIT: usize = 15;
	let mut lines: Vec<String> = markers
		.iter()
		.take(LIMIT)
		.map(|m| {
			format!(
				"  {} {}:{} ({})",
				m.commit_short, m.file, m.line, m.kind
			)
		})
		.collect();
	if markers.len() > LIMIT {
		lines.push(format!(
			"  … and {} more",
			markers.len() - LIMIT
		));
	}
	lines.join("\n")
}

fn commits_in_range(
	repo: &Repository,
	upstream: Oid,
	local: Oid,
) -> Result<Vec<CommitId>> {
	let mut revwalk = repo.revwalk()?;
	revwalk.hide(upstream)?;
	revwalk.push(local)?;
	revwalk
		.map(|id| id.map(CommitId::new))
		.collect::<std::result::Result<Vec<_>, _>>()
		.map_err(Into::into)
}

fn collect_todo_markers_from_diff(
	diff: &Diff<'_>,
	commit: CommitId,
	commit_short: String,
	out: &mut Vec<PushTodoMarker>,
) {
	let _ = diff.foreach(
		&mut |_delta, _progress| true,
		None,
		None,
		Some(&mut |delta, _hunk, line| {
			if line.origin() != '+' {
				return true;
			}
			let Some(text) = std::str::from_utf8(line.content()).ok() else {
				return true;
			};
			let Some(kind) = line_contains_marker(text) else {
				return true;
			};
			let file = delta
				.new_file()
				.path()
				.and_then(|p| p.to_str())
				.unwrap_or("?")
				.to_string();
			out.push(PushTodoMarker {
				commit,
				commit_short: commit_short.clone(),
				file,
				line: line.new_lineno().unwrap_or(0),
				kind: kind.to_string(),
			});
			true
		}),
	);
}

fn line_contains_marker(line: &str) -> Option<&'static str> {
	if contains_word(line, "TODO") {
		Some("TODO")
	} else if contains_word(line, "FIXME") {
		Some("FIXME")
	} else {
		None
	}
}

fn contains_word(haystack: &str, word: &str) -> bool {
	haystack
		.split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
		.any(|part| part.eq_ignore_ascii_case(word))
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::sync::{
		tests::{repo_init, write_commit_file},
		RepoPath,
	};

	#[test]
	fn test_find_push_todo_markers() -> Result<()> {
		let (_td, repo) = repo_init()?;
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();

		write_commit_file(&repo, "todo.rs", "fn ok() {}\n", "init");
		write_commit_file(
			&repo,
			"todo.rs",
			"fn ok() { // TODO: fix }\n",
			"add todo",
		);

		let markers = find_push_todo_markers(repo_path, "master")?;
		assert_eq!(markers.len(), 1);
		assert_eq!(markers[0].kind, "TODO");
		assert_eq!(markers[0].file, "todo.rs");

		Ok(())
	}

	#[test]
	fn test_contains_word() {
		assert!(contains_word("// TODO: fix", "TODO"));
		assert!(!contains_word("// TODOLIST", "TODO"));
		assert!(contains_word("FIXME here", "FIXME"));
	}
}
