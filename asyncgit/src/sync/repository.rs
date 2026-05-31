use std::{
	cell::RefCell,
	path::{Path, PathBuf},
};

use git2::{Repository, RepositoryOpenFlags};

use crate::error::Result;

#[cfg(target_env = "ohos")]
use std::sync::Once;

#[cfg(target_env = "ohos")]
static INIT_OHOS: Once = Once::new();

#[cfg(target_env = "ohos")]
pub(crate) fn init_ohos_owner_validation() {
	INIT_OHOS.call_once(|| {
		#[allow(unsafe_code)]
		unsafe {
			git2::opts::set_verify_owner_validation(false).ok();
		}
	});
}

///
pub type RepoPathRef = RefCell<RepoPath>;

///
#[derive(Clone, Debug)]
pub enum RepoPath {
	///
	Path(PathBuf),
	///
	Workdir {
		///
		gitdir: PathBuf,
		///
		workdir: PathBuf,
	},
}

impl RepoPath {
	///
	pub fn gitpath(&self) -> &Path {
		match self {
			Self::Path(p) => p.as_path(),
			Self::Workdir { gitdir, .. } => gitdir.as_path(),
		}
	}

	///
	pub fn workdir(&self) -> Option<&Path> {
		match self {
			Self::Path(_) => None,
			Self::Workdir { workdir, .. } => Some(workdir.as_path()),
		}
	}
}

impl From<PathBuf> for RepoPath {
	fn from(value: PathBuf) -> Self {
		Self::Path(value)
	}
}

impl From<&str> for RepoPath {
	fn from(p: &str) -> Self {
		Self::Path(PathBuf::from(p))
	}
}

pub fn repo(repo_path: &RepoPath) -> Result<Repository> {
	#[cfg(target_env = "ohos")]
	init_ohos_owner_validation();

	let repo = Repository::open_ext(
		repo_path.gitpath(),
		RepositoryOpenFlags::FROM_ENV,
		Vec::<&Path>::new(),
	)?;

	if let Some(workdir) = repo_path.workdir() {
		repo.set_workdir(workdir, false)?;
	}

	Ok(repo)
}

#[cfg(not(target_env = "ohos"))]
pub fn gix_repo(repo_path: &RepoPath) -> Result<gix::Repository> {
	let mut repo: gix::Repository = gix::ThreadSafeRepository::discover_with_environment_overrides(
		repo_path.gitpath(),
	)
	.map(Into::into)?;

	if let Some(workdir) = repo_path.workdir() {
		repo.set_workdir(Some(workdir.into()))?;
	}

	Ok(repo)
}
