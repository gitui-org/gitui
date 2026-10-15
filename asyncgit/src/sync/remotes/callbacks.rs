use super::push::ProgressNotification;
use crate::{
	error::Result,
	sync::cred::{
		default_ssh_private_keys_from_home, BasicAuthCredential,
	},
};
use crossbeam_channel::Sender;
use git2::{Cred, Error as GitError, RemoteCallbacks};
use std::{
	path::{Path, PathBuf},
	sync::{
		atomic::{AtomicBool, AtomicUsize, Ordering},
		Arc, Mutex,
	},
};

#[derive(Default)]
struct CredentialAttempts {
	ssh_agent: AtomicBool,
	ssh_private_key_index: AtomicUsize,
	username: AtomicBool,
	user_pass_plaintext: AtomicBool,
	default: AtomicBool,
}

enum SshCredentialSource<'a> {
	Agent,
	PrivateKey(&'a Path),
}

fn existing_ssh_public_key(private_key: &Path) -> Option<PathBuf> {
	let public_key = private_key.with_extension("pub");
	public_key.is_file().then_some(public_key)
}

///
#[derive(Default, Clone)]
pub struct CallbackStats {
	pub push_rejected_msg: Option<(String, String)>,
}

///
#[derive(Clone)]
pub struct Callbacks {
	sender: Option<Sender<ProgressNotification>>,
	basic_credential: Option<BasicAuthCredential>,
	stats: Arc<Mutex<CallbackStats>>,
	credential_attempts: Arc<CredentialAttempts>,
	ssh_private_keys: Arc<Vec<PathBuf>>,
}

impl Callbacks {
	///
	pub fn new(
		sender: Option<Sender<ProgressNotification>>,
		basic_credential: Option<BasicAuthCredential>,
	) -> Self {
		Self::new_with_ssh_private_keys(
			sender,
			basic_credential,
			default_ssh_private_keys_from_home(),
		)
	}

	fn new_with_ssh_private_keys(
		sender: Option<Sender<ProgressNotification>>,
		basic_credential: Option<BasicAuthCredential>,
		ssh_private_keys: Vec<PathBuf>,
	) -> Self {
		let stats = Arc::new(Mutex::new(CallbackStats::default()));

		Self {
			sender,
			basic_credential,
			stats,
			credential_attempts: Arc::new(
				CredentialAttempts::default(),
			),
			ssh_private_keys: Arc::new(ssh_private_keys),
		}
	}

	fn next_ssh_credential_source(
		&self,
	) -> Option<SshCredentialSource<'_>> {
		if !self
			.credential_attempts
			.ssh_agent
			.swap(true, Ordering::Relaxed)
		{
			return Some(SshCredentialSource::Agent);
		}

		let index = self
			.credential_attempts
			.ssh_private_key_index
			.fetch_add(1, Ordering::Relaxed);
		self.ssh_private_keys.get(index).map(|path| {
			SshCredentialSource::PrivateKey(path.as_path())
		})
	}

	///
	pub fn get_stats(&self) -> Result<CallbackStats> {
		let stats = self.stats.lock()?;
		Ok(stats.clone())
	}

	///
	pub fn callbacks<'a>(&self) -> RemoteCallbacks<'a> {
		let mut callbacks = RemoteCallbacks::new();

		let this = self.clone();
		callbacks.push_transfer_progress(
			move |current, total, bytes| {
				this.push_transfer_progress(current, total, bytes);
			},
		);

		let this = self.clone();
		callbacks.update_tips(move |name, a, b| {
			this.update_tips(name, a, b);
			true
		});

		let this = self.clone();
		callbacks.transfer_progress(move |p| {
			this.transfer_progress(&p);
			true
		});

		let this = self.clone();
		callbacks.pack_progress(move |stage, current, total| {
			this.pack_progress(stage, total, current);
		});

		let this = self.clone();
		callbacks.push_update_reference(move |reference, msg| {
			this.push_update_reference(reference, msg);
			Ok(())
		});

		let this = self.clone();
		callbacks.credentials(
			move |url, username_from_url, allowed_types| {
				this.credentials(
					url,
					username_from_url,
					allowed_types,
				)
			},
		);

		callbacks.sideband_progress(move |data| {
			log::debug!(
				"sideband transfer: '{}'",
				String::from_utf8_lossy(data).trim()
			);
			true
		});

		callbacks
	}

	fn push_update_reference(
		&self,
		reference: &str,
		msg: Option<&str>,
	) {
		log::debug!("push_update_reference: '{reference}' {msg:?}");

		if let Ok(mut stats) = self.stats.lock() {
			stats.push_rejected_msg = msg
				.map(|msg| (reference.to_string(), msg.to_string()));
		}
	}

	fn pack_progress(
		&self,
		stage: git2::PackBuilderStage,
		total: usize,
		current: usize,
	) {
		log::debug!("packing: {stage:?} - {current}/{total}");
		self.sender.clone().map(|sender| {
			sender.send(ProgressNotification::Packing {
				stage,
				total,
				current,
			})
		});
	}

	fn transfer_progress(&self, p: &git2::Progress) {
		log::debug!(
			"transfer: {}/{}",
			p.received_objects(),
			p.total_objects()
		);
		self.sender.clone().map(|sender| {
			sender.send(ProgressNotification::Transfer {
				objects: p.received_objects(),
				total_objects: p.total_objects(),
			})
		});
	}

	fn update_tips(&self, name: &str, a: git2::Oid, b: git2::Oid) {
		log::debug!("update tips: '{name}' [{a}] [{b}]");
		self.sender.clone().map(|sender| {
			sender.send(ProgressNotification::UpdateTips {
				name: name.to_string(),
				a: a.into(),
				b: b.into(),
			})
		});
	}

	fn push_transfer_progress(
		&self,
		current: usize,
		total: usize,
		bytes: usize,
	) {
		log::debug!("progress: {current}/{total} ({bytes} B)");
		self.sender.clone().map(|sender| {
			sender.send(ProgressNotification::PushTransfer {
				current,
				total,
				bytes,
			})
		});
	}

	// If credentials are bad, we don't ask the user to re-fill their creds. We push an error and they will be able to restart their action (for example a push) and retype their creds.
	// This behavior is explained in a issue on git2-rs project : https://github.com/rust-lang/git2-rs/issues/347
	// An implementation reference is done in cargo : https://github.com/rust-lang/cargo/blob/9fb208dddb12a3081230a5fd8f470e01df8faa25/src/cargo/sources/git/utils.rs#L588
	// There is also a guide about libgit2 authentication : https://libgit2.org/docs/guides/authentication/
	fn credentials(
		&self,
		url: &str,
		username_from_url: Option<&str>,
		allowed_types: git2::CredentialType,
	) -> std::result::Result<Cred, GitError> {
		log::debug!(
			"creds: '{url}' {username_from_url:?} ({allowed_types:?})",
		);

		match &self.basic_credential {
			_ if allowed_types.is_ssh_key() => {
				let username = username_from_url
					.or_else(|| {
						self.basic_credential.as_ref().and_then(
							|credential| {
								credential.username.as_deref()
							},
						)
					})
					.unwrap_or("git");

				match self.next_ssh_credential_source() {
					Some(SshCredentialSource::Agent) => {
						Cred::ssh_key_from_agent(username)
					}
					Some(SshCredentialSource::PrivateKey(path)) => {
						let public_key =
							existing_ssh_public_key(path);
						Cred::ssh_key(
							username,
							public_key.as_deref(),
							path,
							None,
						)
					}
					None => Err(GitError::from_str(
						"No SSH credentials left to try.",
					)),
				}
			}
			Some(BasicAuthCredential {
				username: Some(user),
				password: Some(pwd),
			}) if allowed_types.is_user_pass_plaintext()
				&& !self
					.credential_attempts
					.user_pass_plaintext
					.swap(true, Ordering::Relaxed) =>
			{
				Cred::userpass_plaintext(user, pwd)
			}
			Some(BasicAuthCredential {
				username: Some(user),
				password: _,
			}) if allowed_types.is_username()
				&& !self
					.credential_attempts
					.username
					.swap(true, Ordering::Relaxed) =>
			{
				Cred::username(user)
			}
			_ if allowed_types.is_username()
				&& !self
					.credential_attempts
					.username
					.swap(true, Ordering::Relaxed) =>
			{
				Cred::username("git")
			}
			_ if allowed_types.is_default()
				&& !self
					.credential_attempts
					.default
					.swap(true, Ordering::Relaxed) =>
			{
				Cred::default()
			}
			_ => Err(GitError::from_str(
				"No supported credentials left to try.",
			)),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::{
		existing_ssh_public_key, Callbacks, SshCredentialSource,
	};
	use crate::sync::cred::BasicAuthCredential;
	use git2::CredentialType;
	use std::{error::Error, fs, path::PathBuf};
	use tempfile::tempdir;

	#[test]
	fn ssh_private_key_uses_matching_public_key_when_available(
	) -> Result<(), Box<dyn Error>> {
		let directory = tempdir()?;
		let private_key = directory.path().join("id_ed25519");
		let public_key = private_key.with_extension("pub");
		fs::write(&private_key, b"private")?;
		fs::write(&public_key, b"public")?;

		assert_eq!(
			existing_ssh_public_key(&private_key),
			Some(public_key)
		);
		Ok(())
	}

	#[test]
	fn ssh_private_key_allows_missing_public_key(
	) -> Result<(), Box<dyn Error>> {
		let directory = tempdir()?;
		let private_key = directory.path().join("id_ed25519");
		fs::write(&private_key, b"private")?;

		assert_eq!(existing_ssh_public_key(&private_key), None);
		Ok(())
	}

	#[test]
	fn ssh_credentials_try_agent_then_each_private_key_once() {
		let first = PathBuf::from("first_key");
		let second = PathBuf::from("second_key");
		let callbacks = Callbacks::new_with_ssh_private_keys(
			None,
			None,
			vec![first.clone(), second.clone()],
		);

		assert!(matches!(
			callbacks.next_ssh_credential_source(),
			Some(SshCredentialSource::Agent)
		));
		assert!(matches!(
			callbacks.next_ssh_credential_source(),
			Some(SshCredentialSource::PrivateKey(path)) if path == first
		));
		assert!(matches!(
			callbacks.next_ssh_credential_source(),
			Some(SshCredentialSource::PrivateKey(path)) if path == second
		));
		assert!(callbacks.next_ssh_credential_source().is_none());
	}

	#[test]
	fn username_and_password_are_separate_credential_attempts() {
		let callbacks = Callbacks::new_with_ssh_private_keys(
			None,
			Some(BasicAuthCredential::new(
				Some("user".to_owned()),
				Some("password".to_owned()),
			)),
			Vec::new(),
		);

		assert!(callbacks
			.credentials(
				"https://example.com",
				None,
				CredentialType::USERNAME,
			)
			.is_ok());
		assert!(callbacks
			.credentials(
				"https://example.com",
				Some("user"),
				CredentialType::USER_PASS_PLAINTEXT,
			)
			.is_ok());
		assert!(callbacks
			.credentials(
				"https://example.com",
				Some("user"),
				CredentialType::USER_PASS_PLAINTEXT,
			)
			.is_err());
	}
}
