//! Sign commit data.

use std::path::PathBuf;

/// Error type for [`SignBuilder`], used to create [`Sign`]'s
#[derive(thiserror::Error, Debug)]
pub enum SignBuilderError {
	/// The given format is invalid
	#[error("Failed to derive a commit signing method from git configuration 'gpg.format': {0}")]
	InvalidFormat(String),

	/// The GPG signing key could
	#[error("Failed to retrieve 'user.signingkey' from the git configuration: {0}")]
	GPGSigningKey(String),

	/// The SSH signing key could
	#[error("Failed to retrieve 'user.signingkey' from the git configuration: {0}")]
	SSHSigningKey(String),

	/// No signing signature could be built from the configuration data present
	#[error("Failed to build signing signature: {0}")]
	Signature(String),

	/// Failure on unimplemented signing methods
	/// to be removed once all methods have been implemented
	#[error("Select signing method '{0}' has not been implemented")]
	MethodNotImplemented(String),
}

/// Error type for [`Sign`], used to sign data
#[derive(thiserror::Error, Debug)]
pub enum SignError {
	/// Unable to spawn process
	#[error("Failed to spawn signing process: {0}")]
	Spawn(String),

	/// Unable to acquire the child process' standard input to write the commit data for signing
	#[error("Failed to acquire standard input handler")]
	Stdin,

	/// Unable to write commit data to sign to standard input of the child process
	#[error("Failed to write buffer to standard input of signing process: {0}")]
	WriteBuffer(String),

	/// Unable to retrieve the signed data from the child process
	#[error("Failed to get output of signing process call: {0}")]
	Output(String),

	/// Failure of the child process
	#[error("Failed to execute signing process: {0}")]
	Shellout(String),
}

/// Sign commit data using various methods
pub trait Sign {
	/// Sign commit with the respective implementation.
	///
	/// Retrieve an implementation using [`SignBuilder::from_gitconfig`].
	///
	/// The `commit` buffer can be created using the following steps:
	/// - create a buffer using [`git2::Repository::commit_create_buffer`]
	///
	/// The function returns a tuple of `signature` and `signature_field`.
	/// These values can then be passed into [`git2::Repository::commit_signed`].
	/// Finally, the repository head needs to be advanced to the resulting commit ID
	/// using [`git2::Reference::set_target`].
	fn sign(
		&self,
		commit: &[u8],
	) -> Result<(String, Option<String>), SignError>;

	/// only available in `#[cfg(test)]` helping to diagnose issues
	#[cfg(test)]
	fn program(&self) -> String;

	/// only available in `#[cfg(test)]` helping to diagnose issues
	#[cfg(test)]
	fn signing_key(&self) -> String;
}

/// A builder to facilitate the creation of a signing method ([`Sign`]) by examining the git configuration.
pub struct SignBuilder;

impl SignBuilder {
	/// Get a [`Sign`] from the given repository configuration to sign commit data
	///
	///
	/// ```no_run
	/// use asyncgit::sync::sign::SignBuilder;
	/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
	///
	/// /// Repo in a temporary directory for demonstration
	/// let dir = std::env::temp_dir();
	/// let repo = git2::Repository::init(dir)?;
	///
	/// /// Get the config from the repository
	/// let config = repo.config()?;
	///
	/// /// Retrieve a `Sign` implementation
	/// let sign = SignBuilder::from_gitconfig(&repo, &config)?;
	/// # Ok(())
	/// # }
	/// ```
	pub fn from_gitconfig(
		repo: &git2::Repository,
		config: &git2::Config,
	) -> Result<Box<dyn Sign>, SignBuilderError> {
		let format = config
			.get_string("gpg.format")
			.unwrap_or_else(|_| "openpgp".to_string());

		// Variants are described in the git config documentation
		// https://git-scm.com/docs/git-config#Documentation/git-config.txt-gpgformat
		match format.as_str() {
			"openpgp" => {
				// Try to retrieve the gpg program from the git configuration,
				// moving from the least to the most specific config key,
				// defaulting to "gpg" if nothing is explicitly defined (per git's implementation)
				// https://git-scm.com/docs/git-config#Documentation/git-config.txt-gpgprogram
				// https://git-scm.com/docs/git-config#Documentation/git-config.txt-gpgprogram
				let program = config
					.get_string("gpg.openpgp.program")
					.or_else(|_| config.get_string("gpg.program"))
					.unwrap_or_else(|_| "gpg".to_string());

				// Optional signing key.
				// If 'user.signingKey' is not set, we'll use 'user.name' and 'user.email'
				// to build a default signature in the format 'name <email>'.
				// https://git-scm.com/docs/git-config#Documentation/git-config.txt-usersigningKey
				let signing_key = config
					.get_string("user.signingKey")
					.or_else(
						|_| -> Result<String, SignBuilderError> {
							Ok(crate::sync::commit::signature_allow_undefined_name(repo)
                                .map_err(|err| {
                                    SignBuilderError::Signature(
                                        err.to_string(),
                                    )
                                })?
                                .to_string())
						},
					)
					.map_err(|err| {
						SignBuilderError::GPGSigningKey(
							err.to_string(),
						)
					})?;

				Ok(Box::new(GPGSign {
					program,
					signing_key,
				}))
			}
			"x509" => Err(SignBuilderError::MethodNotImplemented(
				String::from("x509"),
			)),
			"ssh" => {
				let program = config
					.get_string("gpg.ssh.program")
					.unwrap_or_else(|_| "ssh-keygen".to_string());

				let signing_key = config
					.get_string("user.signingKey")
					.map_err(|err| {
						SignBuilderError::SSHSigningKey(
							err.to_string(),
						)
					})
					.and_then(|signing_key| {
						Self::signing_key_into_path(&signing_key)
					})?;

				Ok(Box::new(SSHSign {
					program,
					signing_key,
				}))
			}
			_ => Err(SignBuilderError::InvalidFormat(format)),
		}
	}

	fn signing_key_into_path(
		signing_key: &str,
	) -> Result<PathBuf, SignBuilderError> {
		let key_path = PathBuf::from(signing_key);
		if signing_key.starts_with("ssh-") {
			use std::io::Write;
			use tempfile::NamedTempFile;
			let mut temp_file =
				NamedTempFile::new().map_err(|err| {
					SignBuilderError::SSHSigningKey(err.to_string())
				})?;
			writeln!(temp_file, "{signing_key}").map_err(|err| {
				SignBuilderError::SSHSigningKey(err.to_string())
			})?;
			let temp_file = temp_file.keep().map_err(|err| {
				SignBuilderError::SSHSigningKey(err.to_string())
			})?;
			Ok(temp_file.1)
		} else {
			Ok(key_path)
		}
	}
}

/// Sign commit data using `OpenPGP`
pub struct GPGSign {
	program: String,
	signing_key: String,
}

impl Sign for GPGSign {
	fn sign(
		&self,
		commit: &[u8],
	) -> Result<(String, Option<String>), SignError> {
		use std::io::Write;
		use std::process::{Command, Stdio};

		let mut cmd = Command::new(&self.program);
		cmd.stdin(Stdio::piped())
			.stdout(Stdio::piped())
			.stderr(Stdio::piped())
			.arg("--status-fd=2")
			.arg("-bsau")
			.arg(&self.signing_key);

		log::trace!("signing command: {cmd:?}");

		let mut child = cmd
			.spawn()
			.map_err(|e| SignError::Spawn(e.to_string()))?;

		let mut stdin = child.stdin.take().ok_or(SignError::Stdin)?;

		stdin
			.write_all(commit)
			.map_err(|e| SignError::WriteBuffer(e.to_string()))?;
		drop(stdin); // close stdin to not block indefinitely

		let output = child
			.wait_with_output()
			.map_err(|e| SignError::Output(e.to_string()))?;

		if !output.status.success() {
			return Err(SignError::Shellout(format!(
				"failed to sign data, program '{}' exited non-zero: {}",
				&self.program,
				std::str::from_utf8(&output.stderr)
					.unwrap_or("[error could not be read from stderr]")
			)));
		}

		let stderr = std::str::from_utf8(&output.stderr)
			.map_err(|e| SignError::Shellout(e.to_string()))?;

		if !stderr.contains("\n[GNUPG:] SIG_CREATED ") {
			return Err(SignError::Shellout(
				format!("failed to sign data, program '{}' failed, SIG_CREATED not seen in stderr", &self.program),
			));
		}

		let signed_commit = std::str::from_utf8(&output.stdout)
			.map_err(|e| SignError::Shellout(e.to_string()))?;

		Ok((signed_commit.to_string(), Some("gpgsig".to_string())))
	}

	#[cfg(test)]
	fn program(&self) -> String {
		self.program.clone()
	}

	#[cfg(test)]
	fn signing_key(&self) -> String {
		self.signing_key.clone()
	}
}

/// Sign commit data using `ssh-keygen`
pub struct SSHSign {
	program: String,
	signing_key: PathBuf,
}

impl Sign for SSHSign {
	fn sign(
		&self,
		commit: &[u8],
	) -> Result<(String, Option<String>), SignError> {
		use std::io::Write;
		use std::process::{Command, Stdio};

		let mut cmd = Command::new(&self.program);
		cmd.stdin(Stdio::piped())
			.stdout(Stdio::piped())
			.stderr(Stdio::piped())
			.arg("-Y")
			.arg("sign")
			.arg("-n")
			.arg("git")
			.arg("-f")
			.arg(&self.signing_key);

		if &self.program == "ssh-keygen" {
			cmd.arg("-P").arg("\"\"");
		}

		log::trace!("signing command: {cmd:?}");

		let mut child = cmd
			.spawn()
			.map_err(|e| SignError::Spawn(e.to_string()))?;

		let mut stdin = child.stdin.take().ok_or(SignError::Stdin)?;

		stdin
			.write_all(commit)
			.map_err(|e| SignError::WriteBuffer(e.to_string()))?;
		drop(stdin);

		let output = child
			.wait_with_output()
			.map_err(|e| SignError::Output(e.to_string()))?;

		let tmp_path = std::env::temp_dir();
		if self.signing_key.starts_with(tmp_path) {
			// Not handling error, as its not that bad. OS maintenance tasks will take care of it at a later point.
			let _ = std::fs::remove_file(PathBuf::from(
				&self.signing_key,
			));
		}

		if !output.status.success() {
			let error_msg = std::str::from_utf8(&output.stderr)
				.unwrap_or("[error could not be read from stderr]");
			if error_msg.contains("passphrase") {
				return Err(SignError::Shellout(String::from("Currently, we only support unencrypted pairs of ssh keys in disk or ssh-agents")));
			}
			return Err(SignError::Shellout(format!(
				"failed to sign data, program '{}' exited non-zero: {}",
				&self.program,
				error_msg
			)));
		}

		let signed_commit = std::str::from_utf8(&output.stdout)
			.map_err(|e| SignError::Shellout(e.to_string()))?;

		Ok((signed_commit.to_string(), None))
	}

	#[cfg(test)]
	fn program(&self) -> String {
		self.program.clone()
	}

	#[cfg(test)]
	fn signing_key(&self) -> String {
		format!("{}", self.signing_key.display())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::error::Result;
	use crate::sync::tests::repo_init_empty;

	#[test]
	fn test_invalid_signing_format() -> Result<()> {
		let (_temp_dir, repo) = repo_init_empty()?;

		{
			let mut config = repo.config()?;
			config.set_str("gpg.format", "INVALID_SIGNING_FORMAT")?;
		}

		let sign =
			SignBuilder::from_gitconfig(&repo, &repo.config()?);

		assert!(sign.is_err());

		Ok(())
	}

	#[test]
	fn test_program_and_signing_key_defaults() -> Result<()> {
		let (_tmp_dir, repo) = repo_init_empty()?;
		let sign =
			SignBuilder::from_gitconfig(&repo, &repo.config()?)?;

		assert_eq!("gpg", sign.program());
		assert_eq!("name <email>", sign.signing_key());

		Ok(())
	}

	#[test]
	fn test_gpg_program_configs() -> Result<()> {
		let (_tmp_dir, repo) = repo_init_empty()?;

		{
			let mut config = repo.config()?;
			config.set_str("gpg.program", "GPG_PROGRAM_TEST")?;
		}

		let sign =
			SignBuilder::from_gitconfig(&repo, &repo.config()?)?;

		// we get gpg.program, because gpg.openpgp.program is not set
		assert_eq!("GPG_PROGRAM_TEST", sign.program());

		{
			let mut config = repo.config()?;
			config.set_str(
				"gpg.openpgp.program",
				"GPG_OPENPGP_PROGRAM_TEST",
			)?;
		}

		let sign =
			SignBuilder::from_gitconfig(&repo, &repo.config()?)?;

		// since gpg.openpgp.program is now set as well, it is more specific than
		// gpg.program and therefore takes precedence
		assert_eq!("GPG_OPENPGP_PROGRAM_TEST", sign.program());

		Ok(())
	}

	#[test]
	fn test_user_signingkey() -> Result<()> {
		let (_tmp_dir, repo) = repo_init_empty()?;

		{
			let mut config = repo.config()?;
			config.set_str("user.signingKey", "FFAA")?;
		}

		let sign =
			SignBuilder::from_gitconfig(&repo, &repo.config()?)?;

		assert_eq!("FFAA", sign.signing_key());
		Ok(())
	}

	#[test]
	fn test_ssh_program_configs() -> Result<()> {
		let (_tmp_dir, repo) = repo_init_empty()?;
		let temp_file = tempfile::NamedTempFile::new()
			.expect("failed to create temp file");

		{
			let mut config = repo.config()?;
			config.set_str("gpg.format", "ssh")?;
			config.set_str(
				"user.signingKey",
				temp_file.path().to_str().unwrap(),
			)?;
		}

		let sign =
			SignBuilder::from_gitconfig(&repo, &repo.config()?)?;

		assert_eq!("ssh-keygen", sign.program());
		assert_eq!(
			temp_file.path().to_str().unwrap(),
			sign.signing_key()
		);

		Ok(())
	}

	#[test]
	fn test_ssh_keyliteral_config() -> Result<()> {
		let (_tmp_dir, repo) = repo_init_empty()?;

		{
			let mut config = repo.config()?;
			config.set_str("gpg.format", "ssh")?;
			config.set_str("user.signingKey", "ssh-ed25519 test")?;
		}

		let sign =
			SignBuilder::from_gitconfig(&repo, &repo.config()?)?;

		assert_eq!("ssh-keygen", sign.program());
		assert!(PathBuf::from(sign.signing_key()).is_file());

		Ok(())
	}

	#[test]
	fn test_ssh_external_bin_config() -> Result<()> {
		let (_tmp_dir, repo) = repo_init_empty()?;
		let temp_file = tempfile::NamedTempFile::new()
			.expect("failed to create temp file");

		{
			let mut config = repo.config()?;
			config.set_str("gpg.format", "ssh")?;
			config.set_str("gpg.ssh.program", "/opt/ssh/signer")?;
			config.set_str(
				"user.signingKey",
				temp_file.path().to_str().unwrap(),
			)?;
		}

		let sign =
			SignBuilder::from_gitconfig(&repo, &repo.config()?)?;

		assert_eq!("/opt/ssh/signer", sign.program());
		assert_eq!(
			temp_file.path().to_str().unwrap(),
			sign.signing_key()
		);

		Ok(())
	}
}
