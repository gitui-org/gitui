use super::{CommitId, SharedCommitFilterFn};
use crate::error::Result;
use git2::{Repository, Revwalk, Sort};
use gix::traverse::commit::topo;
use gix::traverse::commit::Topo;

/// Visit commits in topological order, and date order where possible.
/// If a filter is provided, only commits that pass the filter are returned.
pub struct LogWalker<'a> {
	/// Revision walk engine
	walk: Revwalk<'a>,
	/// Total number of commits that have been visited
	visited_count: usize,
	/// Upper limit on buffer size
	limit: usize,
	/// The source of commits
	repo: &'a Repository,
	/// Filter on which commits will be returned when reading
	filter: Option<SharedCommitFilterFn>,
}

impl<'a> LogWalker<'a> {
	/// Create a new log walker
	/// with an upper limit on number of commits visited in one batch.
	pub fn new(repo: &'a Repository, limit: usize) -> Result<Self> {
		let mut walk = repo.revwalk()?;
		// TOPOLOGICAL + TIME guarantees parents come after children,
		// and ties/independent branches are ordered by timestamp (--date-order).
		walk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;

		// Push all references (heads, tags, remotes, etc.) into the revision walker.
		// This corresponds to running "git log --all"
		walk.push_glob("*")?;

		Ok(Self {
			walk,
			visited_count: 0,
			limit,
			repo,
			filter: None,
		})
	}

	/// Number of visited commits
	pub const fn visited(&self) -> usize {
		self.visited_count
	}

	/// Add a filter to use when reading commits
	#[must_use]
	pub fn filter(
		self,
		filter: Option<SharedCommitFilterFn>,
	) -> Self {
		Self { filter, ..self }
	}

	/// Get a batch of commits
	pub fn read(&mut self, out: &mut Vec<CommitId>) -> Result<usize> {
		let mut count = 0_usize;

		for oid_result in self.walk.by_ref() {
			let oid = oid_result?;
			let id: CommitId = oid.into();

			let commit_should_be_included =
				if let Some(ref filter) = self.filter {
					filter(self.repo, &id)?
				} else {
					true
				};

			if commit_should_be_included {
				out.push(id);
			}

			self.visited_count += 1;
			count += 1;
			if count == self.limit {
				break;
			}
		}

		Ok(count)
	}
}

/// This is separate from `LogWalker` because filtering currently (June 2024) works through
/// `SharedCommitFilterFn`.
///
/// `SharedCommitFilterFn` requires access to a `git2::repo::Repository` because, under the hood,
/// it calls into functions that work with a `git2::repo::Repository`. It seems unwise to open a
/// repo both through `gix::discover` and `Repository::open_ext` at the same time, so there is a
/// separate struct that works with `gix::Repository` only.
///
/// A more long-term option is to refactor filtering to work with a `gix::Repository` and to remove
/// `LogWalker` once this is done, but this is a larger effort.
pub struct LogWalkerWithoutFilter<'a> {
	walk: Topo<&'a gix::Repository, fn(&gix::hash::oid) -> bool>,
	limit: usize,
	visited: usize,
}

impl<'a> LogWalkerWithoutFilter<'a> {
	///
	pub fn new(
		repo: &'a mut gix::Repository,
		limit: usize,
	) -> Result<Self> {
		// This seems to be an object cache size that yields optimal performance. There’s no specific
		// reason this is 2^14, so benchmarking might reveal that there’s better values.
		repo.object_cache_size_if_unset(2_usize.pow(14));

		// Walk every local branch
		let mut tips = Vec::new();
		for ref_result in repo.references()?.local_branches()? {
			let mut reference = match ref_result {
				Ok(reference) => reference,
				Err(err) => {
					log::warn!("failed to read local branch reference: {err}");
					continue;
				}
			};

			match reference.peel_to_commit() {
				Ok(commit) => tips.push(commit.id),
				Err(err) => {
					log::warn!("failed to resolve local branch {} to a commit: {}",
						reference.name().as_bstr(),
						err,
					);
				}
			}
		}
		// .. and HEAD, in case it is detached
		match repo.head()?.try_peel_to_id() {
			Ok(Some(id)) => tips.push(id.detach()),
			Ok(None) => {}
			Err(err) => {
				log::warn!("failed to resolve HEAD: {err}");
			}
		}
		// Avoid bug in gitoxide that triggers when adding two identical
		// starting points for the walk.
		// It is valid for multiple refs to point to the same commit.
		tips.sort_unstable();
		tips.dedup();

		let walk = topo::Builder::new(&*repo)
			// Show no parents before all of its children are shown,
			// but otherwise show commits in the commit timestamp order.
			.sorting(topo::Sorting::DateOrder)
			.with_tips(tips)
			.build()?;

		Ok(Self {
			walk,
			limit,
			visited: 0,
		})
	}

	///
	pub const fn visited(&self) -> usize {
		self.visited
	}

	///
	pub fn read(&mut self, out: &mut Vec<CommitId>) -> Result<usize> {
		let mut count = 0_usize;

		while let Some(info) = self.walk.next() {
			let info = info?;
			out.push(info.id.into());

			count += 1;

			if count == self.limit {
				break;
			}
		}

		self.visited += count;

		Ok(count)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::error::Result;
	use crate::sync::commit_filter::{SearchFields, SearchOptions};
	use crate::sync::repository::gix_repo;
	use crate::sync::tests::write_commit_file;
	use crate::sync::{
		commit, get_commits_info, stage_add_file,
		tests::repo_init_empty,
	};
	use crate::sync::{
		diff_contains_file, filter_commit_by_search, LogFilterSearch,
		LogFilterSearchOptions, RepoPath,
	};
	use pretty_assertions::assert_eq;
	use std::{fs::File, io::Write, path::Path};

	#[test]
	fn test_limit() -> Result<()> {
		let file_path = Path::new("foo");
		let (_td, repo) = repo_init_empty().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();

		File::create(root.join(file_path))?.write_all(b"a")?;
		stage_add_file(repo_path, file_path).unwrap();
		commit(repo_path, "commit1").unwrap();
		File::create(root.join(file_path))?.write_all(b"a")?;
		stage_add_file(repo_path, file_path).unwrap();
		let oid2 = commit(repo_path, "commit2").unwrap();

		let mut items = Vec::new();
		let mut walk = LogWalker::new(&repo, 1)?;
		walk.read(&mut items).unwrap();

		assert_eq!(items.len(), 1);
		assert_eq!(items[0], oid2);

		Ok(())
	}

	#[test]
	fn test_logwalker() -> Result<()> {
		let file_path = Path::new("foo");
		let (_td, repo) = repo_init_empty().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();

		File::create(root.join(file_path))?.write_all(b"a")?;
		stage_add_file(repo_path, file_path).unwrap();
		commit(repo_path, "commit1").unwrap();
		File::create(root.join(file_path))?.write_all(b"a")?;
		stage_add_file(repo_path, file_path).unwrap();
		let oid2 = commit(repo_path, "commit2").unwrap();

		let mut items = Vec::new();
		let mut walk = LogWalker::new(&repo, 100)?;
		walk.read(&mut items).unwrap();

		let info = get_commits_info(repo_path, &items, 50).unwrap();
		dbg!(&info);

		assert_eq!(items.len(), 2);
		assert_eq!(items[0], oid2);

		let mut items = Vec::new();
		walk.read(&mut items).unwrap();

		assert_eq!(items.len(), 0);

		Ok(())
	}

	#[test]
	fn test_logwalker_without_filter() -> Result<()> {
		let file_path = Path::new("foo");
		let (_td, repo) = repo_init_empty().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();

		File::create(root.join(file_path))?.write_all(b"a")?;
		stage_add_file(repo_path, file_path).unwrap();
		commit(repo_path, "commit1").unwrap();
		File::create(root.join(file_path))?.write_all(b"a")?;
		stage_add_file(repo_path, file_path).unwrap();
		let oid2 = commit(repo_path, "commit2").unwrap();

		let mut repo: gix::Repository = gix_repo(repo_path)?;
		let mut walk = LogWalkerWithoutFilter::new(&mut repo, 100)?;
		let mut items = Vec::new();
		assert!(matches!(walk.read(&mut items), Ok(2)));

		let info = get_commits_info(repo_path, &items, 50).unwrap();
		dbg!(&info);

		assert_eq!(items.len(), 2);
		assert_eq!(items[0], oid2);

		let mut items = Vec::new();
		assert!(matches!(walk.read(&mut items), Ok(0)));

		assert_eq!(items.len(), 0);

		Ok(())
	}

	#[test]
	fn test_logwalker_with_filter() -> Result<()> {
		let file_path = Path::new("foo");
		let second_file_path = Path::new("baz");
		let (_td, repo) = repo_init_empty().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: RepoPath =
			root.as_os_str().to_str().unwrap().into();

		File::create(root.join(file_path))?.write_all(b"a")?;
		stage_add_file(&repo_path, file_path).unwrap();

		let _first_commit_id = commit(&repo_path, "commit1").unwrap();

		File::create(root.join(second_file_path))?.write_all(b"a")?;
		stage_add_file(&repo_path, second_file_path).unwrap();

		let second_commit_id = commit(&repo_path, "commit2").unwrap();

		File::create(root.join(file_path))?.write_all(b"b")?;
		stage_add_file(&repo_path, file_path).unwrap();

		let _third_commit_id = commit(&repo_path, "commit3").unwrap();

		let diff_contains_baz = diff_contains_file("baz".into());

		let mut items = Vec::new();
		let mut walker = LogWalker::new(&repo, 100)?
			.filter(Some(diff_contains_baz));
		walker.read(&mut items).unwrap();

		assert_eq!(items.len(), 1);
		assert_eq!(items[0], second_commit_id);

		let mut items = Vec::new();
		walker.read(&mut items).unwrap();

		assert_eq!(items.len(), 0);

		let diff_contains_bar = diff_contains_file("bar".into());

		let mut items = Vec::new();
		let mut walker = LogWalker::new(&repo, 100)?
			.filter(Some(diff_contains_bar));
		walker.read(&mut items).unwrap();

		assert_eq!(items.len(), 0);

		Ok(())
	}

	#[test]
	fn test_logwalker_with_filter_search() {
		let (_td, repo) = repo_init_empty().unwrap();

		write_commit_file(&repo, "foo", "a", "commit1");
		let second_commit_id = write_commit_file(
			&repo,
			"baz",
			"a",
			"my commit msg (#2)",
		);
		write_commit_file(&repo, "foo", "b", "commit3");

		let log_filter = filter_commit_by_search(
			LogFilterSearch::new(LogFilterSearchOptions {
				fields: SearchFields::MESSAGE_SUMMARY,
				options: SearchOptions::FUZZY_SEARCH,
				search_pattern: String::from("my msg"),
			}),
		);

		let mut items = Vec::new();
		let mut walker = LogWalker::new(&repo, 100)
			.unwrap()
			.filter(Some(log_filter));
		walker.read(&mut items).unwrap();

		assert_eq!(items.len(), 1);
		assert_eq!(items[0], second_commit_id);

		let log_filter = filter_commit_by_search(
			LogFilterSearch::new(LogFilterSearchOptions {
				fields: SearchFields::FILENAMES,
				options: SearchOptions::FUZZY_SEARCH,
				search_pattern: String::from("fo"),
			}),
		);

		let mut items = Vec::new();
		let mut walker = LogWalker::new(&repo, 100)
			.unwrap()
			.filter(Some(log_filter));
		walker.read(&mut items).unwrap();

		assert_eq!(items.len(), 2);
	}
}
