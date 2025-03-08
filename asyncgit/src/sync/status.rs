//! sync git api for fetching a status

use crate::{
	error::Error,
	error::Result,
	sync::{config::untracked_files_config_repo, repository::repo},
};
use git2::{Delta, Status, StatusOptions, StatusShow};
use scopetime::scope_time;
use std::path::Path;

use super::{RepoPath, ShowUntrackedFilesConfig};

///
#[derive(Copy, Clone, Hash, PartialEq, Eq, Debug)]
pub enum StatusItemType {
	///
	New,
	///
	Modified,
	///
	Deleted,
	///
	Renamed,
	///
	Typechange,
	///
	Conflicted,
}

impl From<Status> for StatusItemType {
	fn from(s: Status) -> Self {
		if s.is_index_new() || s.is_wt_new() {
			Self::New
		} else if s.is_index_deleted() || s.is_wt_deleted() {
			Self::Deleted
		} else if s.is_index_renamed() || s.is_wt_renamed() {
			Self::Renamed
		} else if s.is_index_typechange() || s.is_wt_typechange() {
			Self::Typechange
		} else if s.is_conflicted() {
			Self::Conflicted
		} else {
			Self::Modified
		}
	}
}

impl From<Delta> for StatusItemType {
	fn from(d: Delta) -> Self {
		match d {
			Delta::Added => Self::New,
			Delta::Deleted => Self::Deleted,
			Delta::Renamed => Self::Renamed,
			Delta::Typechange => Self::Typechange,
			_ => Self::Modified,
		}
	}
}

///
#[derive(Clone, Hash, PartialEq, Eq, Debug)]
pub struct StatusItem {
	///
	pub path: String,
	///
	pub status: StatusItemType,
}

///
#[derive(Copy, Clone, Hash, PartialEq, Eq, Debug)]
pub enum StatusType {
	///
	WorkingDir,
	///
	Stage,
	///
	Both,
}

impl Default for StatusType {
	fn default() -> Self {
		Self::WorkingDir
	}
}

impl From<StatusType> for StatusShow {
	fn from(s: StatusType) -> Self {
		match s {
			StatusType::WorkingDir => Self::Workdir,
			StatusType::Stage => Self::Index,
			StatusType::Both => Self::IndexAndWorkdir,
		}
	}
}

///
pub fn is_workdir_clean(
	repo_path: &RepoPath,
	show_untracked: Option<ShowUntrackedFilesConfig>,
) -> Result<bool> {
	let repo = repo(repo_path)?;

	if repo.is_bare() && !repo.is_worktree() {
		return Ok(true);
	}

	let show_untracked = if let Some(config) = show_untracked {
		config
	} else {
		untracked_files_config_repo(&repo)?
	};

	let mut options = StatusOptions::default();
	options
		.show(StatusShow::Workdir)
		.update_index(true)
		.include_untracked(show_untracked.include_untracked())
		.renames_head_to_index(true)
		.recurse_untracked_dirs(
			show_untracked.recurse_untracked_dirs(),
		);

	let statuses = repo.statuses(Some(&mut options))?;

	Ok(statuses.is_empty())
}

/// guarantees sorting
pub fn get_status(
	repo_path: &RepoPath,
	status_type: StatusType,
	show_untracked: Option<ShowUntrackedFilesConfig>,
) -> Result<Vec<StatusItem>> {
	scope_time!("get_status");

	let repo = repo(repo_path)?;

	if repo.is_bare() && !repo.is_worktree() {
		return Ok(Vec::new());
	}

	let show_untracked = if let Some(config) = show_untracked {
		config
	} else {
		untracked_files_config_repo(&repo)?
	};

	let mut options = StatusOptions::default();
	options
		.show(status_type.into())
		.update_index(true)
		.include_untracked(show_untracked.include_untracked())
		.renames_head_to_index(true)
		.recurse_untracked_dirs(
			show_untracked.recurse_untracked_dirs(),
		);

	let statuses = repo.statuses(Some(&mut options))?;

	let mut res = Vec::with_capacity(statuses.len());

	for e in statuses.iter() {
		let status: Status = e.status();

		let path = match e.head_to_index() {
			Some(diff) => diff
				.new_file()
				.path()
				.and_then(Path::to_str)
				.map(String::from)
				.ok_or_else(|| {
					Error::Generic(
						"failed to get path to diff's new file."
							.to_string(),
					)
				})?,
			None => e.path().map(String::from).ok_or_else(|| {
				Error::Generic(
					"failed to get the path to indexed file."
						.to_string(),
				)
			})?,
		};

		res.push(StatusItem {
			path,
			status: StatusItemType::from(status),
		});
	}

	res.sort_by(|a, b| {
		Path::new(a.path.as_str()).cmp(Path::new(b.path.as_str()))
	});

	Ok(res)
}

/// discard all changes in the working directory
pub fn discard_status(repo_path: &RepoPath) -> Result<bool> {
	let repo = repo(repo_path)?;
	let commit = repo.head()?.peel_to_commit()?;

	repo.reset(commit.as_object(), git2::ResetType::Hard, None)?;

	Ok(true)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::sync::tests::repo_init;
	use std::{fs::File, io::Write, path::Path};

	#[test]
	fn test_discard_status() {
		let file_path = Path::new("foo");
		let (_td, repo) = repo_init().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();

		File::create(root.join(file_path))
			.unwrap()
			.write_all(b"test\nfoo")
			.unwrap();

		let statuses =
			get_status(repo_path, StatusType::WorkingDir, None)
				.unwrap();
		assert_eq!(statuses.len(), 1);

		discard_status(repo_path).unwrap();

		let statuses =
			get_status(repo_path, StatusType::WorkingDir, None)
				.unwrap();
		assert_eq!(statuses.len(), 0);
	}
}
