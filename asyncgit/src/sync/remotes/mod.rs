//!

mod callbacks;
pub(crate) mod push;
pub(crate) mod tags;

use crate::{
	error::{Error, Result},
	sync::{
		cred::BasicAuthCredential,
		remotes::push::ProgressNotification, repository::repo, utils,
	},
	ProgressPercent,
};
use crossbeam_channel::Sender;
use git2::{
	BranchType, FetchOptions, ProxyOptions, Remote, Repository,
};
use scopetime::scope_time;
use utils::bytes2string;

pub use callbacks::Callbacks;
pub use tags::tags_missing_remote;

use super::RepoPath;

/// origin
pub const DEFAULT_REMOTE_NAME: &str = "origin";

///
pub fn proxy_auto<'a>() -> ProxyOptions<'a> {
	let mut proxy = ProxyOptions::new();
	proxy.auto();
	proxy
}

///
pub fn add_remote(
	repo_path: &RepoPath,
	name: &str,
	url: &str,
) -> Result<()> {
	let repo = repo(repo_path)?;
	repo.remote(name, url)?;
	Ok(())
}

///
pub fn rename_remote(
	repo_path: &RepoPath,
	name: &str,
	new_name: &str,
) -> Result<()> {
	let repo = repo(repo_path)?;
	repo.remote_rename(name, new_name)?;
	Ok(())
}

///
pub fn update_remote_url(
	repo_path: &RepoPath,
	name: &str,
	new_url: &str,
) -> Result<()> {
	let repo = repo(repo_path)?;
	repo.remote_set_url(name, new_url)?;
	Ok(())
}

///
pub fn delete_remote(
	repo_path: &RepoPath,
	remote_name: &str,
) -> Result<()> {
	let repo = repo(repo_path)?;
	repo.remote_delete(remote_name)?;
	Ok(())
}

///
pub fn validate_remote_name(name: &str) -> bool {
	Remote::is_valid_name(name)
}

///
pub fn get_remotes(repo_path: &RepoPath) -> Result<Vec<String>> {
	scope_time!("get_remotes");

	let repo = repo(repo_path)?;
	let remotes = repo.remotes()?;
	let remotes: Vec<String> =
		remotes.iter().flatten().map(String::from).collect();

	Ok(remotes)
}

///
pub fn get_remote_url(
	repo_path: &RepoPath,
	remote_name: &str,
) -> Result<Option<String>> {
	let repo = repo(repo_path)?;
	let remote = repo.find_remote(remote_name)?.clone();
	let url = remote.url();
	if let Some(u) = url {
		return Ok(Some(u.to_string()));
	}
	Ok(None)
}

/// tries to find origin or the only remote that is defined if any
/// in case of multiple remotes and none named *origin* we fail
pub fn get_default_remote(repo_path: &RepoPath) -> Result<String> {
	let repo = repo(repo_path)?;
	get_default_remote_in_repo(&repo)
}

/// Gets the current branch the user is on.
/// Returns none if they are not on a branch
/// and Err if there was a problem finding the branch
fn get_current_branch(
	repo: &Repository,
) -> Result<Option<git2::Branch>> {
	for b in repo.branches(None)? {
		let branch = b?.0;
		if branch.is_head() {
			return Ok(Some(branch));
		}
	}
	Ok(None)
}

/// Tries to find the default repo to fetch from based on configuration.
///
/// > `branch.<name>.remote`
/// >
/// > When on branch `<name>`, it tells `git fetch` and `git push` which remote to fetch from or
/// > push to. [...] If no remote is configured, or if you are not on any branch and there is more
/// > than one remote defined in the repository, it defaults to `origin` for fetching [...].
///
/// [git-config-branch-name-remote]: https://git-scm.com/docs/git-config#Documentation/git-config.txt-branchltnamegtremote
///
/// Falls back to `get_default_remote_in_repo`.
pub fn get_default_remote_for_fetch(
	repo_path: &RepoPath,
) -> Result<String> {
	let repo = repo(repo_path)?;
	get_default_remote_for_fetch_in_repo(&repo)
}

// TODO: Very similar to `get_default_remote_for_push_in_repo`. Can probably be refactored.
pub(crate) fn get_default_remote_for_fetch_in_repo(
	repo: &Repository,
) -> Result<String> {
	scope_time!("get_default_remote_for_fetch_in_repo");

	let config = repo.config()?;

	let branch = get_current_branch(repo)?;

	if let Some(branch) = branch {
		let remote_name = bytes2string(branch.name_bytes()?)?;

		let entry_name = format!("branch.{}.remote", &remote_name);

		if let Ok(entry) = config.get_entry(&entry_name) {
			return bytes2string(entry.value_bytes());
		}
	}

	get_default_remote_in_repo(repo)
}

/// Tries to find the default repo to push to based on configuration.
///
/// > `remote.pushDefault`
/// >
/// > The remote to push to by default. Overrides `branch.<name>.remote` for all branches, and is
/// > overridden by `branch.<name>.pushRemote` for specific branches.
///
/// > `branch.<name>.remote`
/// >
/// > When on branch `<name>`, it tells `git fetch` and `git push` which remote to fetch from or
/// > push to. The remote to push to may be overridden with `remote.pushDefault` (for all
/// > branches). The remote to push to, for the current branch, may be further overridden by
/// > `branch.<name>.pushRemote`. If no remote is configured, or if you are not on any branch and
/// > there is more than one remote defined in the repository, it defaults to `origin` for fetching
/// > and `remote.pushDefault` for pushing.
///
/// [git-config-remote-push-default]: https://git-scm.com/docs/git-config#Documentation/git-config.txt-remotepushDefault
/// [git-config-branch-name-remote]: https://git-scm.com/docs/git-config#Documentation/git-config.txt-branchltnamegtremote
///
/// Falls back to `get_default_remote_in_repo`.
pub fn get_default_remote_for_push(
	repo_path: &RepoPath,
) -> Result<String> {
	let repo = repo(repo_path)?;
	get_default_remote_for_push_in_repo(&repo)
}

// TODO: Very similar to `get_default_remote_for_fetch_in_repo`. Can probably be refactored.
pub(crate) fn get_default_remote_for_push_in_repo(
	repo: &Repository,
) -> Result<String> {
	scope_time!("get_default_remote_for_push_in_repo");

	let config = repo.config()?;

	let branch = get_current_branch(repo)?;

	if let Some(branch) = branch {
		let remote_name = bytes2string(branch.name_bytes()?)?;

		let entry_name =
			format!("branch.{}.pushRemote", &remote_name);

		if let Ok(entry) = config.get_entry(&entry_name) {
			return bytes2string(entry.value_bytes());
		}

		if let Ok(entry) = config.get_entry("remote.pushDefault") {
			return bytes2string(entry.value_bytes());
		}

		let entry_name = format!("branch.{}.remote", &remote_name);

		if let Ok(entry) = config.get_entry(&entry_name) {
			return bytes2string(entry.value_bytes());
		}
	}

	get_default_remote_in_repo(repo)
}

/// see `get_default_remote`
pub(crate) fn get_default_remote_in_repo(
	repo: &Repository,
) -> Result<String> {
	scope_time!("get_default_remote_in_repo");

	let remotes = repo.remotes()?;

	// if `origin` exists return that
	let found_origin = remotes
		.iter()
		.any(|r| r.is_some_and(|r| r == DEFAULT_REMOTE_NAME));
	if found_origin {
		return Ok(DEFAULT_REMOTE_NAME.into());
	}

	//if only one remote exists pick that
	if remotes.len() == 1 {
		let first_remote = remotes
			.iter()
			.next()
			.flatten()
			.map(String::from)
			.ok_or_else(|| {
				Error::Generic("no remote found".into())
			})?;

		return Ok(first_remote);
	}

	//inconclusive
	Err(Error::NoDefaultRemoteFound)
}

///
fn fetch_from_remote(
	repo_path: &RepoPath,
	remote: &str,
	basic_credential: Option<BasicAuthCredential>,
	progress_sender: Option<Sender<ProgressNotification>>,
) -> Result<()> {
	let repo = repo(repo_path)?;

	let mut remote = repo.find_remote(remote)?;

	let mut options = FetchOptions::new();
	let callbacks = Callbacks::new(progress_sender, basic_credential);
	options.prune(git2::FetchPrune::On);
	options.proxy_options(proxy_auto());
	options.download_tags(git2::AutotagOption::All);
	options.remote_callbacks(callbacks.callbacks());
	remote.fetch(&[] as &[&str], Some(&mut options), None)?;
	// fetch tags (also removing remotely deleted ones)
	remote.fetch(
		&["refs/tags/*:refs/tags/*"],
		Some(&mut options),
		None,
	)?;

	Ok(())
}

/// updates/prunes all branches from all remotes
pub fn fetch_all(
	repo_path: &RepoPath,
	basic_credential: &Option<BasicAuthCredential>,
	progress_sender: &Option<Sender<ProgressPercent>>,
) -> Result<()> {
	scope_time!("fetch_all");

	let repo = repo(repo_path)?;
	let remotes = repo
		.remotes()?
		.iter()
		.flatten()
		.map(String::from)
		.collect::<Vec<_>>();
	let remotes_count = remotes.len();

	for (idx, remote) in remotes.into_iter().enumerate() {
		fetch_from_remote(
			repo_path,
			&remote,
			basic_credential.clone(),
			None,
		)?;

		if let Some(sender) = progress_sender {
			let progress = ProgressPercent::new(idx, remotes_count);
			sender.send(progress)?;
		}
	}

	Ok(())
}

/// fetches from upstream/remote for local `branch`
pub(crate) fn fetch(
	repo_path: &RepoPath,
	branch: &str,
	basic_credential: Option<BasicAuthCredential>,
	progress_sender: Option<Sender<ProgressNotification>>,
) -> Result<usize> {
	scope_time!("fetch");

	let repo = repo(repo_path)?;
	let branch_ref = repo
		.find_branch(branch, BranchType::Local)?
		.into_reference();
	let branch_ref = bytes2string(branch_ref.name_bytes())?;
	let remote_name = repo.branch_upstream_remote(&branch_ref)?;
	let remote_name = bytes2string(&remote_name)?;
	let mut remote = repo.find_remote(&remote_name)?;

	let mut options = FetchOptions::new();
	options.download_tags(git2::AutotagOption::All);
	let callbacks = Callbacks::new(progress_sender, basic_credential);
	options.remote_callbacks(callbacks.callbacks());
	options.proxy_options(proxy_auto());

	remote.fetch(&[branch], Some(&mut options), None)?;

	Ok(remote.stats().received_bytes())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::sync::tests::{
		debug_cmd_print, repo_clone, repo_init,
	};

	#[test]
	fn test_smoke() {
		let (remote_dir, _remote) = repo_init().unwrap();
		let remote_path = remote_dir.path().to_str().unwrap();
		let (repo_dir, _repo) = repo_clone(remote_path).unwrap();
		let repo_path: &RepoPath = &repo_dir
			.into_path()
			.as_os_str()
			.to_str()
			.unwrap()
			.into();

		let remotes = get_remotes(repo_path).unwrap();

		assert_eq!(remotes, vec![String::from("origin")]);

		fetch(repo_path, "master", None, None).unwrap();
	}

	#[test]
	fn test_default_remote() {
		let (remote_dir, _remote) = repo_init().unwrap();
		let remote_path = remote_dir.path().to_str().unwrap();
		let (repo_dir, _repo) = repo_clone(remote_path).unwrap();
		let repo_path: &RepoPath = &repo_dir
			.into_path()
			.as_os_str()
			.to_str()
			.unwrap()
			.into();

		debug_cmd_print(
			repo_path,
			&format!("git remote add second {remote_path}")[..],
		);

		let remotes = get_remotes(repo_path).unwrap();

		assert_eq!(
			remotes,
			vec![String::from("origin"), String::from("second")]
		);

		let first =
			get_default_remote_in_repo(&repo(repo_path).unwrap())
				.unwrap();
		assert_eq!(first, String::from("origin"));
	}

	#[test]
	fn test_default_remote_out_of_order() {
		let (remote_dir, _remote) = repo_init().unwrap();
		let remote_path = remote_dir.path().to_str().unwrap();
		let (repo_dir, _repo) = repo_clone(remote_path).unwrap();
		let repo_path: &RepoPath = &repo_dir
			.into_path()
			.as_os_str()
			.to_str()
			.unwrap()
			.into();

		debug_cmd_print(
			repo_path,
			"git remote rename origin alternate",
		);

		debug_cmd_print(
			repo_path,
			&format!("git remote add origin {remote_path}")[..],
		);

		//NOTE: apparently remotes are not chronolically sorted but alphabetically
		let remotes = get_remotes(repo_path).unwrap();

		assert_eq!(
			remotes,
			vec![String::from("alternate"), String::from("origin")]
		);

		let first =
			get_default_remote_in_repo(&repo(repo_path).unwrap())
				.unwrap();
		assert_eq!(first, String::from("origin"));
	}

	#[test]
	fn test_default_remote_inconclusive() {
		let (remote_dir, _remote) = repo_init().unwrap();
		let remote_path = remote_dir.path().to_str().unwrap();
		let (repo_dir, _repo) = repo_clone(remote_path).unwrap();
		let repo_path: &RepoPath = &repo_dir
			.into_path()
			.as_os_str()
			.to_str()
			.unwrap()
			.into();

		debug_cmd_print(
			repo_path,
			"git remote rename origin alternate",
		);

		debug_cmd_print(
			repo_path,
			&format!("git remote add someremote {remote_path}")[..],
		);

		let remotes = get_remotes(repo_path).unwrap();
		assert_eq!(
			remotes,
			vec![
				String::from("alternate"),
				String::from("someremote")
			]
		);

		let default_remote =
			get_default_remote_in_repo(&repo(repo_path).unwrap());

		assert!(matches!(
			default_remote,
			Err(Error::NoDefaultRemoteFound)
		));
	}

	#[test]
	fn test_default_remote_for_fetch() {
		let (remote_dir, _remote) = repo_init().unwrap();
		let remote_path = remote_dir.path().to_str().unwrap();
		let (repo_dir, repo) = repo_clone(remote_path).unwrap();
		let repo_path: &RepoPath = &repo_dir
			.into_path()
			.as_os_str()
			.to_str()
			.unwrap()
			.into();

		debug_cmd_print(
			repo_path,
			"git remote rename origin alternate",
		);

		debug_cmd_print(
			repo_path,
			&format!("git remote add someremote {remote_path}")[..],
		);

		let mut config = repo.config().unwrap();

		config
			.set_str("branch.master.remote", "branchremote")
			.unwrap();

		let default_fetch_remote =
			get_default_remote_for_fetch_in_repo(&repo);

		assert!(
			matches!(default_fetch_remote, Ok(remote_name) if remote_name == "branchremote")
		);
	}

	#[test]
	fn test_default_remote_for_push() {
		let (remote_dir, _remote) = repo_init().unwrap();
		let remote_path = remote_dir.path().to_str().unwrap();
		let (repo_dir, repo) = repo_clone(remote_path).unwrap();
		let repo_path: &RepoPath = &repo_dir
			.into_path()
			.as_os_str()
			.to_str()
			.unwrap()
			.into();

		debug_cmd_print(
			repo_path,
			"git remote rename origin alternate",
		);

		debug_cmd_print(
			repo_path,
			&format!("git remote add someremote {remote_path}")[..],
		);

		let mut config = repo.config().unwrap();

		config
			.set_str("branch.master.remote", "branchremote")
			.unwrap();

		let default_push_remote =
			get_default_remote_for_push_in_repo(&repo);

		assert!(
			matches!(default_push_remote, Ok(remote_name) if remote_name == "branchremote")
		);

		config.set_str("remote.pushDefault", "pushdefault").unwrap();

		let default_push_remote =
			get_default_remote_for_push_in_repo(&repo);

		assert!(
			matches!(default_push_remote, Ok(remote_name) if remote_name == "pushdefault")
		);

		config
			.set_str("branch.master.pushRemote", "branchpushremote")
			.unwrap();

		let default_push_remote =
			get_default_remote_for_push_in_repo(&repo);

		assert!(
			matches!(default_push_remote, Ok(remote_name) if remote_name == "branchpushremote")
		);
	}
}
