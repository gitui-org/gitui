//! Walk commits reachable from all local/remote branches, tags, and HEAD.

use super::{
	branch::get_branches_info, tags::get_tags, CommitId, RepoPath,
};
use crate::{error::Result, sync::repository::repo};
use git2::{Commit, Oid, Repository};
use scopetime::scope_time;
use std::{
	cmp::Ordering,
	collections::{BinaryHeap, HashSet},
};

/// A commit plus its parent ids (first parent first).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphCommit {
	///
	pub id: CommitId,
	///
	pub parents: Vec<CommitId>,
}

struct TimeOrderedCommit<'a>(Commit<'a>);

impl Eq for TimeOrderedCommit<'_> {}

impl PartialEq for TimeOrderedCommit<'_> {
	fn eq(&self, other: &Self) -> bool {
		self.0.time().eq(&other.0.time())
	}
}

impl PartialOrd for TimeOrderedCommit<'_> {
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		Some(self.cmp(other))
	}
}

impl Ord for TimeOrderedCommit<'_> {
	fn cmp(&self, other: &Self) -> Ordering {
		self.0.time().cmp(&other.0.time())
	}
}

/// Collect tip commit ids from HEAD, local/remote branches, and tags.
pub fn get_graph_tips(repo_path: &RepoPath) -> Result<Vec<CommitId>> {
	scope_time!("get_graph_tips");

	let mut tips = HashSet::new();

	if let Ok(head) = super::utils::get_head(repo_path) {
		tips.insert(head);
	}

	if let Ok(local) = get_branches_info(repo_path, true) {
		for b in local {
			tips.insert(b.top_commit);
		}
	}

	if let Ok(remote) = get_branches_info(repo_path, false) {
		for b in remote {
			tips.insert(b.top_commit);
		}
	}

	if let Ok(tags) = get_tags(repo_path) {
		for id in tags.keys() {
			tips.insert(*id);
		}
	}

	Ok(tips.into_iter().collect())
}

/// Walk the commit graph from all tips, newest first, up to `limit`.
pub fn get_graph_commits(
	repo_path: &RepoPath,
	limit: usize,
) -> Result<Vec<GraphCommit>> {
	scope_time!("get_graph_commits");

	let repo = repo(repo_path)?;
	let tips = get_graph_tips(repo_path)?;
	let mut walker = GraphLogWalker::new(&repo, &tips, limit)?;
	let mut out = Vec::new();
	walker.read(&mut out)?;
	Ok(out)
}

struct GraphLogWalker<'a> {
	commits: BinaryHeap<TimeOrderedCommit<'a>>,
	visited: HashSet<Oid>,
	limit: usize,
}

impl<'a> GraphLogWalker<'a> {
	fn new(
		repo: &'a Repository,
		tips: &[CommitId],
		limit: usize,
	) -> Result<Self> {
		let mut commits = BinaryHeap::with_capacity(tips.len().max(1));
		let mut visited = HashSet::with_capacity(1000);

		for tip in tips {
			if let Ok(c) = repo.find_commit((*tip).into()) {
				if visited.insert(c.id()) {
					commits.push(TimeOrderedCommit(c));
				}
			}
		}

		// Fall back to HEAD if no tips resolved (empty repo edge cases).
		if commits.is_empty() {
			if let Ok(c) = repo.head()?.peel_to_commit() {
				if visited.insert(c.id()) {
					commits.push(TimeOrderedCommit(c));
				}
			}
		}

		Ok(Self {
			commits,
			visited,
			limit,
		})
	}

	fn read(&mut self, out: &mut Vec<GraphCommit>) -> Result<usize> {
		let mut count = 0_usize;

		while let Some(c) = self.commits.pop() {
			let parents: Vec<CommitId> = c
				.0
				.parents()
				.map(|p| {
					let id = p.id();
					if self.visited.insert(id) {
						self.commits.push(TimeOrderedCommit(p));
					}
					id.into()
				})
				.collect();

			out.push(GraphCommit {
				id: c.0.id().into(),
				parents,
			});

			count += 1;
			if count == self.limit {
				break;
			}
		}

		Ok(count)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::sync::{
		create_branch, stage_add_file, tests::repo_init_empty, commit,
	};
	use pretty_assertions::assert_eq;
	use std::{fs::File, io::Write, path::Path};

	#[test]
	fn test_graph_includes_side_branch() {
		let file_path = Path::new("foo");
		let (_td, repo) = repo_init_empty().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();

		File::create(root.join(file_path))
			.unwrap()
			.write_all(b"a")
			.unwrap();
		stage_add_file(repo_path, file_path).unwrap();
		let c1 = commit(repo_path, "base").unwrap();

		let main_ref = repo
			.head()
			.unwrap()
			.name()
			.unwrap()
			.to_string();

		create_branch(repo_path, "feature").unwrap();
		File::create(root.join(file_path))
			.unwrap()
			.write_all(b"b")
			.unwrap();
		stage_add_file(repo_path, file_path).unwrap();
		let c2 = commit(repo_path, "feature-commit").unwrap();

		repo.set_head(&main_ref).unwrap();
		repo.checkout_head(Some(
			git2::build::CheckoutBuilder::new().force(),
		))
		.unwrap();
		File::create(root.join(file_path))
			.unwrap()
			.write_all(b"c")
			.unwrap();
		stage_add_file(repo_path, file_path).unwrap();
		let c3 = commit(repo_path, "main-commit").unwrap();

		let graph = get_graph_commits(repo_path, 100).unwrap();
		let ids: HashSet<_> = graph.iter().map(|c| c.id).collect();

		assert!(ids.contains(&c1));
		assert!(ids.contains(&c2));
		assert!(ids.contains(&c3));
		assert_eq!(graph.len(), 3);
	}
}
