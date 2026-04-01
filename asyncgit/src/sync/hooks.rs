use super::{repository::repo, RepoPath};
use crate::{
	error::Result,
	sync::{
		branch::get_branch_upstream_merge,
		config::{
			push_default_strategy_config_repo,
			PushDefaultStrategyConfig,
		},
		remotes::{proxy_auto, tags::tags_missing_remote, Callbacks},
	},
};
use git2::{BranchType, Direction, Oid};
pub use git2_hooks::{PrePushRef, PrepareCommitMsgSource};
use scopetime::scope_time;
use std::collections::HashMap;

///
#[derive(Debug, PartialEq, Eq)]
pub enum HookResult {
	/// Everything went fine
	Ok,
	/// Hook returned error
	NotOk(String),
}

impl From<git2_hooks::HookResult> for HookResult {
	fn from(v: git2_hooks::HookResult) -> Self {
		match v {
			git2_hooks::HookResult::NoHookFound => Self::Ok,
			git2_hooks::HookResult::Run(response) => {
				if response.is_successful() {
					Self::Ok
				} else {
					Self::NotOk(if response.stderr.is_empty() {
						response.stdout
					} else if response.stdout.is_empty() {
						response.stderr
					} else {
						format!(
							"{}\n{}",
							response.stdout, response.stderr
						)
					})
				}
			}
		}
	}
}

/// Retrieve advertised refs from the remote for the upcoming push.
fn advertised_remote_refs(
	repo_path: &RepoPath,
	remote: Option<&str>,
	url: &str,
	basic_credential: Option<crate::sync::cred::BasicAuthCredential>,
) -> Result<HashMap<String, Oid>> {
	let repo = repo(repo_path)?;
	let mut remote_handle = if let Some(name) = remote {
		repo.find_remote(name)?
	} else {
		repo.remote_anonymous(url)?
	};

	let callbacks = Callbacks::new(None, basic_credential);
	let conn = remote_handle.connect_auth(
		Direction::Push,
		Some(callbacks.callbacks()),
		Some(proxy_auto()),
	)?;

	let mut map = HashMap::new();
	for head in conn.list()? {
		map.insert(head.name().to_string(), head.oid());
	}

	Ok(map)
}

/// Determine the remote ref name for a branch push.
///
/// Respects `push.default=upstream` config when set and upstream is configured.
/// Otherwise defaults to `refs/heads/{branch}`. Delete operations always use
/// the simple ref name.
fn get_remote_ref_for_push(
	repo_path: &RepoPath,
	branch: &str,
	delete: bool,
) -> Result<String> {
	// For delete operations, always use the simple ref name
	// regardless of push.default configuration
	if delete {
		return Ok(format!("refs/heads/{branch}"));
	}

	let repo = repo(repo_path)?;
	let push_default_strategy =
		push_default_strategy_config_repo(&repo)?;

	// When push.default=upstream, use the configured upstream ref if available
	if push_default_strategy == PushDefaultStrategyConfig::Upstream {
		if let Ok(Some(upstream_ref)) =
			get_branch_upstream_merge(repo_path, branch)
		{
			return Ok(upstream_ref);
		}
		// If upstream strategy is set but no upstream is configured,
		// fall through to default behavior
	}

	// Default: push to remote branch with same name as local
	Ok(format!("refs/heads/{branch}"))
}

/// see `git2_hooks::hooks_commit_msg`
pub fn hooks_commit_msg(
	repo_path: &RepoPath,
	msg: &mut String,
) -> Result<HookResult> {
	scope_time!("hooks_commit_msg");

	let repo = repo(repo_path)?;

	Ok(git2_hooks::hooks_commit_msg(&repo, None, msg)?.into())
}

/// see `git2_hooks::hooks_pre_commit`
pub fn hooks_pre_commit(repo_path: &RepoPath) -> Result<HookResult> {
	scope_time!("hooks_pre_commit");

	let repo = repo(repo_path)?;

	Ok(git2_hooks::hooks_pre_commit(&repo, None)?.into())
}

/// see `git2_hooks::hooks_post_commit`
pub fn hooks_post_commit(repo_path: &RepoPath) -> Result<HookResult> {
	scope_time!("hooks_post_commit");

	let repo = repo(repo_path)?;

	Ok(git2_hooks::hooks_post_commit(&repo, None)?.into())
}

/// see `git2_hooks::hooks_prepare_commit_msg`
pub fn hooks_prepare_commit_msg(
	repo_path: &RepoPath,
	source: PrepareCommitMsgSource,
	msg: &mut String,
) -> Result<HookResult> {
	scope_time!("hooks_prepare_commit_msg");

	let repo = repo(repo_path)?;

	Ok(git2_hooks::hooks_prepare_commit_msg(
		&repo, None, source, msg,
	)?
	.into())
}

/// see `git2_hooks::hooks_pre_push`
pub fn hooks_pre_push(
	repo_path: &RepoPath,
	remote: &str,
	push: &PrePushTarget<'_>,
	basic_credential: Option<crate::sync::cred::BasicAuthCredential>,
) -> Result<HookResult> {
	scope_time!("hooks_pre_push");

	let repo = repo(repo_path)?;
	if !git2_hooks::hook_available(
		&repo,
		None,
		git2_hooks::HOOK_PRE_PUSH,
	)? {
		return Ok(HookResult::Ok);
	}

	let git_remote = repo.find_remote(remote)?;
	let url = git_remote
		.pushurl()
		.or_else(|| git_remote.url())
		.ok_or_else(|| {
			crate::error::Error::Generic(format!(
				"remote '{remote}' has no URL configured"
			))
		})?
		.to_string();

	let advertised = advertised_remote_refs(
		repo_path,
		Some(remote),
		&url,
		basic_credential,
	)?;
	let updates = match push {
		PrePushTarget::Branch { branch, delete } => {
			let remote_ref =
				get_remote_ref_for_push(repo_path, branch, *delete)?;
			vec![pre_push_branch_update(
				repo_path,
				branch,
				&remote_ref,
				*delete,
				&advertised,
			)?]
		}
		PrePushTarget::Tags => {
			pre_push_tag_updates(repo_path, remote, &advertised)?
		}
	};

	Ok(git2_hooks::hooks_pre_push(
		&repo,
		None,
		Some(remote),
		&url,
		&updates,
	)?
	.into())
}

/// Build a single pre-push update line for a branch.
fn pre_push_branch_update(
	repo_path: &RepoPath,
	branch_name: &str,
	remote_ref: &str,
	delete: bool,
	advertised: &HashMap<String, Oid>,
) -> Result<PrePushRef> {
	let repo = repo(repo_path)?;
	let local_ref = format!("refs/heads/{branch_name}");
	let local_oid = (!delete)
		.then(|| {
			repo.find_branch(branch_name, BranchType::Local)
				.ok()
				.and_then(|branch| branch.get().peel_to_commit().ok())
				.map(|commit| commit.id())
		})
		.flatten();

	let remote_oid = advertised.get(remote_ref).copied();

	Ok(PrePushRef::new(
		local_ref, local_oid, remote_ref, remote_oid,
	))
}

/// Build pre-push updates for tags that are missing on the remote.
fn pre_push_tag_updates(
	repo_path: &RepoPath,
	remote: &str,
	advertised: &HashMap<String, Oid>,
) -> Result<Vec<PrePushRef>> {
	let repo = repo(repo_path)?;
	let tags = tags_missing_remote(repo_path, remote, None)?;
	let mut updates = Vec::with_capacity(tags.len());

	for tag_ref in tags {
		if let Ok(reference) = repo.find_reference(&tag_ref) {
			let tag_oid = reference.target().or_else(|| {
				reference.peel_to_commit().ok().map(|c| c.id())
			});
			let remote_ref = tag_ref.clone();
			let advertised_oid = advertised.get(&remote_ref).copied();
			updates.push(PrePushRef::new(
				tag_ref.clone(),
				tag_oid,
				remote_ref,
				advertised_oid,
			));
		}
	}

	Ok(updates)
}

/// What is being pushed.
pub enum PrePushTarget<'a> {
	/// Push a single branch.
	Branch {
		/// Local branch name being pushed.
		branch: &'a str,
		/// Whether this is a delete push.
		delete: bool,
	},
	/// Push tags.
	Tags,
}

#[cfg(test)]
mod tests {
	use std::{ffi::OsString, io::Write as _, path::Path};

	use git2::Repository;
	use tempfile::TempDir;

	use super::*;
	use crate::sync::tests::repo_init_with_prefix;

	fn repo_init() -> Result<(TempDir, Repository)> {
		let mut os_string: OsString = OsString::new();

		os_string.push("gitui $# ' ");

		#[cfg(target_os = "linux")]
		{
			use std::os::unix::ffi::OsStrExt;

			const INVALID_UTF8: &[u8] = b"\xED\xA0\x80";

			os_string.push(std::ffi::OsStr::from_bytes(INVALID_UTF8));

			assert!(os_string.to_str().is_none());
		}

		os_string.push(" ");

		repo_init_with_prefix(os_string)
	}

	fn create_hook_in_path(path: &Path, hook_script: &[u8]) {
		std::fs::File::create(path)
			.unwrap()
			.write_all(hook_script)
			.unwrap();

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

	#[test]
	fn test_post_commit_hook_reject_in_subfolder() {
		let (_td, repo) = repo_init().unwrap();
		let root = repo.workdir().unwrap();

		let hook = b"#!/bin/sh
	echo 'rejected'
	exit 1
			";

		git2_hooks::create_hook(
			&repo,
			git2_hooks::HOOK_POST_COMMIT,
			hook,
		);

		let subfolder = root.join("foo/");
		std::fs::create_dir_all(&subfolder).unwrap();

		let res = hooks_post_commit(&subfolder.into()).unwrap();

		assert_eq!(
			res,
			HookResult::NotOk(String::from("rejected\n"))
		);
	}

	// make sure we run the hooks with the correct pwd.
	// for non-bare repos this is the dir of the worktree
	// unfortunately does not work on windows
	#[test]
	#[cfg(unix)]
	fn test_pre_commit_workdir() {
		let (_td, repo) = repo_init().unwrap();
		let root = repo.workdir().unwrap();
		let repo_path: &RepoPath = &root.to_path_buf().into();

		let hook = b"#!/bin/sh
	echo \"$(pwd)\"
	exit 1
		";
		git2_hooks::create_hook(
			&repo,
			git2_hooks::HOOK_PRE_COMMIT,
			hook,
		);
		let res = hooks_pre_commit(repo_path).unwrap();
		if let HookResult::NotOk(res) = res {
			assert_eq!(
				res.trim_end().trim_end_matches('/'),
				// TODO: fix if output isn't utf8.
				root.to_string_lossy().trim_end_matches('/'),
			);
		} else {
			assert!(false);
		}
	}

	#[test]
	fn test_hooks_commit_msg_reject_in_subfolder() {
		let (_td, repo) = repo_init().unwrap();
		let root = repo.workdir().unwrap();

		let hook = b"#!/bin/sh
	echo 'msg' > \"$1\"
	echo 'rejected'
	exit 1
		";

		git2_hooks::create_hook(
			&repo,
			git2_hooks::HOOK_COMMIT_MSG,
			hook,
		);

		let subfolder = root.join("foo/");
		std::fs::create_dir_all(&subfolder).unwrap();

		let mut msg = String::from("test");
		let res =
			hooks_commit_msg(&subfolder.into(), &mut msg).unwrap();

		assert_eq!(
			res,
			HookResult::NotOk(String::from("rejected\n"))
		);

		assert_eq!(msg, String::from("msg\n"));
	}

	#[test]
	fn test_hooks_commit_msg_reject_in_hooks_folder_githooks_moved_absolute(
	) {
		let (_td, repo) = repo_init().unwrap();
		let root = repo.workdir().unwrap();
		let mut config = repo.config().unwrap();

		const HOOKS_DIR: &str = "my_hooks";
		config.set_str("core.hooksPath", HOOKS_DIR).unwrap();

		let hook = b"#!/bin/sh
	echo 'msg' > \"$1\"
	echo 'rejected'
	exit 1
	        ";
		let hooks_folder = root.join(HOOKS_DIR);
		std::fs::create_dir_all(&hooks_folder).unwrap();
		create_hook_in_path(&hooks_folder.join("commit-msg"), hook);

		let mut msg = String::from("test");
		let res =
			hooks_commit_msg(&hooks_folder.into(), &mut msg).unwrap();
		assert_eq!(
			res,
			HookResult::NotOk(String::from("rejected\n"))
		);

		assert_eq!(msg, String::from("msg\n"));
	}

	#[test]
	fn test_pre_push_hook_rejects_based_on_stdin() {
		let (_td, repo) = repo_init().unwrap();

		let hook = b"#!/bin/sh
cat
exit 1
        ";

		git2_hooks::create_hook(
			&repo,
			git2_hooks::HOOK_PRE_PUSH,
			hook,
		);

		let commit_id = repo.head().unwrap().target().unwrap();
		let update = git2_hooks::PrePushRef::new(
			"refs/heads/master",
			Some(commit_id),
			"refs/heads/master",
			None,
		);

		let expected_stdin =
			git2_hooks::PrePushRef::to_stdin(&[update.clone()]);

		let res = git2_hooks::hooks_pre_push(
			&repo,
			None,
			Some("origin"),
			"https://github.com/test/repo.git",
			&[update],
		)
		.unwrap();

		let git2_hooks::HookResult::Run(response) = res else {
			panic!("Expected Run result");
		};
		assert!(!response.is_successful());
		assert_eq!(response.stdout, expected_stdin);
		assert!(expected_stdin.contains("refs/heads/master"));
	}
}
