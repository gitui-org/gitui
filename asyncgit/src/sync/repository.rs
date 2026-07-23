use std::{
	cell::RefCell,
	ffi::OsStr,
	path::{Path, PathBuf},
};

use git2::{ConfigLevel, Repository, RepositoryOpenFlags};

use crate::error::Result;

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

/// Works around repo opening failing under strict snap confinement.
///
/// Strict snap confinement denies libgit2 access to `/etc/gitconfig`, and
/// every repo open fails outright with something like `'/etc/gitconfig' is
/// locked: Permission denied` (see [#2984]), because opening a repo always
/// loads the system-level git config as part of the merged config.
///
/// This redirects libgit2's system-level config search path to the snap's
/// own (read-only, always accessible) install directory, which never
/// contains a `gitconfig` file, so libgit2 simply finds no system config
/// instead of failing to reach `/etc`. It mirrors what the test suite
/// already does to avoid touching real system config files, see
/// `sandbox_config_files` in `sync::tests`.
///
/// Must be called once, as early as possible at startup, before any repo
/// is opened.
///
/// [#2984]: https://github.com/gitui-org/gitui/issues/2984
#[allow(unsafe_code)]
pub fn sanitize_snap_config_search_path() {
	if let Some(path) = snap_system_config_override(
		std::env::var_os("SNAP").as_deref(),
		std::env::var_os("SNAP_CONFINEMENT").as_deref(),
	) {
		// SAFETY: `set_search_path` mutates process-global libgit2 state
		// and must not race with concurrent config reads. This function
		// is documented to run once, before any repo is opened, so no
		// other thread is touching libgit2 config state yet.
		unsafe {
			let _ = git2::opts::set_search_path(
				ConfigLevel::System,
				path,
			);
		}
	}
}

/// Decides whether libgit2's system-level config search path needs to be
/// redirected away from `/etc` to work around strict snap confinement, and
/// if so, to what path.
///
/// Returns `None` when running unconfined, or under `classic`/`devmode`
/// confinement, both of which can reach the real filesystem, so the
/// default search path should be left untouched.
fn snap_system_config_override(
	snap: Option<&OsStr>,
	snap_confinement: Option<&OsStr>,
) -> Option<PathBuf> {
	let snap = snap?;

	// `classic` and `devmode` confinement have (near-)unrestricted
	// filesystem access, so `/etc/gitconfig` is reachable there. Only
	// `strict` confinement - or older snapd releases that predate the
	// `SNAP_CONFINEMENT` variable and default to `strict` - need the
	// workaround.
	if matches!(
		snap_confinement.and_then(OsStr::to_str),
		Some("classic" | "devmode")
	) {
		return None;
	}

	Some(PathBuf::from(snap))
}

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

#[cfg(test)]
mod tests {
	use super::snap_system_config_override;
	use std::{ffi::OsStr, path::PathBuf};

	#[test]
	fn test_snap_override_not_applied_outside_snap() {
		assert_eq!(snap_system_config_override(None, None), None);
		assert_eq!(
			snap_system_config_override(
				None,
				Some(OsStr::new("strict"))
			),
			None
		);
	}

	#[test]
	fn test_snap_override_applied_under_strict_confinement() {
		assert_eq!(
			snap_system_config_override(
				Some(OsStr::new("/snap/gitui/380")),
				Some(OsStr::new("strict"))
			),
			Some(PathBuf::from("/snap/gitui/380"))
		);
	}

	#[test]
	fn test_snap_override_applied_when_confinement_unknown() {
		// Older snapd releases don't set `SNAP_CONFINEMENT`; assume the
		// worst (strict) rather than risk failing to open the repo.
		assert_eq!(
			snap_system_config_override(
				Some(OsStr::new("/snap/gitui/380")),
				None
			),
			Some(PathBuf::from("/snap/gitui/380"))
		);
	}

	#[test]
	fn test_snap_override_not_applied_under_classic_confinement() {
		assert_eq!(
			snap_system_config_override(
				Some(OsStr::new("/snap/gitui/380")),
				Some(OsStr::new("classic"))
			),
			None
		);
	}

	#[test]
	fn test_snap_override_not_applied_under_devmode_confinement() {
		assert_eq!(
			snap_system_config_override(
				Some(OsStr::new("/snap/gitui/380")),
				Some(OsStr::new("devmode"))
			),
			None
		);
	}
}
