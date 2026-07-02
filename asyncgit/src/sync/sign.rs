//! Sign commit data.

use ssh_key::{HashAlg, LineEnding, PrivateKey};
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
	fn program(&self) -> &String;

	/// only available in `#[cfg(test)]` helping to diagnose issues
	#[cfg(test)]
	fn signing_key(&self) -> &String;
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
				let ssh_signer = config
					.get_string("user.signingKey")
					.ok()
					.and_then(|key_path| {
						key_path.strip_prefix('~').map_or_else(
							|| Some(PathBuf::from(&key_path)),
							|ssh_key_path| {
								dirs::home_dir().map(|home| {
									home.join(
										ssh_key_path
											.strip_prefix('/')
											.unwrap_or(ssh_key_path),
									)
								})
							},
						)
					})
					.ok_or_else(|| {
						SignBuilderError::SSHSigningKey(String::from(
							"ssh key setting absent",
						))
					})
					.and_then(SSHSign::new)?;
				let signer: Box<dyn Sign> = Box::new(ssh_signer);
				Ok(signer)
			}
			_ => Err(SignBuilderError::InvalidFormat(format)),
		}
	}
}

/// Sign commit data using `OpenPGP`
pub struct GPGSign {
	program: String,
	signing_key: String,
}

impl GPGSign {
	/// Create new [`GPGSign`] using given program and signing key.
	pub fn new(program: &str, signing_key: &str) -> Self {
		Self {
			program: program.to_string(),
			signing_key: signing_key.to_string(),
		}
	}
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
	fn program(&self) -> &String {
		&self.program
	}

	#[cfg(test)]
	fn signing_key(&self) -> &String {
		&self.signing_key
	}
}

/// Sign commit data using `SSHDiskKeySign`
pub struct SSHSign {
	#[cfg(test)]
	program: String,
	#[cfg(test)]
	key_path: String,
	secret_key: PrivateKey,
}

impl SSHSign {
	/// Create new `SSHDiskKeySign` for sign.
	pub fn new(mut key: PathBuf) -> Result<Self, SignBuilderError> {
		key.set_extension("");
		if key.is_file() {
			#[cfg(test)]
			let key_path = format!("{}", &key.display());
			std::fs::read(key)
				.ok()
				.and_then(|bytes| {
					PrivateKey::from_openssh(bytes).ok()
				})
				.map(|secret_key| Self {
					#[cfg(test)]
					program: "ssh".to_string(),
					#[cfg(test)]
					key_path,
					secret_key,
				})
				.ok_or_else(|| {
					SignBuilderError::SSHSigningKey(String::from(
						"Fail to read the private key for sign.",
					))
				})
		} else {
			Err(SignBuilderError::SSHSigningKey(
				String::from("Currently, we only support a pair of ssh key in disk."),
			))
		}
	}
}

impl Sign for SSHSign {
	fn sign(
		&self,
		commit: &[u8],
	) -> Result<(String, Option<String>), SignError> {
		let sig = self
			.secret_key
			.sign("git", HashAlg::Sha256, commit)
			.map_err(|err| SignError::Spawn(err.to_string()))?
			.to_pem(LineEnding::LF)
			.map_err(|err| SignError::Spawn(err.to_string()))?;
		Ok((sig, None))
	}

	#[cfg(test)]
	fn program(&self) -> &String {
		&self.program
	}

	#[cfg(test)]
	fn signing_key(&self) -> &String {
		&self.key_path
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

		{
			let mut config = repo.config()?;
			config.set_str("gpg.program", "ssh")?;
			config.set_str("user.signingKey", "/tmp/key.pub")?;
		}

		let sign =
			SignBuilder::from_gitconfig(&repo, &repo.config()?)?;

		assert_eq!("ssh", sign.program());
		assert_eq!("/tmp/key.pub", sign.signing_key());

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
		let p12 = home.join("bundle.p12");
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
		run(
			"openssl",
			&[
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
				// legacy PBE: gpgsm can't read OpenSSL 3's default PBES2/AES.
				"-keypbe",
				"PBE-SHA1-3DES",
				"-certpbe",
				"PBE-SHA1-3DES",
				"-macalg",
				"sha1",
			],
		);
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

		// trust our self-signed root ("S" relaxes CA checks) so gpgsm will sign.
		let listing = run(
			"gpgsm",
			&["--batch", "--with-colons", "--list-secret-keys"],
		);
		let listing = String::from_utf8_lossy(&listing.stdout);
		let fingerprint = listing
			.lines()
			.filter_map(|line| line.strip_prefix("fpr:"))
			.find_map(|rest| {
				rest.split(':').find(|field| {
					field.len() == 40
						&& field
							.bytes()
							.all(|b| b.is_ascii_hexdigit())
				})
			})
			.expect("could not determine cert fingerprint");
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
}
