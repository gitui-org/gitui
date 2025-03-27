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

const HOOK_COMMIT_MSG_TEMP_FILE: &str = "COMMIT_EDITMSG";

#[derive(Debug, PartialEq, Eq)]
pub enum HookResult {
	/// No hook found
	NoHookFound,
	/// Hook executed with non error return code
	Ok {
		/// path of the hook that was run
		hook: PathBuf,
	},
	/// Hook executed and returned an error code
	RunNotSuccessful {
		/// exit code as reported back from process calling the hook
		code: Option<i32>,
		/// stderr output emitted by hook
		stdout: String,
		/// stderr output emitted by hook
		stderr: String,
		/// path of the hook that was run
		hook: PathBuf,
	},
}

impl HookResult {
	/// helper to check if result is ok
	pub const fn is_ok(&self) -> bool {
		matches!(self, Self::Ok { .. })
	}

	/// helper to check if result was run and not rejected
	pub const fn is_not_successful(&self) -> bool {
		matches!(self, Self::RunNotSuccessful { .. })
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

		let HookResult::RunNotSuccessful {
			code,
			stdout,
			stderr,
			hook: h,
		} = result
		else {
			unreachable!("run_hook should've failed");
		};

		let stdout = stdout.as_str().trim_ascii_end();

		assert_eq!(code, Some(42));
		assert_eq!(h, hook.hook);
		assert_eq!(stdout, TEXT, "{:?} != {TEXT:?}", stdout);
		assert!(stderr.is_empty());
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
	fn test_other_path_precendence() {
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

		let HookResult::RunNotSuccessful { stdout, .. } = res else {
			unreachable!()
		};

		assert!(
			stdout
				.lines()
				.any(|line| line.starts_with(PATH_EXPORT)),
			"Could not find line starting with {PATH_EXPORT:?} in: {stdout:?}"
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

		let HookResult::RunNotSuccessful { code, stdout, .. } = res
		else {
			unreachable!()
		};

		assert_eq!(code.unwrap(), 1);
		assert_eq!(&stdout, "rejected\n");
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

		// mirror how python pre-commmit sets itself up
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

		// mirror how python pre-commmit sets itself up
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

		let HookResult::RunNotSuccessful { code, stdout, .. } = res
		else {
			unreachable!()
		};

		assert_eq!(code.unwrap(), 1);
		assert_eq!(&stdout, "rejected\n");

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

		assert!(matches!(res, HookResult::Ok { .. }));
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

		let HookResult::RunNotSuccessful { code, stdout, .. } = res
		else {
			unreachable!()
		};

		assert_eq!(code.unwrap(), 2);
		assert_eq!(&stdout, "rejected\n");

		assert_eq!(
			msg,
			String::from(
				"commit,0000000000000000000000000000000000000000\n"
			)
		);
	}
}
