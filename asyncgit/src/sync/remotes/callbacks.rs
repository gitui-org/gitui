use super::push::ProgressNotification;
use crate::{error::Result, sync::cred::BasicAuthCredential};
use crossbeam_channel::Sender;
use git2::{Cred, Error as GitError, RemoteCallbacks};
use std::sync::{
	atomic::{AtomicBool, Ordering},
	Arc, Mutex,
};

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
	first_call_to_credentials: Arc<AtomicBool>,
}

impl Callbacks {
	///
	pub fn new(
		sender: Option<Sender<ProgressNotification>>,
		basic_credential: Option<BasicAuthCredential>,
	) -> Self {
		let stats = Arc::new(Mutex::new(CallbackStats::default()));

		Self {
			sender,
			basic_credential,
			stats,
			first_call_to_credentials: Arc::new(AtomicBool::new(
				true,
			)),
		}
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
		log::debug!("push_update_reference: '{reference}' {msg:?}",);

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
		log::debug!("progress: {current}/{total} ({bytes} B)",);
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

		// This boolean is used to avoid multiple calls to credentials callback.
		if self.first_call_to_credentials.load(Ordering::Relaxed) {
			self.first_call_to_credentials
				.store(false, Ordering::Relaxed);
		} else {
			return Err(GitError::from_str("Bad credentials."));
		}

		match &self.basic_credential {
			_ if allowed_types.is_ssh_key() => username_from_url
				.map_or_else(
					|| {
						Err(GitError::from_str(
							" Couldn't extract username from url.",
						))
					},
					Cred::ssh_key_from_agent,
				),
			Some(BasicAuthCredential {
				username: Some(user),
				password: Some(pwd),
			}) if allowed_types.is_user_pass_plaintext() => {
				Cred::userpass_plaintext(user, pwd)
			}
			Some(BasicAuthCredential {
				username: Some(user),
				password: _,
			}) if allowed_types.is_username() => Cred::username(user),
			_ if allowed_types.is_default() => Cred::default(),
			_ => Err(GitError::from_str("Couldn't find credentials")),
		}
	}
}
