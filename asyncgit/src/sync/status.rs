//! sync git api for fetching a status

use crate::{
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

impl From<gix::status::index_worktree::iter::Summary>
	for StatusItemType
{
	fn from(
		summary: gix::status::index_worktree::iter::Summary,
	) -> Self {
		use gix::status::index_worktree::iter::Summary;

		match summary {
			Summary::Removed => Self::Deleted,
			Summary::Added
			| Summary::Copied
			| Summary::IntentToAdd => Self::New,
			Summary::Modified => Self::Modified,
			Summary::TypeChange => Self::Typechange,
			Summary::Renamed => Self::Renamed,
			Summary::Conflict => Self::Conflicted,
		}
	}
}

impl From<gix::diff::index::ChangeRef<'_, '_>> for StatusItemType {
	fn from(change_ref: gix::diff::index::ChangeRef) -> Self {
		use gix::diff::index::ChangeRef;

		match change_ref {
			ChangeRef::Addition { .. } => Self::New,
			ChangeRef::Deletion { .. } => Self::Deleted,
			ChangeRef::Modification { .. }
			| ChangeRef::Rewrite { .. } => Self::Modified,
		}
	}
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
	_show_untracked: Option<ShowUntrackedFilesConfig>,
) -> Result<Vec<StatusItem>> {
	scope_time!("get_status");

	// TODO: take parameter `show_untracked` into account, trying to replicate the existing
	// behaviour.

	let repo: gix::Repository =
				gix::ThreadSafeRepository::discover_with_environment_overrides(repo_path.gitpath())
						.map(Into::into)?;

	let status = repo
		.status(gix::progress::Discard)?
		.untracked_files(gix::status::UntrackedFiles::Files);

	let mut res = Vec::new();

	match status_type {
		StatusType::WorkingDir => {
			let iter = status.into_index_worktree_iter(Vec::new())?;

			for item in iter {
				let item = item?;

				let status = item.summary().map(Into::into);

				if let Some(status) = status {
					let path = item.rela_path().to_string();

					res.push(StatusItem { path, status });
				}
			}
		}
		StatusType::Stage => {
			let iter = status.into_iter(Vec::new())?;

			for item in iter {
				let item = item?;

				let path = item.location().to_string();

				match item {
					gix::status::Item::IndexWorktree(_) => {}
					gix::status::Item::TreeIndex(change_ref) => {
						res.push(StatusItem {
							path,
							status: change_ref.into(),
						});
					}
				}
			}
		}
		StatusType::Both => {
			let iter = status.into_iter(Vec::new())?;

			for item in iter {
				let item = item?;

				let path = item.location().to_string();

				let status = match item {
					gix::status::Item::IndexWorktree(item) => {
						item.summary().map(Into::into)
					}
					gix::status::Item::TreeIndex(change_ref) => {
						Some(change_ref.into())
					}
				};

				if let Some(status) = status {
					res.push(StatusItem { path, status });
				}
			}
		}
	}

	res.sort_by(|a, b| {
		Path::new(a.path.as_str()).cmp(Path::new(b.path.as_str()))
	});

	Ok(res)
}
