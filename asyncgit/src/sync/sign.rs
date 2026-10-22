//! Sign commit data.

use tempfile::NamedTempFile;

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

/// Build a signed commit object and return its [`git2::Oid`].
///
/// Creates the commit buffer, signs it with `signer` and writes the
/// signed commit. It does not move any reference, the caller is
/// responsible for advancing the relevant head or branch to the
/// returned id.
pub fn create_signed_commit(
	repo: &git2::Repository,
	signer: &dyn Sign,
	author: &git2::Signature<'_>,
	committer: &git2::Signature<'_>,
	message: &str,
	tree: &git2::Tree<'_>,
	parents: &[&git2::Commit<'_>],
) -> crate::error::Result<git2::Oid> {
	let buffer = repo.commit_create_buffer(
		author, committer, message, tree, parents,
	)?;

	let contents = std::str::from_utf8(&buffer).map_err(|_| {
		SignError::Shellout("utf8 conversion error".to_string())
	})?;

	let (signature, signature_field) = signer.sign(&buffer)?;

	Ok(repo.commit_signed(
		contents,
		&signature,
		signature_field.as_deref(),
	)?)
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
			"openpgp" | "x509" => {
				// Try to retrieve the gpg program from the git configuration,
				// moving from the least to the most specific config key,
				// defaulting to "gpg" if nothing is explicitly defined (per git's implementation)
				// https://git-scm.com/docs/git-config#Documentation/git-config.txt-gpgprogram
				let program = config
					.get_string(
						format!("gpg.{format}.program").as_str(),
					)
					.or_else(|_| config.get_string("gpg.program"))
					.unwrap_or_else(|_| {
						(if format == "x509" {
							"gpgsm"
						} else {
							"gpg"
						})
						.to_string()
					});

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
					})?;
				// `key::<literal>` is git's syntax for an inline key
				let signing_key = signing_key
					.strip_prefix("key::")
					.map(str::to_string)
					.unwrap_or(signing_key);

				// A literal public key has to be written to a temp
				// file so `ssh-keygen -f` can read it; the file must
				// outlive the signer, hence it is kept in the struct.
				let (signing_key_path, pub_key_temp_file) =
					if signing_key.starts_with("ssh-") {
						let temp_file =
							Self::write_signing_key_to_temp_file(
								&signing_key,
							)?;
						let path =
							temp_file.path().display().to_string();
						(path, Some(temp_file))
					} else {
						(signing_key, None)
					};

				Ok(Box::new(SSHSign {
					program,
					signing_key_path,
					_pub_key_temp_file: pub_key_temp_file,
				}))
			}
			_ => Err(SignBuilderError::InvalidFormat(format)),
		}
	}

	fn write_signing_key_to_temp_file(
		signing_key: &str,
	) -> Result<NamedTempFile, SignBuilderError> {
		use std::io::Write;
		let mut temp_file = NamedTempFile::new().map_err(|err| {
			SignBuilderError::SSHSigningKey(err.to_string())
		})?;
		writeln!(temp_file, "{signing_key}").map_err(|err| {
			SignBuilderError::SSHSigningKey(err.to_string())
		})?;
		Ok(temp_file)
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
				self.program,
				std::str::from_utf8(&output.stderr)
					.unwrap_or("[error could not be read from stderr]")
			)));
		}

		let stderr = std::str::from_utf8(&output.stderr)
			.map_err(|e| SignError::Shellout(e.to_string()))?;

		if !stderr.contains("\n[GNUPG:] SIG_CREATED ") {
			return Err(SignError::Shellout(
				format!("failed to sign data, program '{}' failed, SIG_CREATED not seen in stderr", self.program),
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
	signing_key_path: String,
	/// Holds the temp file backing a literal public key so it is
	/// deleted only when the signer is dropped. Never read directly,
	/// hence the underscore; `signing_key_path` points at its path.
	_pub_key_temp_file: Option<NamedTempFile>,
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
			.arg(&self.signing_key_path);

		// `-P ""` avoids a passphrase prompt without forcing agent-only signing
		// (`-U` breaks on-disk keys, see #2464). Third-party signers such as
		// 1Password's op-ssh-sign reject the flag, so pass it to ssh-keygen only.
		if &self.program == "ssh-keygen" {
			cmd.arg("-P").arg("");
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

		if !output.status.success() {
			let error_msg = std::str::from_utf8(&output.stderr)
				.unwrap_or("[error could not be read from stderr]");
			if error_msg.contains("passphrase") {
				return Err(SignError::Shellout(String::from("Currently, we only support unencrypted pairs of ssh keys in disk or ssh-agents")));
			}
			return Err(SignError::Shellout(format!(
				"failed to sign data, program '{}' exited non-zero: {}",
				self.program, error_msg
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
		self.signing_key_path.clone()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::error::Result;
	use crate::sync::tests::repo_init_empty;
	#[cfg(unix)]
	use serial_test::serial;

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
		use std::path::PathBuf;
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

	#[test]
	fn test_x509_program_defaults() -> Result<()> {
		let (_tmp_dir, repo) = repo_init_empty()?;

		{
			let mut config = repo.config()?;
			config.set_str("gpg.format", "x509")?;
		}

		let sign =
			SignBuilder::from_gitconfig(&repo, &repo.config()?)?;

		// default x509 program should be gpgsm
		assert_eq!("gpgsm", sign.program());
		// default signing key should be "name <email>" when not specified
		assert_eq!("name <email>", sign.signing_key());

		Ok(())
	}

	#[test]
	fn test_x509_program_configs() -> Result<()> {
		let (_tmp_dir, repo) = repo_init_empty()?;

		{
			let mut config = repo.config()?;
			config.set_str("gpg.format", "x509")?;
			config.set_str("gpg.program", "GPG_PROGRAM_TEST")?;
		}

		let sign =
			SignBuilder::from_gitconfig(&repo, &repo.config()?)?;

		// we get gpg.program, because gpg.x509.program is not set
		assert_eq!("GPG_PROGRAM_TEST", sign.program());

		{
			let mut config = repo.config()?;
			config.set_str(
				"gpg.x509.program",
				"GPG_X509_PROGRAM_TEST",
			)?;
		}

		let sign =
			SignBuilder::from_gitconfig(&repo, &repo.config()?)?;

		// since gpg.x509.program is now set as well, it is more specific than
		// gpg.program and therefore takes precedence
		assert_eq!("GPG_X509_PROGRAM_TEST", sign.program());

		Ok(())
	}

	/// Export a PKCS#12 bundle for `key`+`cert` with the given `cipher_args`,
	/// import it into the gpgsm keyring under `home`, and return the imported
	/// secret key's fingerprint. Returns `None` when this openssl/gpgsm pair
	/// can't round-trip the bundle, so the caller can try another cipher.
	#[cfg(unix)]
	fn gpgsm_import_p12(
		home: &std::path::Path,
		key: &std::path::Path,
		cert: &std::path::Path,
		cipher_args: &[&str],
	) -> Option<String> {
		use std::process::Command;

		let p12 = home.join("bundle.p12");
		// tolerates failure (unlike the test's `run`) so a failed cipher can
		// fall back to another.
		let run = |program: &str, args: &[&str]| {
			Command::new(program)
				.args(args)
				.env("GNUPGHOME", home)
				.output()
				.unwrap_or_else(|e| {
					panic!("failed to run {program}: {e}")
				})
		};

		let mut export_args = vec![
			"pkcs12",
			"-export",
			"-inkey",
			key.to_str().unwrap(),
			"-in",
			cert.to_str().unwrap(),
			"-out",
			p12.to_str().unwrap(),
			"-passout",
			"pass:",
		];
		export_args.extend_from_slice(cipher_args);
		if !run("openssl", &export_args).status.success() {
			return None;
		}

		run(
			"gpgsm",
			&[
				"--batch",
				"--pinentry-mode",
				"loopback",
				"--passphrase",
				"",
				"--import",
				p12.to_str().unwrap(),
			],
		);

		// a listed secret key (fpr line) means the import gave us a key.
		let listing = run(
			"gpgsm",
			&["--batch", "--with-colons", "--list-secret-keys"],
		);
		String::from_utf8_lossy(&listing.stdout)
			.lines()
			.filter_map(|line| line.strip_prefix("fpr:"))
			.find_map(|rest| {
				rest.split(':')
					.find(|field| {
						field.len() == 40
							&& field
								.bytes()
								.all(|b| b.is_ascii_hexdigit())
					})
					.map(|field| field.to_string())
			})
	}

	/// e2e x509 signing: set up a throwaway `gpgsm` identity, sign a real
	/// commit and verify it. Serial + unix-only: uses a process-wide `GNUPGHOME`.
	#[cfg(unix)]
	#[test]
	#[serial]
	fn test_x509_sign_and_verify_e2e() -> Result<()> {
		use std::os::unix::fs::PermissionsExt;
		use std::process::Command;

		// note: openssl wants `version`, not `--version`
		fn tool_available(bin: &str, version_arg: &str) -> bool {
			Command::new(bin)
				.arg(version_arg)
				.stdout(std::process::Stdio::null())
				.stderr(std::process::Stdio::null())
				.status()
				.map(|s| s.success())
				.unwrap_or(false)
		}

		assert!(
			tool_available("gpgsm", "--version"),
			"gpgsm is required for the x509 e2e test"
		);
		assert!(
			tool_available("openssl", "version"),
			"openssl is required for the x509 e2e test"
		);

		let email = "gitui-x509-test@example.com";
		let gnupg = tempfile::tempdir()?;
		let home = gnupg.path();
		std::fs::set_permissions(
			home,
			std::fs::Permissions::from_mode(0o700),
		)?;

		// pinentry that OKs everything: empty passphrase + auto-trust, no tty.
		let pinentry = home.join("fake-pinentry.sh");
		std::fs::write(
			&pinentry,
			"#!/bin/sh\necho \"OK ready\"\nwhile read -r cmd; do\n  echo OK\n  [ \"$cmd\" = BYE ] && exit 0\ndone\n",
		)?;
		std::fs::set_permissions(
			&pinentry,
			std::fs::Permissions::from_mode(0o700),
		)?;
		std::fs::write(
			home.join("gpg-agent.conf"),
			format!(
				"allow-loopback-pinentry\npinentry-program {}\n",
				pinentry.display()
			),
		)?;

		// GPGSign inherits env, so point the child gpgsm at our keyring.
		std::env::set_var("GNUPGHOME", home);

		let run = |program: &str, args: &[&str]| {
			let out = Command::new(program)
				.args(args)
				.env("GNUPGHOME", home)
				.output()
				.unwrap_or_else(|e| {
					panic!("failed to run {program}: {e}")
				});
			assert!(
				out.status.success(),
				"{program} {args:?} failed: {}",
				String::from_utf8_lossy(&out.stderr)
			);
			out
		};

		let key = home.join("key.pem");
		let cert = home.join("cert.pem");
		run(
			"openssl",
			&[
				"req",
				"-x509",
				"-newkey",
				"rsa:2048",
				"-nodes",
				"-keyout",
				key.to_str().unwrap(),
				"-out",
				cert.to_str().unwrap(),
				"-days",
				"3650",
				"-subj",
				&format!("/CN=gitui test/emailAddress={email}"),
			],
		);
		// gpgsm's PKCS#12 reader accepts different ciphers across versions, so
		// try a legacy (3DES/SHA1) then a modern (OpenSSL default) bundle and
		// fail only if neither imports a usable secret key.
		let legacy = &[
			"-keypbe",
			"PBE-SHA1-3DES",
			"-certpbe",
			"PBE-SHA1-3DES",
			"-macalg",
			"sha1",
		];
		let fingerprint = gpgsm_import_p12(home, &key, &cert, legacy)
			.or_else(|| gpgsm_import_p12(home, &key, &cert, &[]))
			.expect(
				"gpgsm could not import a legacy or modern PKCS#12 bundle",
			);

		// trust our self-signed root ("S" relaxes CA checks) so gpgsm will sign.
		std::fs::write(
			home.join("trustlist.txt"),
			format!("{fingerprint} S\n"),
		)?;
		// reload gpg-agent to read the new trustlist
		run("gpgconf", &["--kill", "gpg-agent"]);

		let (_tmp_dir, repo) = repo_init_empty()?;
		{
			let mut config = repo.config()?;
			config.set_str("gpg.format", "x509")?;
			config.set_str("user.signingKey", email)?;
		}
		let signer =
			SignBuilder::from_gitconfig(&repo, &repo.config()?)?;
		assert_eq!("gpgsm", signer.program());

		let sig = git2::Signature::now("gitui test", email)?;
		let tree = {
			let mut index = repo.index()?;
			let tree_id = index.write_tree()?;
			repo.find_tree(tree_id)?
		};
		let commit_id = create_signed_commit(
			&repo,
			&*signer,
			&sig,
			&sig,
			"x509 signed commit",
			&tree,
			&[],
		)?;

		let (signature, signed_data) =
			repo.extract_signature(&commit_id, None)?;
		let signature = std::str::from_utf8(&signature).unwrap();
		assert!(
			signature.contains("BEGIN SIGNED MESSAGE"),
			"expected an armored CMS signature, got: {signature}"
		);

		let sig_file = home.join("commit.sig");
		let data_file = home.join("commit.data");
		std::fs::write(&sig_file, signature)?;
		std::fs::write(&data_file, &*signed_data)?;
		let verify = run(
			"gpgsm",
			&[
				"--verify",
				sig_file.to_str().unwrap(),
				data_file.to_str().unwrap(),
			],
		);
		let verify_err = String::from_utf8_lossy(&verify.stderr);
		assert!(
			verify_err.contains("Good signature"),
			"gpgsm did not accept the signature: {verify_err}"
		);

		std::env::remove_var("GNUPGHOME");
		Ok(())
	}

	/// e2e openpgp signing: generate a throwaway `gpg` key, sign a real
	/// commit and verify it. Serial + unix-only: uses a process-wide `GNUPGHOME`.
	#[cfg(unix)]
	#[test]
	#[serial]
	fn test_openpgp_sign_and_verify_e2e() -> Result<()> {
		use std::os::unix::fs::PermissionsExt;
		use std::process::Command;

		fn tool_available(bin: &str) -> bool {
			Command::new(bin)
				.arg("--version")
				.stdout(std::process::Stdio::null())
				.stderr(std::process::Stdio::null())
				.status()
				.map(|s| s.success())
				.unwrap_or(false)
		}

		assert!(
			tool_available("gpg"),
			"gpg is required for the openpgp e2e test"
		);

		let email = "gitui-openpgp-test@example.com";
		let gnupg = tempfile::tempdir()?;
		let home = gnupg.path();
		std::fs::set_permissions(
			home,
			std::fs::Permissions::from_mode(0o700),
		)?;

		// GPGSign inherits env, so point the child gpg at our keyring.
		std::env::set_var("GNUPGHOME", home);

		let run = |program: &str, args: &[&str]| {
			let out = Command::new(program)
				.args(args)
				.env("GNUPGHOME", home)
				.output()
				.unwrap_or_else(|e| {
					panic!("failed to run {program}: {e}")
				});
			assert!(
				out.status.success(),
				"{program} {args:?} failed: {}",
				String::from_utf8_lossy(&out.stderr)
			);
			out
		};

		// unattended keygen: %no-protection => no passphrase, so no pinentry
		// and no agent trust dance are needed (unlike the x509/gpgsm path).
		let params = home.join("keyparams");
		std::fs::write(
			&params,
			format!(
				"%no-protection\nKey-Type: RSA\nKey-Length: 2048\nSubkey-Type: RSA\nSubkey-Length: 2048\nName-Real: gitui test\nName-Email: {email}\nExpire-Date: 0\n%commit\n"
			),
		)?;
		run(
			"gpg",
			&["--batch", "--gen-key", params.to_str().unwrap()],
		);

		let (_tmp_dir, repo) = repo_init_empty()?;
		{
			let mut config = repo.config()?;
			config.set_str("gpg.format", "openpgp")?;
			config.set_str("user.signingKey", email)?;
		}
		let signer =
			SignBuilder::from_gitconfig(&repo, &repo.config()?)?;
		assert_eq!("gpg", signer.program());

		let sig = git2::Signature::now("gitui test", email)?;
		let tree = {
			let mut index = repo.index()?;
			let tree_id = index.write_tree()?;
			repo.find_tree(tree_id)?
		};
		let commit_id = create_signed_commit(
			&repo,
			&*signer,
			&sig,
			&sig,
			"openpgp signed commit",
			&tree,
			&[],
		)?;

		let (signature, signed_data) =
			repo.extract_signature(&commit_id, None)?;
		let signature = std::str::from_utf8(&signature).unwrap();
		assert!(
			signature.contains("BEGIN PGP SIGNATURE"),
			"expected an armored OpenPGP signature, got: {signature}"
		);

		let sig_file = home.join("commit.sig");
		let data_file = home.join("commit.data");
		std::fs::write(&sig_file, signature)?;
		std::fs::write(&data_file, &*signed_data)?;
		let verify = run(
			"gpg",
			&[
				"--verify",
				sig_file.to_str().unwrap(),
				data_file.to_str().unwrap(),
			],
		);
		let verify_err = String::from_utf8_lossy(&verify.stderr);
		assert!(
			verify_err.contains("Good signature"),
			"gpg did not accept the signature: {verify_err}"
		);

		std::env::remove_var("GNUPGHOME");
		Ok(())
	}

	/// e2e ssh signing: generate a throwaway unencrypted ssh key on disk, sign a
	/// real commit with no agent reachable and verify it. Reproduces the hang
	/// reported in PR #2464: `-U` forced ssh-keygen to sign only with an
	/// agent-held key, so an on-disk key failed ("Couldn't get agent socket") or
	/// hung when no matching key was loaded. Serial + unix-only: it mutates the
	/// process-wide `SSH_AUTH_SOCK`.
	#[cfg(unix)]
	#[test]
	#[serial]
	fn test_ssh_sign_and_verify_e2e() -> Result<()> {
		use std::process::{Command, Stdio};

		// ssh-keygen exits non-zero when invoked with no args, so availability
		// is "did it spawn at all", not "did it succeed".
		let ssh_keygen_available = Command::new("ssh-keygen")
			.stdin(Stdio::null())
			.stdout(Stdio::null())
			.stderr(Stdio::null())
			.status()
			.is_ok();
		assert!(
			ssh_keygen_available,
			"ssh-keygen is required for the ssh e2e test"
		);

		let run = |program: &str, args: &[&str]| {
			let out = Command::new(program)
				.args(args)
				.output()
				.unwrap_or_else(|e| {
					panic!("failed to run {program}: {e}")
				});
			assert!(
				out.status.success(),
				"{program} {args:?} failed: {}",
				String::from_utf8_lossy(&out.stderr)
			);
			out
		};

		let email = "gitui-ssh-test@example.com";
		let dir = tempfile::tempdir()?;
		let key_path = dir.path().join("id_ed25519");
		let pub_path = dir.path().join("id_ed25519.pub");

		// unencrypted key (-N "") so no passphrase and no agent are involved.
		run(
			"ssh-keygen",
			&[
				"-t",
				"ed25519",
				"-N",
				"",
				"-C",
				email,
				"-f",
				key_path.to_str().unwrap(),
			],
		);

		let (_tmp_dir, repo) = repo_init_empty()?;
		{
			let mut config = repo.config()?;
			config.set_str("gpg.format", "ssh")?;
			// the common on-disk config: point at the public key file.
			config.set_str(
				"user.signingKey",
				pub_path.to_str().unwrap(),
			)?;
		}
		let signer =
			SignBuilder::from_gitconfig(&repo, &repo.config()?)?;
		assert_eq!("ssh-keygen", signer.program());

		let sig = git2::Signature::now("gitui test", email)?;
		let tree = {
			let mut index = repo.index()?;
			let tree_id = index.write_tree()?;
			repo.find_tree(tree_id)?
		};

		// no agent reachable: with the buggy `-U` this fails/hangs, with the
		// on-disk key it must just work.
		let saved_sock = std::env::var_os("SSH_AUTH_SOCK");
		std::env::remove_var("SSH_AUTH_SOCK");
		let commit_id = create_signed_commit(
			&repo,
			&*signer,
			&sig,
			&sig,
			"ssh signed commit",
			&tree,
			&[],
		);
		match saved_sock {
			Some(sock) => std::env::set_var("SSH_AUTH_SOCK", sock),
			None => std::env::remove_var("SSH_AUTH_SOCK"),
		}
		let commit_id = commit_id?;

		let (signature, signed_data) =
			repo.extract_signature(&commit_id, None)?;
		let signature = std::str::from_utf8(&signature).unwrap();
		assert!(
			signature.contains("BEGIN SSH SIGNATURE"),
			"expected an ssh signature, got: {signature}"
		);

		// verify against an allowed_signers file built from the public key.
		let pub_key = std::fs::read_to_string(&pub_path)?;
		let mut fields = pub_key.split_whitespace();
		let key_type = fields.next().expect("key type");
		let key_blob = fields.next().expect("key blob");
		let allowed = dir.path().join("allowed_signers");
		std::fs::write(
			&allowed,
			format!("{email} {key_type} {key_blob}\n"),
		)?;

		let sig_file = dir.path().join("commit.sig");
		let data_file = dir.path().join("commit.data");
		std::fs::write(&sig_file, signature)?;
		std::fs::write(&data_file, &*signed_data)?;
		let verify = Command::new("ssh-keygen")
			.args([
				"-Y",
				"verify",
				"-f",
				allowed.to_str().unwrap(),
				"-I",
				email,
				"-n",
				"git",
				"-s",
				sig_file.to_str().unwrap(),
			])
			.stdin(std::fs::File::open(&data_file)?)
			.output()?;
		let verify_out = String::from_utf8_lossy(&verify.stdout);
		assert!(
			verify.status.success()
				&& verify_out.contains("Good")
				&& verify_out.contains(email),
			"ssh-keygen did not accept the signature: {}{}",
			verify_out,
			String::from_utf8_lossy(&verify.stderr)
		);

		Ok(())
	}
}
