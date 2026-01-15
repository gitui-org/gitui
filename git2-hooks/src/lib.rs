//! git2-rs addon supporting git hooks
//!
//! we look for hooks in the following locations:
//!  * whatever `config.hooksPath` points to
//!  * `.git/hooks/`
//!  * whatever list of paths provided as `other_paths` (in order)
//!
//! most basic hook is: [`hooks_pre_commit`]. see also other `hooks_*` functions.
//!
//! [`create_hook`] is useful to create git hooks from code (unittest make heavy usage of it)

#![forbid(unsafe_code)]
#![deny(
	mismatched_lifetime_syntaxes,
	unused_imports,
	unused_must_use,
	dead_code,
	unstable_name_collisions,
	unused_assignments
)]
#![deny(clippy::all, clippy::perf, clippy::pedantic, clippy::nursery)]
#![allow(
	clippy::missing_errors_doc,
	clippy::must_use_candidate,
	clippy::module_name_repetitions
)]

mod error;
mod hookspath;

use std::{
	fs::File,
	io::{Read, Write},
	path::{Path, PathBuf},
};

pub use error::HooksError;
use error::Result;
use hookspath::HookPaths;

use git2::{Oid, Repository};

pub const HOOK_POST_COMMIT: &str = "post-commit";
pub const HOOK_PRE_COMMIT: &str = "pre-commit";
pub const HOOK_COMMIT_MSG: &str = "commit-msg";
pub const HOOK_PREPARE_COMMIT_MSG: &str = "prepare-commit-msg";
pub const HOOK_PRE_PUSH: &str = "pre-push";

const HOOK_COMMIT_MSG_TEMP_FILE: &str = "COMMIT_EDITMSG";

/// Check if a given hook is present considering config/paths and optional extra paths.
pub fn hook_available(
	repo: &Repository,
	other_paths: Option<&[&str]>,
	hook: &str,
) -> Result<bool> {
	let hook = HookPaths::new(repo, other_paths, hook)?;
	Ok(hook.found())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrePushRef {
	pub local_ref: String,
	pub local_oid: Option<Oid>,
	pub remote_ref: String,
	pub remote_oid: Option<Oid>,
}

impl PrePushRef {
	#[must_use]
	pub fn new(
		local_ref: impl Into<String>,
		local_oid: Option<Oid>,
		remote_ref: impl Into<String>,
		remote_oid: Option<Oid>,
	) -> Self {
		Self {
			local_ref: local_ref.into(),
			local_oid,
			remote_ref: remote_ref.into(),
			remote_oid,
		}
	}

	fn format_oid(oid: Option<Oid>) -> String {
		oid.map_or_else(|| "0".repeat(40), |id| id.to_string())
	}

	#[must_use]
	pub fn to_line(&self) -> String {
		format!(
			"{} {} {} {}",
			self.local_ref,
			Self::format_oid(self.local_oid),
			self.remote_ref,
			Self::format_oid(self.remote_oid)
		)
	}
}

/// Response from running a hook
#[derive(Debug, PartialEq, Eq)]
pub struct HookRunResponse {
	/// path of the hook that was run
	pub hook: PathBuf,
	/// stdout output emitted by hook
	pub stdout: String,
	/// stderr output emitted by hook
	pub stderr: String,
	/// exit code as reported back from process calling the hook (0 = success)
	pub code: i32,
}

#[derive(Debug, PartialEq, Eq)]
pub enum HookResult {
	/// No hook found
	NoHookFound,
	/// Hook executed (check `HookRunResponse.code` for success/failure)
	Run(HookRunResponse),
}

impl HookResult {
	/// helper to check if result is ok (hook succeeded with exit code 0 or no hook found)
	pub const fn is_ok(&self) -> bool {
		match self {
			Self::Run(response) => response.code == 0,
			Self::NoHookFound => true,
		}
	}

	/// helper to check if no hook was found
	pub const fn is_no_hook_found(&self) -> bool {
		matches!(self, Self::NoHookFound)
	}
}

impl HookRunResponse {
	/// Check if the hook succeeded (exit code 0)
	pub const fn is_successful(&self) -> bool {
		self.code == 0
	}
}

/// helper method to create git hooks programmatically (heavy used in unittests)
///
/// # Panics
/// Panics if hook could not be created
pub fn create_hook(
	r: &Repository,
	hook: &str,
	hook_script: &[u8],
) -> PathBuf {
	let hook = HookPaths::new(r, None, hook).unwrap();

	let path = hook.hook.clone();

	create_hook_in_path(&hook.hook, hook_script);

	path
}

fn create_hook_in_path(path: &Path, hook_script: &[u8]) {
	File::create(path).unwrap().write_all(hook_script).unwrap();

	#[cfg(unix)]
	{
		std::process::Command::new("chmod")
			.arg("+x")
			.arg(path)
			// .current_dir(path)
			.output()
			.unwrap();
	}
}

/// Git hook: `commit_msg`
///
/// This hook is documented here <https://git-scm.com/docs/githooks#_commit_msg>.
/// We use the same convention as other git clients to create a temp file containing
/// the commit message at `<.git|hooksPath>/COMMIT_EDITMSG` and pass it's relative path as the only
/// parameter to the hook script.
pub fn hooks_commit_msg(
	repo: &Repository,
	other_paths: Option<&[&str]>,
	msg: &mut String,
) -> Result<HookResult> {
	let hook = HookPaths::new(repo, other_paths, HOOK_COMMIT_MSG)?;

	if !hook.found() {
		return Ok(HookResult::NoHookFound);
	}

	let temp_file = hook.git.join(HOOK_COMMIT_MSG_TEMP_FILE);
	File::create(&temp_file)?.write_all(msg.as_bytes())?;

	let res = hook.run_hook_os_str([&temp_file])?;

	// load possibly altered msg
	msg.clear();
	File::open(temp_file)?.read_to_string(msg)?;

	Ok(res)
}

/// this hook is documented here <https://git-scm.com/docs/githooks#_pre_commit>
pub fn hooks_pre_commit(
	repo: &Repository,
	other_paths: Option<&[&str]>,
) -> Result<HookResult> {
	let hook = HookPaths::new(repo, other_paths, HOOK_PRE_COMMIT)?;

	if !hook.found() {
		return Ok(HookResult::NoHookFound);
	}

	hook.run_hook(&[])
}

/// this hook is documented here <https://git-scm.com/docs/githooks#_post_commit>
pub fn hooks_post_commit(
	repo: &Repository,
	other_paths: Option<&[&str]>,
) -> Result<HookResult> {
	let hook = HookPaths::new(repo, other_paths, HOOK_POST_COMMIT)?;

	if !hook.found() {
		return Ok(HookResult::NoHookFound);
	}

	hook.run_hook(&[])
}

/// this hook is documented here <https://git-scm.com/docs/githooks#_pre_push>
///
/// According to git documentation, pre-push hook receives:
/// - remote name as first argument (or URL if remote is not named)
/// - remote URL as second argument
/// - information about refs being pushed via stdin in format:
///   `<local-ref> SP <local-object-name> SP <remote-ref> SP <remote-object-name> LF`
///
/// If `remote` is `None` or empty, the `url` is used for both arguments as per Git spec.
///
/// Note: The hook is called even when `updates` is empty (matching Git's behavior).
/// This can occur when pushing tags that already exist on the remote.
pub fn hooks_pre_push(
	repo: &Repository,
	other_paths: Option<&[&str]>,
	remote: Option<&str>,
	url: &str,
	updates: &[PrePushRef],
) -> Result<HookResult> {
	let hook = HookPaths::new(repo, other_paths, HOOK_PRE_PUSH)?;

	if !hook.found() {
		return Ok(HookResult::NoHookFound);
	}

	// If a remote is not named (None or empty), the URL is passed for both arguments
	let remote_name = match remote {
		Some(r) if !r.is_empty() => r,
		_ => url,
	};

	// Build stdin according to Git pre-push hook spec:
	// <local-ref> SP <local-object-name> SP <remote-ref> SP <remote-object-name> LF

	let mut stdin_data = String::new();
	for update in updates {
		stdin_data.push_str(&update.to_line());
		stdin_data.push('\n');
	}

	hook.run_hook_os_str_with_stdin(
		[remote_name, url],
		Some(stdin_data.as_bytes()),
	)
}

pub enum PrepareCommitMsgSource {
	Message,
	Template,
	Merge,
	Squash,
	Commit(git2::Oid),
}

/// this hook is documented here <https://git-scm.com/docs/githooks#_prepare_commit_msg>
#[allow(clippy::needless_pass_by_value)]
pub fn hooks_prepare_commit_msg(
	repo: &Repository,
	other_paths: Option<&[&str]>,
	source: PrepareCommitMsgSource,
	msg: &mut String,
) -> Result<HookResult> {
	let hook =
		HookPaths::new(repo, other_paths, HOOK_PREPARE_COMMIT_MSG)?;

	if !hook.found() {
		return Ok(HookResult::NoHookFound);
	}

	let temp_file = hook.git.join(HOOK_COMMIT_MSG_TEMP_FILE);
	File::create(&temp_file)?.write_all(msg.as_bytes())?;

	let temp_file_path = temp_file.as_os_str().to_string_lossy();

	let vec = vec![
		temp_file_path.as_ref(),
		match source {
			PrepareCommitMsgSource::Message => "message",
			PrepareCommitMsgSource::Template => "template",
			PrepareCommitMsgSource::Merge => "merge",
			PrepareCommitMsgSource::Squash => "squash",
			PrepareCommitMsgSource::Commit(_) => "commit",
		},
	];
	let mut args = vec;

	let id = if let PrepareCommitMsgSource::Commit(id) = &source {
		Some(id.to_string())
	} else {
		None
	};

	if let Some(id) = &id {
		args.push(id);
	}

	let res = hook.run_hook(args.as_slice())?;

	// load possibly altered msg
	msg.clear();
	File::open(temp_file)?.read_to_string(msg)?;

	Ok(res)
}

#[cfg(test)]
mod tests {
	use super::*;
	use git2_testing::{repo_init, repo_init_bare};
	use pretty_assertions::assert_eq;
	use tempfile::TempDir;

	fn branch_update(
		repo: &Repository,
		remote: Option<&str>,
		branch: &str,
		remote_branch: Option<&str>,
		delete: bool,
	) -> PrePushRef {
		let local_ref = format!("refs/heads/{branch}");
		let local_oid = (!delete).then(|| {
			repo.find_branch(branch, git2::BranchType::Local)
				.unwrap()
				.get()
				.peel_to_commit()
				.unwrap()
				.id()
		});

		let remote_branch = remote_branch.unwrap_or(branch);
		let remote_ref = format!("refs/heads/{remote_branch}");
		let remote_oid = remote.and_then(|remote_name| {
			repo.find_reference(&format!(
				"refs/remotes/{remote_name}/{remote_branch}"
			))
			.ok()
			.and_then(|r| r.peel_to_commit().ok())
			.map(|c| c.id())
		});

		PrePushRef::new(local_ref, local_oid, remote_ref, remote_oid)
	}

	fn stdin_from_updates(updates: &[PrePushRef]) -> String {
		updates
			.iter()
			.map(|u| format!("{}\n", u.to_line()))
			.collect()
	}

	fn head_branch(repo: &Repository) -> String {
		repo.head().unwrap().shorthand().unwrap().to_string()
	}

	#[test]
	fn test_smoke() {
		let (_td, repo) = repo_init();

		let mut msg = String::from("test");
		let res = hooks_commit_msg(&repo, None, &mut msg).unwrap();

		assert_eq!(res, HookResult::NoHookFound);

		let hook = b"#!/bin/sh
exit 0
        ";

		create_hook(&repo, HOOK_POST_COMMIT, hook);

		let res = hooks_post_commit(&repo, None).unwrap();

		assert!(res.is_ok());
	}

	#[test]
	fn test_hooks_commit_msg_ok() {
		let (_td, repo) = repo_init();

		let hook = b"#!/bin/sh
exit 0
        ";

		create_hook(&repo, HOOK_COMMIT_MSG, hook);

		let mut msg = String::from("test");
		let res = hooks_commit_msg(&repo, None, &mut msg).unwrap();

		assert!(res.is_ok());

		assert_eq!(msg, String::from("test"));
	}

	#[test]
	fn test_hooks_commit_msg_with_shell_command_ok() {
		let (_td, repo) = repo_init();

		let hook = br#"#!/bin/sh
COMMIT_MSG="$(cat "$1")"
printf "$COMMIT_MSG" | sed 's/sth/shell_command/g' > "$1"
exit 0
        "#;

		create_hook(&repo, HOOK_COMMIT_MSG, hook);

		let mut msg = String::from("test_sth");
		let res = hooks_commit_msg(&repo, None, &mut msg).unwrap();

		assert!(res.is_ok());

		assert_eq!(msg, String::from("test_shell_command"));
	}

	#[test]
	fn test_pre_commit_sh() {
		let (_td, repo) = repo_init();

		let hook = b"#!/bin/sh
exit 0
        ";

		create_hook(&repo, HOOK_PRE_COMMIT, hook);
		let res = hooks_pre_commit(&repo, None).unwrap();
		assert!(res.is_ok());
	}

	#[test]
	fn test_hook_with_missing_shebang() {
		const TEXT: &str = "Hello, world!";

		let (_td, repo) = repo_init();

		let hook = b"echo \"$@\"\nexit 42";

		create_hook(&repo, HOOK_PRE_COMMIT, hook);

		let hook =
			HookPaths::new(&repo, None, HOOK_PRE_COMMIT).unwrap();

		assert!(hook.found());

		let result = hook.run_hook(&[TEXT]).unwrap();

		let HookResult::Run(response) = result else {
			unreachable!("run_hook should've failed");
		};

		let stdout = response.stdout.as_str().trim_ascii_end();

		assert_eq!(response.code, 42);
		assert_eq!(response.hook, hook.hook);
		assert_eq!(stdout, TEXT, "{:?} != {TEXT:?}", stdout);
		assert!(response.stderr.is_empty());
	}

	#[test]
	fn test_no_hook_found() {
		let (_td, repo) = repo_init();

		let res = hooks_pre_commit(&repo, None).unwrap();
		assert_eq!(res, HookResult::NoHookFound);
	}

	#[test]
	fn test_other_path() {
		let (td, repo) = repo_init();

		let hook = b"#!/bin/sh
exit 0
        ";

		let custom_hooks_path = td.path().join(".myhooks");

		std::fs::create_dir(dbg!(&custom_hooks_path)).unwrap();
		create_hook_in_path(
			dbg!(custom_hooks_path.join(HOOK_PRE_COMMIT).as_path()),
			hook,
		);

		let res =
			hooks_pre_commit(&repo, Some(&["../.myhooks"])).unwrap();

		assert!(res.is_ok());
	}

	#[test]
	fn test_other_path_precedence() {
		let (td, repo) = repo_init();

		{
			let hook = b"#!/bin/sh
exit 0
        ";

			create_hook(&repo, HOOK_PRE_COMMIT, hook);
		}

		{
			let reject_hook = b"#!/bin/sh
exit 1
        ";

			let custom_hooks_path = td.path().join(".myhooks");
			std::fs::create_dir(dbg!(&custom_hooks_path)).unwrap();
			create_hook_in_path(
				dbg!(custom_hooks_path
					.join(HOOK_PRE_COMMIT)
					.as_path()),
				reject_hook,
			);
		}

		let res =
			hooks_pre_commit(&repo, Some(&["../.myhooks"])).unwrap();

		assert!(res.is_ok());
	}

	#[test]
	fn test_pre_commit_fail_sh() {
		let (_td, repo) = repo_init();

		let hook = b"#!/bin/sh
echo 'rejected'
exit 1
        ";

		create_hook(&repo, HOOK_PRE_COMMIT, hook);
		let res = hooks_pre_commit(&repo, None).unwrap();
		assert!(!res.is_ok());
	}

	#[test]
	fn test_env_containing_path() {
		const PATH_EXPORT: &str = "export PATH";

		let (_td, repo) = repo_init();

		let hook = b"#!/bin/sh
export
exit 1
        ";

		create_hook(&repo, HOOK_PRE_COMMIT, hook);
		let res = hooks_pre_commit(&repo, None).unwrap();

		let HookResult::Run(response) = res else {
			unreachable!()
		};

		assert!(
			response
				.stdout
				.lines()
				.any(|line| line.starts_with(PATH_EXPORT)),
			"Could not find line starting with {PATH_EXPORT:?} in: {:?}",
			response.stdout
		);
	}

	#[test]
	fn test_pre_commit_fail_hookspath() {
		let (_td, repo) = repo_init();
		let hooks = TempDir::new().unwrap();

		let hook = b"#!/bin/sh
echo 'rejected'
exit 1
        ";

		create_hook_in_path(&hooks.path().join("pre-commit"), hook);

		repo.config()
			.unwrap()
			.set_str(
				"core.hooksPath",
				hooks.path().as_os_str().to_str().unwrap(),
			)
			.unwrap();

		let res = hooks_pre_commit(&repo, None).unwrap();

		let HookResult::Run(response) = res else {
			unreachable!()
		};

		assert_eq!(response.code, 1);
		assert_eq!(&response.stdout, "rejected\n");
	}

	#[test]
	fn test_pre_commit_fail_bare() {
		let (_td, repo) = repo_init_bare();

		let hook = b"#!/bin/sh
echo 'rejected'
exit 1
        ";

		create_hook(&repo, HOOK_PRE_COMMIT, hook);
		let res = hooks_pre_commit(&repo, None).unwrap();
		assert!(!res.is_ok());
	}

	#[test]
	fn test_pre_commit_py() {
		let (_td, repo) = repo_init();

		// mirror how python pre-commit sets itself up
		#[cfg(not(windows))]
		let hook = b"#!/usr/bin/env python
import sys
sys.exit(0)
        ";
		#[cfg(windows)]
		let hook = b"#!/bin/env python.exe
import sys
sys.exit(0)
        ";

		create_hook(&repo, HOOK_PRE_COMMIT, hook);
		let res = hooks_pre_commit(&repo, None).unwrap();
		assert!(res.is_ok(), "{res:?}");
	}

	#[test]
	fn test_pre_commit_fail_py() {
		let (_td, repo) = repo_init();

		// mirror how python pre-commit sets itself up
		#[cfg(not(windows))]
		let hook = b"#!/usr/bin/env python
import sys
sys.exit(1)
        ";
		#[cfg(windows)]
		let hook = b"#!/bin/env python.exe
import sys
sys.exit(1)
        ";

		create_hook(&repo, HOOK_PRE_COMMIT, hook);
		let res = hooks_pre_commit(&repo, None).unwrap();
		assert!(!res.is_ok());
	}

	#[test]
	fn test_hooks_commit_msg_reject() {
		let (_td, repo) = repo_init();

		let hook = b"#!/bin/sh
	echo 'msg' > \"$1\"
	echo 'rejected'
	exit 1
        ";

		create_hook(&repo, HOOK_COMMIT_MSG, hook);

		let mut msg = String::from("test");
		let res = hooks_commit_msg(&repo, None, &mut msg).unwrap();

		let HookResult::Run(response) = res else {
			unreachable!()
		};

		assert_eq!(response.code, 1);
		assert_eq!(&response.stdout, "rejected\n");

		assert_eq!(msg, String::from("msg\n"));
	}

	#[test]
	fn test_commit_msg_no_block_but_alter() {
		let (_td, repo) = repo_init();

		let hook = b"#!/bin/sh
echo 'msg' > \"$1\"
exit 0
        ";

		create_hook(&repo, HOOK_COMMIT_MSG, hook);

		let mut msg = String::from("test");
		let res = hooks_commit_msg(&repo, None, &mut msg).unwrap();

		assert!(res.is_ok());
		assert_eq!(msg, String::from("msg\n"));
	}

	#[test]
	fn test_hook_pwd_in_bare_without_workdir() {
		let (_td, repo) = repo_init_bare();
		let git_root = repo.path().to_path_buf();

		let hook =
			HookPaths::new(&repo, None, HOOK_POST_COMMIT).unwrap();

		assert_eq!(hook.pwd, git_root);
	}

	#[test]
	fn test_hook_pwd() {
		let (_td, repo) = repo_init();
		let git_root = repo.path().to_path_buf();

		let hook =
			HookPaths::new(&repo, None, HOOK_POST_COMMIT).unwrap();

		assert_eq!(hook.pwd, git_root.parent().unwrap());
	}

	#[test]
	fn test_hooks_prep_commit_msg_success() {
		let (_td, repo) = repo_init();

		let hook = b"#!/bin/sh
echo \"msg:$2\" > \"$1\"
exit 0
        ";

		create_hook(&repo, HOOK_PREPARE_COMMIT_MSG, hook);

		let mut msg = String::from("test");
		let res = hooks_prepare_commit_msg(
			&repo,
			None,
			PrepareCommitMsgSource::Message,
			&mut msg,
		)
		.unwrap();

		assert!(res.is_ok());
		assert_eq!(msg, String::from("msg:message\n"));
	}

	#[test]
	fn test_hooks_prep_commit_msg_reject() {
		let (_td, repo) = repo_init();

		let hook = b"#!/bin/sh
echo \"$2,$3\" > \"$1\"
echo 'rejected'
exit 2
        ";

		create_hook(&repo, HOOK_PREPARE_COMMIT_MSG, hook);

		let mut msg = String::from("test");
		let res = hooks_prepare_commit_msg(
			&repo,
			None,
			PrepareCommitMsgSource::Commit(git2::Oid::zero()),
			&mut msg,
		)
		.unwrap();

		let HookResult::Run(response) = res else {
			unreachable!()
		};

		assert_eq!(response.code, 2);
		assert_eq!(&response.stdout, "rejected\n");

		assert_eq!(
			msg,
			String::from(
				"commit,0000000000000000000000000000000000000000\n"
			)
		);
	}

	#[test]
	fn test_pre_push_sh() {
		let (_td, repo) = repo_init();

		let hook = b"#!/bin/sh
exit 0
	";

		create_hook(&repo, HOOK_PRE_PUSH, hook);

		let branch = head_branch(&repo);
		let updates = [branch_update(
			&repo,
			Some("origin"),
			&branch,
			None,
			false,
		)];

		let res = hooks_pre_push(
			&repo,
			None,
			Some("origin"),
			"https://example.com/repo.git",
			&updates,
		)
		.unwrap();

		assert!(res.is_ok());
	}

	#[test]
	fn test_pre_push_fail_sh() {
		let (_td, repo) = repo_init();

		let hook = b"#!/bin/sh
echo 'failed'
exit 3
	";
		create_hook(&repo, HOOK_PRE_PUSH, hook);

		let branch = head_branch(&repo);
		let updates = [branch_update(
			&repo,
			Some("origin"),
			&branch,
			None,
			false,
		)];

		let res = hooks_pre_push(
			&repo,
			None,
			Some("origin"),
			"https://example.com/repo.git",
			&updates,
		)
		.unwrap();
		let HookResult::Run(response) = res else {
			unreachable!()
		};
		assert_eq!(response.code, 3);
		assert_eq!(&response.stdout, "failed\n");
	}

	#[test]
	fn test_pre_push_no_remote_name() {
		let (_td, repo) = repo_init();

		let hook = b"#!/bin/sh
# Verify that when remote is None, URL is passed for both arguments
echo \"arg1=$1 arg2=$2\"
exit 0
	";

		create_hook(&repo, HOOK_PRE_PUSH, hook);

		let branch = head_branch(&repo);
		let updates =
			[branch_update(&repo, None, &branch, None, false)];

		let res = hooks_pre_push(
			&repo,
			None,
			None,
			"https://example.com/repo.git",
			&updates,
		)
		.unwrap();

		assert!(res.is_ok(), "Expected Ok result, got: {res:?}");
	}

	#[test]
	fn test_pre_push_with_arguments() {
		let (_td, repo) = repo_init();

		// Hook that verifies it receives the correct arguments
		// and prints them for verification
		let hook = b"#!/bin/sh
echo \"remote_name=$1\"
echo \"remote_url=$2\"
# Read stdin to verify it's available (even if empty for now)
stdin_content=$(cat)
echo \"stdin_length=${#stdin_content}\"
exit 0
	";

		create_hook(&repo, HOOK_PRE_PUSH, hook);

		let branch = head_branch(&repo);
		let updates = [branch_update(
			&repo,
			Some("origin"),
			&branch,
			None,
			false,
		)];

		let res = hooks_pre_push(
			&repo,
			None,
			Some("origin"),
			"https://example.com/repo.git",
			&updates,
		)
		.unwrap();

		let HookResult::Run(response) = res else {
			unreachable!("Expected Run result, got: {res:?}")
		};

		assert!(response.is_successful(), "Hook should succeed");

		// Verify the hook received and echoed the correct arguments
		assert!(
			response.stdout.contains("remote_name=origin"),
			"Expected stdout to contain 'remote_name=origin', got: {}",
			response.stdout
		);
		assert!(
			response
				.stdout
				.contains("remote_url=https://example.com/repo.git"),
			"Expected stdout to contain URL, got: {}",
			response.stdout
		);
		assert!(
			response.stdout.contains("stdin_length=")
				&& !response.stdout.contains("stdin_length=0"),
			"Expected stdin to be non-empty, got: {}",
			response.stdout
		);
	}

	#[test]
	fn test_pre_push_multiple_updates() {
		let (_td, repo) = repo_init();

		let hook = b"#!/bin/sh
cat
exit 0
	";

		create_hook(&repo, HOOK_PRE_PUSH, hook);

		let branch = head_branch(&repo);
		let branch_update = branch_update(
			&repo,
			Some("origin"),
			&branch,
			None,
			false,
		);

		// create a tag to add a second refspec
		let head_commit =
			repo.head().unwrap().peel_to_commit().unwrap();
		repo.tag_lightweight("v1", head_commit.as_object(), false)
			.unwrap();
		let tag_ref = repo.find_reference("refs/tags/v1").unwrap();
		let tag_oid = tag_ref.target().unwrap();
		let tag_update = PrePushRef::new(
			"refs/tags/v1",
			Some(tag_oid),
			"refs/tags/v1",
			None,
		);

		let updates = [branch_update, tag_update];
		let expected_stdin = stdin_from_updates(&updates);

		let res = hooks_pre_push(
			&repo,
			None,
			Some("origin"),
			"https://example.com/repo.git",
			&updates,
		)
		.unwrap();

		let HookResult::Run(response) = res else {
			unreachable!("Expected Run result, got: {res:?}")
		};

		assert!(
			response.is_successful(),
			"Hook should succeed: stdout {} stderr {}",
			response.stdout,
			response.stderr
		);
		assert_eq!(
			response.stdout, expected_stdin,
			"stdin should include all refspec lines"
		);
	}

	#[test]
	fn test_pre_push_delete_ref_uses_zero_oid() {
		let (_td, repo) = repo_init();

		let hook = b"#!/bin/sh
cat
exit 0
	";

		create_hook(&repo, HOOK_PRE_PUSH, hook);

		let branch = head_branch(&repo);
		let updates = [branch_update(
			&repo,
			Some("origin"),
			&branch,
			None,
			true,
		)];
		let expected_stdin = stdin_from_updates(&updates);

		let res = hooks_pre_push(
			&repo,
			None,
			Some("origin"),
			"https://example.com/repo.git",
			&updates,
		)
		.unwrap();

		let HookResult::Run(response) = res else {
			unreachable!("Expected Run result, got: {res:?}")
		};

		assert!(response.is_successful());
		assert_eq!(response.stdout, expected_stdin);
		assert!(
			response
				.stdout
				.contains("0000000000000000000000000000000000000000"),
			"delete pushes must use zero oid for new object"
		);
	}

	#[test]
	fn test_pre_push_verifies_arguments() {
		let (_td, repo) = repo_init();

		// Hook that verifies and echoes the arguments it receives
		let hook = b"#!/bin/sh
# Verify we got the expected arguments
if [ \"$1\" != \"origin\" ]; then
    echo \"ERROR: Expected remote name 'origin', got '$1'\" >&2
    exit 1
fi
if [ \"$2\" != \"https://github.com/user/repo.git\" ]; then
    echo \"ERROR: Expected URL 'https://github.com/user/repo.git', got '$2'\" >&2
    exit 1
fi
echo \"Arguments verified: remote=$1, url=$2\"
exit 0
	";

		create_hook(&repo, HOOK_PRE_PUSH, hook);

		let branch = head_branch(&repo);
		let updates = [branch_update(
			&repo,
			Some("origin"),
			&branch,
			None,
			false,
		)];

		let res = hooks_pre_push(
			&repo,
			None,
			Some("origin"),
			"https://github.com/user/repo.git",
			&updates,
		)
		.unwrap();

		match res {
			HookResult::Run(response) if response.is_successful() => {
				// Success - arguments were correct
			}
			HookResult::Run(response) => {
				panic!(
					"Hook failed validation!\nstdout: {}\nstderr: {}",
					response.stdout, response.stderr
				);
			}
			_ => unreachable!(),
		}
	}

	#[test]
	fn test_pre_push_empty_stdin_currently() {
		let (_td, repo) = repo_init();

		// Hook that checks stdin content
		let hook = b"#!/bin/sh
	stdin_content=$(cat)
	if [ -z \"$stdin_content\" ]; then
	    echo \"stdin was unexpectedly empty\" >&2
	    exit 1
	fi
	echo \"stdin_length=${#stdin_content}\"
	echo \"stdin_content=$stdin_content\"
	exit 0
		";

		create_hook(&repo, HOOK_PRE_PUSH, hook);

		let branch = head_branch(&repo);
		let updates = [branch_update(
			&repo,
			Some("origin"),
			&branch,
			None,
			false,
		)];

		let res = hooks_pre_push(
			&repo,
			None,
			Some("origin"),
			"https://github.com/user/repo.git",
			&updates,
		)
		.unwrap();

		let HookResult::Run(response) = res else {
			unreachable!("Expected Run result, got: {res:?}")
		};

		assert!(response.is_successful(), "Hook should succeed");

		assert!(
			response.stdout.contains("stdin_length="),
			"Expected stdin to be non-empty, got: {}",
			response.stdout
		);
	}

	#[test]
	fn test_pre_push_with_proper_stdin() {
		let (_td, repo) = repo_init();

		// Hook that verifies it receives refs information via stdin
		// According to Git spec, format should be:
		// <local-ref> SP <local-sha> SP <remote-ref> SP <remote-sha> LF
		let hook = b"#!/bin/sh
# Read stdin
stdin_content=$(cat)
echo \"stdin received: $stdin_content\"

# Verify stdin is not empty
if [ -z \"$stdin_content\" ]; then
    echo \"ERROR: stdin is empty, expected ref information\" >&2
    exit 1
fi

# Verify stdin contains expected format
# Should have: refs/heads/master <sha> refs/heads/master <sha>
if ! echo \"$stdin_content\" | grep -q \"^refs/heads/\"; then
    echo \"ERROR: stdin does not contain expected ref format\" >&2
    echo \"Got: $stdin_content\" >&2
    exit 1
fi

# Verify it has 4 space-separated fields
field_count=$(echo \"$stdin_content\" | awk '{print NF}')
if [ \"$field_count\" != \"4\" ]; then
    echo \"ERROR: Expected 4 fields, got $field_count\" >&2
    exit 1
fi

echo \"SUCCESS: Received properly formatted stdin\"
exit 0
	";

		create_hook(&repo, HOOK_PRE_PUSH, hook);

		let branch = head_branch(&repo);
		let updates = [branch_update(
			&repo,
			Some("origin"),
			&branch,
			None,
			false,
		)];

		let res = hooks_pre_push(
			&repo,
			None,
			Some("origin"),
			"https://github.com/user/repo.git",
			&updates,
		)
		.unwrap();

		let HookResult::Run(response) = res else {
			panic!("Expected Run result, got: {res:?}")
		};

		// This should now pass with proper stdin implementation
		assert!(
			response.is_successful(),
			"Hook should succeed with proper stdin.\nstdout: {}\nstderr: {}",
			response.stdout,
			response.stderr
		);

		// Verify the hook received proper stdin format
		assert!(
			response.stdout.contains("SUCCESS"),
			"Expected hook to validate stdin format.\nstdout: {}\nstderr: {}",
			response.stdout,
			response.stderr
		);
	}

	#[test]
	fn test_pre_push_uses_push_target_remote_not_upstream() {
		let (_td, repo) = repo_init();

		// repo_init() already creates an initial commit on master
		// Get the current HEAD commit
		let head = repo.head().unwrap();
		let local_commit = head.target().unwrap();

		// Set up scenario:
		// - Local master is at local_commit (latest)
		// - origin/master exists at local_commit (fully synced - upstream)
		// - backup/master exists at old_commit (behind/different)
		// - Branch tracks origin/master as upstream
		// - We push to "backup" remote
		// - Expected: remote SHA should be old_commit
		// - Bug (before fix): remote SHA was from upstream origin/master

		// Create origin/master tracking branch (at same commit as local)
		repo.reference(
			"refs/remotes/origin/master",
			local_commit,
			true,
			"create origin/master",
		)
		.unwrap();

		// Create backup/master at a different commit (simulating it's behind)
		// We can't create a reference to a non-existent commit, so we'll
		// create an actual old commit first
		let sig = repo.signature().unwrap();
		let tree_id = {
			let mut index = repo.index().unwrap();
			index.write_tree().unwrap()
		};
		let tree = repo.find_tree(tree_id).unwrap();
		let old_commit = repo
			.commit(
				None, // Don't update any refs
				&sig,
				&sig,
				"old backup commit",
				&tree,
				&[], // No parents
			)
			.unwrap();

		// Now create backup/master pointing to this old commit
		repo.reference(
			"refs/remotes/backup/master",
			old_commit,
			true,
			"create backup/master at old commit",
		)
		.unwrap();

		// Configure branch.master.remote and branch.master.merge to set upstream
		{
			let mut config = repo.config().unwrap();
			config.set_str("branch.master.remote", "origin").unwrap();
			config
				.set_str("branch.master.merge", "refs/heads/master")
				.unwrap();
		}

		// Hook that extracts and prints the remote SHA
		let hook = format!(
			r#"#!/bin/sh
stdin_content=$(cat)
echo "stdin: $stdin_content"

# Extract the 4th field (remote SHA)
remote_sha=$(echo "$stdin_content" | awk '{{print $4}}')
echo "remote_sha=$remote_sha"

# When pushing to backup, we should get backup/master's old SHA
# NOT the SHA from origin/master upstream
if [ "$remote_sha" = "{}" ]; then
    echo "BUG: Got origin/master SHA (upstream) instead of backup/master SHA" >&2
    exit 1
fi

if [ "$remote_sha" = "{}" ]; then
    echo "SUCCESS: Got correct backup/master SHA"
    exit 0
fi

echo "ERROR: Got unexpected SHA: $remote_sha" >&2
echo "Expected backup/master: {}" >&2
echo "Or origin/master (bug): {}" >&2
exit 1
"#,
			local_commit, old_commit, old_commit, local_commit
		);

		create_hook(&repo, HOOK_PRE_PUSH, hook.as_bytes());

		// Push to "backup" remote (which doesn't have master yet)
		// This is different from the upstream "origin"
		let branch = head_branch(&repo);
		let updates = [branch_update(
			&repo,
			Some("backup"),
			&branch,
			None,
			false,
		)];

		let res = hooks_pre_push(
			&repo,
			None,
			Some("backup"),
			"https://github.com/user/backup-repo.git",
			&updates,
		)
		.unwrap();

		let HookResult::Run(response) = res else {
			panic!("Expected Run result, got: {res:?}")
		};

		// This test now passes after fix
		assert!(
			response.is_successful(),
			"Hook should receive backup/master SHA, not upstream origin/master SHA.\nstdout: {}\nstderr: {}",
			response.stdout,
			response.stderr
		);
	}
}
