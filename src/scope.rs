use anyhow::{anyhow, Result};
use asyncgit::sync::{utils::repo_work_dir, RepoPath};
use std::{env, path::Path};

/// Restricts what gitui shows to a single directory of the repository.
///
/// The path is relative to the work dir of the repository, uses `/` as
/// separator and has no trailing one. That way it doubles as a git
/// pathspec (`sub/dir`) and as a prefix of the repo relative paths git
/// reports for files.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PathScope(Option<String>);

impl PathScope {
	/// covers the whole repository
	pub const fn everything() -> Self {
		Self(None)
	}

	/// covers the current working directory only
	///
	/// fails if the current directory is not inside the work dir of
	/// the repository (for example for a bare repository)
	pub fn current_dir(repo_path: &RepoPath) -> Result<Self> {
		let workdir = repo_work_dir(repo_path).map_err(|e| {
			anyhow!("repository has no work dir to scope to: {e}")
		})?;
		let workdir = Path::new(&workdir).canonicalize()?;
		let current_dir = env::current_dir()?.canonicalize()?;

		let relative =
			current_dir.strip_prefix(&workdir).map_err(|_| {
				anyhow!(
					"current directory `{}` is not inside the repository `{}`",
					current_dir.display(),
					workdir.display()
				)
			})?;

		Ok(Self::from_relative_path(relative))
	}

	fn from_relative_path(path: &Path) -> Self {
		let path = path
			.components()
			.map(|c| c.as_os_str().to_string_lossy())
			.collect::<Vec<_>>()
			.join("/");

		if path.is_empty() {
			Self::everything()
		} else {
			Self(Some(path))
		}
	}

	/// the scoped directory, `None` if the whole repository is shown
	pub fn path(&self) -> Option<&str> {
		self.0.as_deref()
	}

	/// pathspec for git operations that would otherwise run on the
	/// whole repository
	pub fn pathspec(&self) -> &str {
		self.0.as_deref().unwrap_or("*")
	}

	/// is the repo relative `path` inside the scope?
	///
	/// a leading `./` is accepted because that is how paths of a
	/// commit tree are reported
	pub fn contains(&self, path: &Path) -> bool {
		self.0.as_deref().is_none_or(|scope| {
			path.strip_prefix(".").unwrap_or(path).starts_with(scope)
		})
	}
}

#[cfg(test)]
mod tests {
	use super::PathScope;
	use std::path::Path;

	#[test]
	fn test_everything_contains_all() {
		let scope = PathScope::everything();

		assert_eq!(scope.path(), None);
		assert_eq!(scope.pathspec(), "*");
		assert!(scope.contains(Path::new("foo.txt")));
		assert!(scope.contains(Path::new("sub/foo.txt")));
	}

	#[test]
	fn test_scope_contains_only_children() {
		let scope =
			PathScope::from_relative_path(Path::new("sub/dir"));

		assert_eq!(scope.path(), Some("sub/dir"));
		assert_eq!(scope.pathspec(), "sub/dir");
		assert!(scope.contains(Path::new("sub/dir/foo.txt")));
		assert!(scope.contains(Path::new("./sub/dir/foo.txt")));
		assert!(!scope.contains(Path::new("./sub/foo.txt")));
		assert!(scope.contains(Path::new("sub/dir/deeper/foo.txt")));
		assert!(!scope.contains(Path::new("sub/dirt/foo.txt")));
		assert!(!scope.contains(Path::new("sub/foo.txt")));
		assert!(!scope.contains(Path::new("foo.txt")));
	}

	#[test]
	fn test_empty_relative_path_is_everything() {
		assert_eq!(
			PathScope::from_relative_path(Path::new("")),
			PathScope::everything()
		);
	}
}
