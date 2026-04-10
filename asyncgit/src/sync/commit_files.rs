//! Functions for getting infos about files in commits

use super::{diff::DiffOptions, CommitId, RepoPath};
use crate::{
	error::Result,
	sync::{get_stashes, repository::repo},
	StatusItem, StatusItemType,
};
use git2::{Diff, Repository};
use scopetime::scope_time;
use std::collections::HashSet;

/// struct containing a new and an old version
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct OldNew<T> {
	/// The old version
	pub old: T,
	/// The new version
	pub new: T,
}

/// Sort two commits.
pub fn sort_commits(
	repo: &Repository,
	commits: (CommitId, CommitId),
) -> Result<OldNew<CommitId>> {
	if repo.graph_descendant_of(
		commits.0.get_oid(),
		commits.1.get_oid(),
	)? {
		Ok(OldNew {
			old: commits.1,
			new: commits.0,
		})
	} else {
		Ok(OldNew {
			old: commits.0,
			new: commits.1,
		})
	}
}

/// get all files that are part of a commit
pub fn get_commit_files(
	repo_path: &RepoPath,
	id: CommitId,
	other: Option<CommitId>,
) -> Result<Vec<StatusItem>> {
	scope_time!("get_commit_files");

	let repo = repo(repo_path)?;

	let diff = if let Some(other) = other {
		get_compare_commits_diff(
			&repo,
			sort_commits(&repo, (id, other))?,
			None,
			None,
		)?
	} else {
		get_commit_diff(
			&repo,
			id,
			None,
			None,
			Some(&get_stashes(repo_path)?.into_iter().collect()),
		)?
	};

	let res = diff
		.deltas()
		.map(|delta| {
			let status = StatusItemType::from(delta.status());

			StatusItem {
				path: delta
					.new_file()
					.path()
					.map(|p| p.to_str().unwrap_or("").to_string())
					.unwrap_or_default(),
				status,
			}
		})
		.collect::<Vec<_>>();

	Ok(res)
}

/// get diff of two arbitrary commits
#[allow(clippy::needless_pass_by_value)]
pub fn get_compare_commits_diff(
	repo: &Repository,
	ids: OldNew<CommitId>,
	pathspec: Option<String>,
	options: Option<DiffOptions>,
) -> Result<Diff<'_>> {
	// scope_time!("get_compare_commits_diff");
	let commits = OldNew {
		old: repo.find_commit(ids.old.into())?,
		new: repo.find_commit(ids.new.into())?,
	};

	let trees = OldNew {
		old: commits.old.tree()?,
		new: commits.new.tree()?,
	};

	let mut opts = git2::DiffOptions::new();
	if let Some(options) = options {
		opts.context_lines(options.context);
		opts.ignore_whitespace(options.ignore_whitespace);
		opts.interhunk_lines(options.interhunk_lines);
	}
	if let Some(p) = &pathspec {
		opts.pathspec(p.clone());
	}

	let diff: Diff<'_> = repo.diff_tree_to_tree(
		Some(&trees.old),
		Some(&trees.new),
		Some(&mut opts),
	)?;

	Ok(diff)
}

/// get diff of a commit to its first parent
pub(crate) fn get_commit_diff<'a>(
	repo: &'a Repository,
	id: CommitId,
	pathspec: Option<String>,
	options: Option<DiffOptions>,
	stashes: Option<&HashSet<CommitId>>,
) -> Result<Diff<'a>> {
	// scope_time!("get_commit_diff");

	let commit = repo.find_commit(id.into())?;
	let commit_tree = commit.tree()?;

	let parent = if commit.parent_count() > 0 {
		repo.find_commit(commit.parent_id(0)?)
			.ok()
			.and_then(|c| c.tree().ok())
	} else {
		None
	};

	let mut opts = git2::DiffOptions::new();
	if let Some(options) = options {
		opts.context_lines(options.context);
		opts.ignore_whitespace(options.ignore_whitespace);
		opts.interhunk_lines(options.interhunk_lines);
	}
	if let Some(p) = &pathspec {
		opts.pathspec(p.clone());
	}
	opts.show_binary(true);

	let mut diff = repo.diff_tree_to_tree(
		parent.as_ref(),
		Some(&commit_tree),
		Some(&mut opts),
	)?;

	if stashes.is_some_and(|stashes| stashes.contains(&id)) {
		if let Ok(untracked_commit) = commit.parent_id(2) {
			let untracked_diff = get_commit_diff(
				repo,
				CommitId::new(untracked_commit),
				pathspec,
				options,
				stashes,
			)?;

			diff.merge(&untracked_diff)?;
		}
	}

	Ok(diff)
}

#[cfg(test)]
mod tests {
	use super::get_commit_files;
	use crate::{
		error::Result,
		sync::{
			commit, stage_add_file, stash_save,
			tests::{get_statuses, repo_init},
			RepoPath,
		},
		StatusItemType,
	};
	use std::{fs::File, io::Write, path::Path};

	#[test]
	fn test_smoke() -> Result<()> {
		let file_path = Path::new("file1.txt");
		let (_td, repo) = repo_init()?;
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();

		File::create(root.join(file_path))?
			.write_all(b"test file1 content")?;

		stage_add_file(repo_path, file_path)?;

		let id = commit(repo_path, "commit msg")?;

		let diff = get_commit_files(repo_path, id, None)?;

		assert_eq!(diff.len(), 1);
		assert_eq!(diff[0].status, StatusItemType::New);

		Ok(())
	}

	#[test]
	fn test_stashed_untracked() -> Result<()> {
		let file_path = Path::new("file1.txt");
		let (_td, repo) = repo_init()?;
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();

		File::create(root.join(file_path))?
			.write_all(b"test file1 content")?;

		let id = stash_save(repo_path, None, true, false)?;

		let diff = get_commit_files(repo_path, id, None)?;

		assert_eq!(diff.len(), 1);
		assert_eq!(diff[0].status, StatusItemType::New);

		Ok(())
	}

	#[test]
	fn test_stashed_untracked_and_modified() -> Result<()> {
		let file_path1 = Path::new("file1.txt");
		let file_path2 = Path::new("file2.txt");
		let (_td, repo) = repo_init()?;
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();

		File::create(root.join(file_path1))?.write_all(b"test")?;
		stage_add_file(repo_path, file_path1)?;
		commit(repo_path, "c1")?;

		File::create(root.join(file_path1))?
			.write_all(b"modified")?;
		File::create(root.join(file_path2))?.write_all(b"new")?;

		assert_eq!(get_statuses(repo_path), (2, 0));

		let id = stash_save(repo_path, None, true, false)?;

		let diff = get_commit_files(repo_path, id, None)?;

		assert_eq!(diff.len(), 2);
		assert_eq!(diff[0].status, StatusItemType::Modified);
		assert_eq!(diff[1].status, StatusItemType::New);

		Ok(())
	}
}
