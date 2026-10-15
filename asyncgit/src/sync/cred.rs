//! credentials git helper

use super::{
	remotes::{
		get_default_remote_for_fetch_in_repo,
		get_default_remote_for_push_in_repo,
		get_default_remote_in_repo,
	},
	repository::repo,
	RepoPath,
};
use crate::error::{Error, Result};
use git2::CredentialHelper;
use std::path::{Path, PathBuf};

const DEFAULT_SSH_PRIVATE_KEY_NAMES: [&str; 3] =
	["id_ed25519", "id_ecdsa", "id_rsa"];

/// basic Authentication Credentials
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BasicAuthCredential {
	///
	pub username: Option<String>,
	///
	pub password: Option<String>,
}

impl BasicAuthCredential {
	///
	pub const fn is_complete(&self) -> bool {
		self.username.is_some() && self.password.is_some()
	}
	///
	pub const fn new(
		username: Option<String>,
		password: Option<String>,
	) -> Self {
		Self { username, password }
	}
}

/// Existing private keys from the conventional OpenSSH locations.
pub(crate) fn default_ssh_private_keys(home: &Path) -> Vec<PathBuf> {
	let ssh_dir = home.join(".ssh");
	DEFAULT_SSH_PRIVATE_KEY_NAMES
		.into_iter()
		.map(|name| ssh_dir.join(name))
		.filter(|path| path.is_file())
		.collect()
}

/// Existing private keys from the current user's home directory.
pub(crate) fn default_ssh_private_keys_from_home() -> Vec<PathBuf> {
	dirs::home_dir()
		.map_or_else(Vec::new, |home| default_ssh_private_keys(&home))
}

/// know if username and password are needed for this url
pub fn need_username_password(repo_path: &RepoPath) -> Result<bool> {
	let repo = repo(repo_path)?;
	let remote =
		repo.find_remote(&get_default_remote_in_repo(&repo)?)?;
	let url = remote
		.pushurl()
		.ok()
		.flatten()
		.or_else(|| remote.url().ok())
		.ok_or(Error::UnknownRemote)?
		.to_owned();
	let is_http = url.starts_with("http");
	Ok(is_http)
}

/// know if username and password are needed for this url
/// TODO: Very similar to `need_username_password_for_fetch`. Can be refactored. See also
/// `need_username_password`.
pub fn need_username_password_for_fetch(
	repo_path: &RepoPath,
) -> Result<bool> {
	let repo = repo(repo_path)?;
	let remote = repo
		.find_remote(&get_default_remote_for_fetch_in_repo(&repo)?)?;
	let url =
		remote.url().map_err(|_| Error::UnknownRemote)?.to_owned();
	let is_http = url.starts_with("http");
	Ok(is_http)
}

/// know if username and password are needed for this url
/// TODO: Very similar to `need_username_password_for_fetch`. Can be refactored. See also
/// `need_username_password`.
pub fn need_username_password_for_push(
	repo_path: &RepoPath,
) -> Result<bool> {
	let repo = repo(repo_path)?;
	let remote = repo
		.find_remote(&get_default_remote_for_push_in_repo(&repo)?)?;
	let url = remote
		.pushurl()
		.ok()
		.flatten()
		.or_else(|| remote.url().ok())
		.ok_or(Error::UnknownRemote)?
		.to_owned();
	let is_http = url.starts_with("http");
	Ok(is_http)
}

/// extract username and password
pub fn extract_username_password(
	repo_path: &RepoPath,
) -> Result<BasicAuthCredential> {
	let repo = repo(repo_path)?;
	let url = repo
		.find_remote(&get_default_remote_in_repo(&repo)?)?
		.url()
		.ok()
		.ok_or(Error::UnknownRemote)?
		.to_owned();
	let mut helper = CredentialHelper::new(&url);

	//TODO: look at Cred::credential_helper,
	//if the username is in the url we need to set it here,
	//I dont think `config` will pick it up

	if let Ok(config) = repo.config() {
		helper.config(&config);
	}

	Ok(match helper.execute() {
		Some((username, password)) => {
			BasicAuthCredential::new(Some(username), Some(password))
		}
		None => extract_cred_from_url(&url),
	})
}

/// extract username and password
/// TODO: Very similar to `extract_username_password_for_fetch`. Can be refactored.
pub fn extract_username_password_for_fetch(
	repo_path: &RepoPath,
) -> Result<BasicAuthCredential> {
	let repo = repo(repo_path)?;
	let url = repo
		.find_remote(&get_default_remote_for_fetch_in_repo(&repo)?)?
		.url()
		.ok()
		.ok_or(Error::UnknownRemote)?
		.to_owned();
	let mut helper = CredentialHelper::new(&url);

	//TODO: look at Cred::credential_helper,
	//if the username is in the url we need to set it here,
	//I dont think `config` will pick it up

	if let Ok(config) = repo.config() {
		helper.config(&config);
	}

	Ok(match helper.execute() {
		Some((username, password)) => {
			BasicAuthCredential::new(Some(username), Some(password))
		}
		None => extract_cred_from_url(&url),
	})
}

/// extract username and password
/// TODO: Very similar to `extract_username_password_for_fetch`. Can be refactored.
pub fn extract_username_password_for_push(
	repo_path: &RepoPath,
) -> Result<BasicAuthCredential> {
	let repo = repo(repo_path)?;
	let url = repo
		.find_remote(&get_default_remote_for_push_in_repo(&repo)?)?
		.url()
		.ok()
		.ok_or(Error::UnknownRemote)?
		.to_owned();
	let mut helper = CredentialHelper::new(&url);

	//TODO: look at Cred::credential_helper,
	//if the username is in the url we need to set it here,
	//I dont think `config` will pick it up

	if let Ok(config) = repo.config() {
		helper.config(&config);
	}

	Ok(match helper.execute() {
		Some((username, password)) => {
			BasicAuthCredential::new(Some(username), Some(password))
		}
		None => extract_cred_from_url(&url),
	})
}

/// extract credentials from url
pub fn extract_cred_from_url(url: &str) -> BasicAuthCredential {
	url::Url::parse(url).map_or_else(
		|_| BasicAuthCredential::new(None, None),
		|url| {
			BasicAuthCredential::new(
				if url.username() == "" {
					None
				} else {
					Some(url.username().to_owned())
				},
				url.password().map(std::borrow::ToOwned::to_owned),
			)
		},
	)
}

#[cfg(test)]
mod tests {
	use crate::sync::{
		cred::{
			default_ssh_private_keys, extract_cred_from_url,
			extract_username_password, need_username_password,
			BasicAuthCredential,
		},
		remotes::DEFAULT_REMOTE_NAME,
		tests::repo_init,
		RepoPath,
	};
	use serial_test::serial;
	use std::fs::File;

	#[test]
	fn test_credential_complete() {
		assert!(BasicAuthCredential::new(
			Some("username".to_owned()),
			Some("password".to_owned())
		)
		.is_complete());
	}

	#[test]
	fn test_credential_not_complete() {
		assert!(!BasicAuthCredential::new(
			None,
			Some("password".to_owned())
		)
		.is_complete());
		assert!(!BasicAuthCredential::new(
			Some("username".to_owned()),
			None
		)
		.is_complete());
		assert!(!BasicAuthCredential::new(None, None).is_complete());
	}

	#[test]
	fn test_default_ssh_private_keys_returns_existing_keys_in_preference_order(
	) {
		let home = tempfile::tempdir().unwrap();
		let ssh_dir = home.path().join(".ssh");
		std::fs::create_dir(&ssh_dir).unwrap();
		File::create(ssh_dir.join("id_rsa")).unwrap();
		File::create(ssh_dir.join("id_ecdsa")).unwrap();
		File::create(ssh_dir.join("id_ed25519")).unwrap();
		File::create(ssh_dir.join("id_ed25519.pub")).unwrap();
		File::create(ssh_dir.join("custom_key")).unwrap();

		assert_eq!(
			default_ssh_private_keys(home.path()),
			vec![
				ssh_dir.join("id_ed25519"),
				ssh_dir.join("id_ecdsa"),
				ssh_dir.join("id_rsa"),
			]
		);
	}

	#[test]
	fn test_extract_username_from_url() {
		assert_eq!(
			extract_cred_from_url("https://user@github.com"),
			BasicAuthCredential::new(Some("user".to_owned()), None)
		);
	}

	#[test]
	fn test_extract_username_password_from_url() {
		assert_eq!(
			extract_cred_from_url("https://user:pwd@github.com"),
			BasicAuthCredential::new(
				Some("user".to_owned()),
				Some("pwd".to_owned())
			)
		);
	}

	#[test]
	fn test_extract_nothing_from_url() {
		assert_eq!(
			extract_cred_from_url("https://github.com"),
			BasicAuthCredential::new(None, None)
		);
	}

	#[test]
	#[serial]
	fn test_need_username_password_if_https() {
		let (_td, repo) = repo_init().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();

		repo.remote(DEFAULT_REMOTE_NAME, "http://user@github.com")
			.unwrap();

		assert!(need_username_password(repo_path).unwrap());
	}

	#[test]
	#[serial]
	fn test_dont_need_username_password_if_ssh() {
		let (_td, repo) = repo_init().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();

		repo.remote(DEFAULT_REMOTE_NAME, "git@github.com:user/repo")
			.unwrap();

		assert!(!need_username_password(repo_path).unwrap());
	}

	#[test]
	#[serial]
	fn test_dont_need_username_password_if_pushurl_ssh() {
		let (_td, repo) = repo_init().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();

		repo.remote(DEFAULT_REMOTE_NAME, "http://user@github.com")
			.unwrap();
		repo.remote_set_pushurl(
			DEFAULT_REMOTE_NAME,
			Some("git@github.com:user/repo"),
		)
		.unwrap();

		assert!(!need_username_password(repo_path).unwrap());
	}

	#[test]
	#[serial]
	#[should_panic]
	fn test_error_if_no_remote_when_trying_to_retrieve_if_need_username_password(
	) {
		let (_td, repo) = repo_init().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();

		need_username_password(repo_path).unwrap();
	}

	#[test]
	#[serial]
	fn test_extract_username_password_from_repo() {
		let (_td, repo) = repo_init().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();

		repo.remote(
			DEFAULT_REMOTE_NAME,
			"http://user:pass@github.com",
		)
		.unwrap();

		assert_eq!(
			extract_username_password(repo_path).unwrap(),
			BasicAuthCredential::new(
				Some("user".to_owned()),
				Some("pass".to_owned())
			)
		);
	}

	#[test]
	#[serial]
	fn test_extract_username_from_repo() {
		let (_td, repo) = repo_init().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();

		repo.remote(DEFAULT_REMOTE_NAME, "http://user@github.com")
			.unwrap();

		assert_eq!(
			extract_username_password(repo_path).unwrap(),
			BasicAuthCredential::new(Some("user".to_owned()), None)
		);
	}

	#[test]
	#[serial]
	#[should_panic]
	fn test_error_if_no_remote_when_trying_to_extract_username_password(
	) {
		let (_td, repo) = repo_init().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();

		extract_username_password(repo_path).unwrap();
	}
}
