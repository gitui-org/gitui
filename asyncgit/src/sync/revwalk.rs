use std::ops::Bound;

use crate::Result;
use git2::{Commit, Oid};

use super::{repo, CommitId, RepoPath};

/// Performs a Git revision walk.
///
/// The revwalk is optionally bounded by `start` and `end` commits, sorted according to `sort`.
/// The revwalk iterator bound by repository's lifetime is exposed through the `iter_fn`.
pub fn revwalk<R>(
	repo_path: &RepoPath,
	start: Bound<&CommitId>,
	end: Bound<&CommitId>,
	sort: git2::Sort,
	iter_fn: impl FnOnce(
		&mut (dyn Iterator<Item = Result<Oid>>),
	) -> Result<R>,
) -> Result<R> {
	let repo = repo(repo_path)?;
	let mut revwalk = repo.revwalk()?;
	revwalk.set_sorting(sort)?;
	let start = resolve(&repo, start)?;
	let end = resolve(&repo, end)?;

	if let Some(s) = start {
		revwalk.hide(s.id())?;
	}
	if let Some(e) = end {
		revwalk.push(e.id())?;
	}
	{
		#![allow(clippy::let_and_return)]
		let ret = iter_fn(&mut revwalk.map(|r| {
			r.map_err(|x| crate::Error::Generic(x.to_string()))
		}));
		ret
	}
}

fn resolve<'r>(
	repo: &'r git2::Repository,
	commit: Bound<&CommitId>,
) -> Result<Option<Commit<'r>>> {
	match commit {
		Bound::Included(c) => {
			let commit = repo.find_commit(c.get_oid())?;
			Ok(Some(commit))
		}
		Bound::Excluded(s) => {
			let commit = repo.find_commit(s.get_oid())?;
			let res = (commit.parent_count() == 1)
				.then(|| commit.parent(0))
				.transpose()?;
			Ok(res)
		}
		Bound::Unbounded => Ok(None),
	}
}
