#![deny(mismatched_lifetime_syntaxes)]

use git2::Repository;
use tempfile::TempDir;

/// initialize test repo in temp path
pub fn repo_init_empty() -> (TempDir, Repository) {
	init_log();

	sandbox_config_files();

	let td = TempDir::new().unwrap();
	let repo = Repository::init(td.path()).unwrap();
	{
		let mut config = repo.config().unwrap();
		config.set_str("user.name", "name").unwrap();
		config.set_str("user.email", "email").unwrap();
	}

	(td, repo)
}

/// initialize test repo in temp path with an empty first commit
pub fn repo_init() -> (TempDir, Repository) {
	init_log();

	sandbox_config_files();

	let td = TempDir::new().unwrap();
	let repo = Repository::init(td.path()).unwrap();
	{
		let mut config = repo.config().unwrap();
		config.set_str("user.name", "name").unwrap();
		config.set_str("user.email", "email").unwrap();

		let mut index = repo.index().unwrap();
		let id = index.write_tree().unwrap();

		let tree = repo.find_tree(id).unwrap();
		let sig = repo.signature().unwrap();
		repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
			.unwrap();
	}

	(td, repo)
}

// init log
fn init_log() {
	let _ = env_logger::builder()
		.is_test(true)
		.filter_level(log::LevelFilter::Trace)
		.try_init();
}

/// Same as `repo_init`, but the repo is a bare repo (--bare)
pub fn repo_init_bare() -> (TempDir, Repository) {
	init_log();

	let tmp_repo_dir = TempDir::new().unwrap();
	let bare_repo =
		Repository::init_bare(tmp_repo_dir.path()).unwrap();

	(tmp_repo_dir, bare_repo)
}

/// Calling `set_search_path` with an empty directory makes sure that there
/// is no git config interfering with our tests (for example user-local
/// `.gitconfig`).
#[allow(unsafe_code)]
fn sandbox_config_files() {
	use git2::{opts::set_search_path, ConfigLevel};
	use std::sync::Once;

	static INIT: Once = Once::new();

	// Adapted from https://github.com/rust-lang/cargo/pull/9035
	INIT.call_once(|| unsafe {
		let temp_dir = TempDir::new().unwrap();
		let path = temp_dir.path();

		set_search_path(ConfigLevel::System, path).unwrap();
		set_search_path(ConfigLevel::Global, path).unwrap();
		set_search_path(ConfigLevel::XDG, path).unwrap();
		set_search_path(ConfigLevel::ProgramData, path).unwrap();
	});
}
