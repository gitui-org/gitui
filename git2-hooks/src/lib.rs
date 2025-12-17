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

use git2::Repository;

pub const HOOK_POST_COMMIT: &str = "post-commit";
pub const HOOK_PRE_COMMIT: &str = "pre-commit";
pub const HOOK_COMMIT_MSG: &str = "commit-msg";
pub const HOOK_PREPARE_COMMIT_MSG: &str = "prepare-commit-msg";
pub const HOOK_PRE_PUSH: &str = "pre-push";

const HOOK_COMMIT_MSG_TEMP_FILE: &str = "COMMIT_EDITMSG";

/// Response from running a hook
#[derive(Debug, PartialEq, Eq)]
pub struct HookRunResponse {
	/// path of the hook that was run
	pub hook: PathBuf,
	/// stdout output emitted by hook
	pub stdout: String,
	/// stderr output emitted by hook
	pub stderr: String,
	/// exit code as reported back from process calling the hook (None if successful)
	pub code: Option<i32>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum HookResult {
	/// No hook found
	NoHookFound,
	/// Hook executed (check `HookRunResponse.code` for success/failure)
	Run(HookRunResponse),
}

impl HookResult {
	/// helper to check if result is ok (hook succeeded with exit code 0)
	pub const fn is_ok(&self) -> bool {
		match self {
			Self::Run(response) => response.code.is_none(),
			Self::NoHookFound => false,
		}
	}

	/// helper to check if result was run and not successful (non-zero exit code)
	pub const fn is_not_successful(&self) -> bool {
		match self {
			Self::Run(response) => response.code.is_some(),
			Self::NoHookFound => false,
		}
	}
}

impl HookRunResponse {
	/// Check if the hook succeeded (exit code 0)
	pub const fn is_successful(&self) -> bool {
		self.code.is_none()
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
/// Parameters:
/// - `branch_name`: Optional local branch name being pushed (e.g., "main"). If `None`, stdin will be empty.
/// - `remote_branch_name`: Optional remote branch name (if different from local)
pub fn hooks_pre_push(
	repo: &Repository,
	other_paths: Option<&[&str]>,
	remote: Option<&str>,
	url: &str,
	branch_name: Option<&str>,
	remote_branch_name: Option<&str>,
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

	let stdin_data = if let Some(branch) = branch_name {
		// Get local branch reference and commit
		let local_branch =
			repo.find_branch(branch, git2::BranchType::Local)?;
		let local_ref = format!("refs/heads/{branch}");
		let local_commit = local_branch.get().peel_to_commit()?;
		let local_sha = local_commit.id().to_string();

		// Determine remote branch name (same as local if not specified)
		let remote_branch = remote_branch_name.unwrap_or(branch);
		let remote_ref = format!("refs/heads/{remote_branch}");

		// Try to get remote commit SHA from upstream
		// If there's no upstream (new branch), use 40 zeros as per Git spec
		let remote_sha = if let Ok(upstream) = local_branch.upstream()
		{
			upstream.get().peel_to_commit()?.id().to_string()
		} else {
			"0".repeat(40)
		};

		// Format: refs/heads/branch local_sha refs/heads/branch remote_sha\n
		format!("{local_ref} {local_sha} {remote_ref} {remote_sha}\n")
	} else {
		// No branch specified (e.g., pushing tags), use empty stdin
		String::new()
	};

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

		assert_eq!(response.code, Some(42));
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
		assert!(res.is_not_successful());
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

		assert_eq!(response.code.unwrap(), 1);
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
		assert!(res.is_not_successful());
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
		assert!(res.is_not_successful());
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

		assert_eq!(response.code.unwrap(), 1);
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

		assert_eq!(response.code.unwrap(), 2);
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

		let res = hooks_pre_push(
			&repo,
			None,
			Some("origin"),
			"https://example.com/repo.git",
			None,
			None,
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
		let res = hooks_pre_push(
			&repo,
			None,
			Some("origin"),
			"https://example.com/repo.git",
			None,
			None,
		)
		.unwrap();
		let HookResult::Run(response) = res else {
			unreachable!()
		};
		assert_eq!(response.code.unwrap(), 3);
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

		let res = hooks_pre_push(
			&repo,
			None,
			None,
			"https://example.com/repo.git",
			None,
			None,
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

		let res = hooks_pre_push(
			&repo,
			None,
			Some("origin"),
			"https://example.com/repo.git",
			None,
			None,
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
			response.stdout.contains("stdin_length=0"),
			"Expected stdin to be empty (length 0), got: {}",
			response.stdout
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

		let res = hooks_pre_push(
			&repo,
			None,
			Some("origin"),
			"https://github.com/user/repo.git",
			None,
			None,
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
		// Currently we pass empty stdin, this test documents that behavior
		let hook = b"#!/bin/sh
stdin_content=$(cat)
if [ -z \"$stdin_content\" ]; then
    echo \"stdin is empty (expected for current implementation)\"
    exit 0
else
    echo \"stdin_length=${#stdin_content}\"
    echo \"stdin_content=$stdin_content\"
    exit 0
fi
	";

		create_hook(&repo, HOOK_PRE_PUSH, hook);

		let res = hooks_pre_push(
			&repo,
			None,
			Some("origin"),
			"https://github.com/user/repo.git",
			None,
			None,
		)
		.unwrap();

		let HookResult::Run(response) = res else {
			unreachable!("Expected Run result, got: {res:?}")
		};

		assert!(response.is_successful(), "Hook should succeed");

		// Verify stdin is currently empty
		assert!(
			response.stdout.contains("stdin is empty"),
			"Expected stdin to be empty, got: {}",
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

		let res = hooks_pre_push(
			&repo,
			None,
			Some("origin"),
			"https://github.com/user/repo.git",
			Some("master"),
			None,
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
}
