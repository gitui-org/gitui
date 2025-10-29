use super::RepoPath;
use crate::{error::Result, sync::repository::repo};
use git2::RepositoryState;
use scopetime::scope_time;

///
#[derive(Debug, PartialEq, Eq)]
pub enum RepoState {
	///
	Clean,
	///
	Merge,
	///
	Rebase,
	///
	Revert,
	///
	Other,
}

impl From<RepositoryState> for RepoState {
	fn from(state: RepositoryState) -> Self {
		match state {
			RepositoryState::Clean => Self::Clean,
			RepositoryState::Merge => Self::Merge,
			RepositoryState::Revert => Self::Revert,
			RepositoryState::RebaseMerge => Self::Rebase,
			_ => {
				log::warn!("state not supported yet: {state:?}");
				Self::Other
			}
		}
	}
}

///
pub fn repo_state(repo_path: &RepoPath) -> Result<RepoState> {
	scope_time!("repo_state");

	let repo = repo(repo_path)?;

	let state = repo.state();

	Ok(state.into())
}
