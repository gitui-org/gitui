use super::{CommitId, RepoPath};
use crate::{
	error::Result,
	sync::{repository::repo, utils::get_head_repo},
};
use git2::{Config, ErrorCode, ObjectType, Repository, Signature};
use scopetime::scope_time;
use ssh_key::{HashAlg, LineEnding, PrivateKey};
use std::path::PathBuf;

///
pub fn amend(
	repo_path: &RepoPath,
	id: CommitId,
	msg: &str,
) -> Result<CommitId> {
	scope_time!("amend");

	let repo = repo(repo_path)?;
	let commit = repo.find_commit(id.into())?;

	let mut index = repo.index()?;
	let tree_id = index.write_tree()?;
	let tree = repo.find_tree(tree_id)?;

	let new_id = commit.amend(
		Some("HEAD"),
		None,
		None,
		None,
		Some(msg),
		Some(&tree),
	)?;

	Ok(CommitId::new(new_id))
}

/// Wrap `Repository::signature` to allow unknown user.name.
///
/// See <https://github.com/extrawurst/gitui/issues/79>.
#[allow(clippy::redundant_pub_crate)]
pub(crate) fn signature_allow_undefined_name(
	repo: &Repository,
) -> std::result::Result<Signature<'_>, git2::Error> {
	let signature = repo.signature();

	if let Err(ref e) = signature {
		if e.code() == ErrorCode::NotFound {
			let config = repo.config()?;

			if let (Err(_), Ok(email_entry)) = (
				config.get_entry("user.name"),
				config.get_entry("user.email"),
			) {
				if let Some(email) = email_entry.value() {
					return Signature::now("unknown", email);
				}
			};
		}
	}

	signature
}

fn fetch_ssh_key(config: &Config) -> Option<PrivateKey> {
	config
		.get_entry("user.signingKey")
		.ok()
		.and_then(|entry| {
			entry.value().map(|key_path| {
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
		})
		.and_then(|key_file| {
			key_file
				.and_then(|mut p| {
					p.set_extension("");
					std::fs::read(p).ok()
				})
				.and_then(|bytes| {
					PrivateKey::from_openssh(bytes).ok()
				})
		})
}

/// this does not run any git hooks, git-hooks have to be executed manually, checkout `hooks_commit_msg` for example
pub fn commit(repo_path: &RepoPath, msg: &str) -> Result<CommitId> {
	scope_time!("commit");

	let repo = repo(repo_path)?;

	let signature = signature_allow_undefined_name(&repo)?;
	let mut index = repo.index()?;
	let tree_id = index.write_tree()?;
	let tree = repo.find_tree(tree_id)?;

	let parents = if let Ok(id) = get_head_repo(&repo) {
		vec![repo.find_commit(id.into())?]
	} else {
		Vec::new()
	};

	let config = repo.config()?;
	let parents = parents.iter().collect::<Vec<_>>();
	let id = match (
		config.get_entry("gpg.format").ok(),
		fetch_ssh_key(&config),
	) {
		(Some(f), Some(sk)) if f.value() == Some("ssh") => {
			let buffer = repo.commit_create_buffer(
				&signature,
				&signature,
				msg,
				&tree,
				parents.as_slice(),
			)?;
			let content = String::from_utf8(buffer.to_vec())?;
			let sig = sk
				.sign("git", HashAlg::Sha256, &buffer)?
				.to_pem(LineEnding::LF)?;
			let commit_id =
				repo.commit_signed(&content, &sig, None)?;
			if let Ok(mut head) = repo.head() {
				head.set_target(commit_id, msg)?;
			} else {
				let default_branch_name = config
					.get_str("init.defaultBranch")
					.unwrap_or("master");
				repo.reference(
					&format!("refs/heads/{default_branch_name}"),
					commit_id,
					true,
					msg,
				)?;
			}
			Ok(commit_id.into())
		}
		_ => Ok(repo
			.commit(
				Some("HEAD"),
				&signature,
				&signature,
				msg,
				&tree,
				parents.as_slice(),
			)?
			.into()),
	};
	id
}

/// Tag a commit.
///
/// This function will return an `Err(…)` variant if the tag’s name is refused
/// by git or if the tag already exists.
pub fn tag_commit(
	repo_path: &RepoPath,
	commit_id: &CommitId,
	tag: &str,
	message: Option<&str>,
) -> Result<CommitId> {
	scope_time!("tag_commit");

	let repo = repo(repo_path)?;

	let object_id = commit_id.get_oid();
	let target =
		repo.find_object(object_id, Some(ObjectType::Commit))?;

	let c = if let Some(message) = message {
		let signature = signature_allow_undefined_name(&repo)?;
		repo.tag(tag, &target, &signature, message, false)?.into()
	} else {
		repo.tag_lightweight(tag, &target, false)?.into()
	};

	Ok(c)
}

#[cfg(test)]
mod tests {
	use crate::error::Result;
	use crate::sync::tags::Tag;
	use crate::sync::RepoPath;
	use crate::sync::{
		commit, get_commit_details, get_commit_files, stage_add_file,
		tags::get_tags,
		tests::{get_statuses, repo_init, repo_init_empty},
		utils::get_head,
		LogWalker,
	};
	use commit::{amend, tag_commit};
	use git2::Repository;
	use std::{fs::File, io::Write, path::Path};

	fn count_commits(repo: &Repository, max: usize) -> usize {
		let mut items = Vec::new();
		let mut walk = LogWalker::new(repo, max).unwrap();
		walk.read(&mut items).unwrap();
		items.len()
	}

	#[test]
	fn test_commit() {
		let file_path = Path::new("foo");
		let (_td, repo) = repo_init().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();

		File::create(root.join(file_path))
			.unwrap()
			.write_all(b"test\nfoo")
			.unwrap();

		assert_eq!(get_statuses(repo_path), (1, 0));

		stage_add_file(repo_path, file_path).unwrap();

		assert_eq!(get_statuses(repo_path), (0, 1));

		commit(repo_path, "commit msg").unwrap();

		assert_eq!(get_statuses(repo_path), (0, 0));
	}

	#[test]
	fn test_commit_in_empty_repo() {
		let file_path = Path::new("foo");
		let (_td, repo) = repo_init_empty().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();

		assert_eq!(get_statuses(repo_path), (0, 0));

		File::create(root.join(file_path))
			.unwrap()
			.write_all(b"test\nfoo")
			.unwrap();

		assert_eq!(get_statuses(repo_path), (1, 0));

		stage_add_file(repo_path, file_path).unwrap();

		assert_eq!(get_statuses(repo_path), (0, 1));

		commit(repo_path, "commit msg").unwrap();

		assert_eq!(get_statuses(repo_path), (0, 0));
	}

	#[test]
	fn test_amend() -> Result<()> {
		let file_path1 = Path::new("foo");
		let file_path2 = Path::new("foo2");
		let (_td, repo) = repo_init_empty()?;
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();

		File::create(root.join(file_path1))?.write_all(b"test1")?;

		stage_add_file(repo_path, file_path1)?;
		let id = commit(repo_path, "commit msg")?;

		assert_eq!(count_commits(&repo, 10), 1);

		File::create(root.join(file_path2))?.write_all(b"test2")?;

		stage_add_file(repo_path, file_path2)?;

		let new_id = amend(repo_path, id, "amended")?;

		assert_eq!(count_commits(&repo, 10), 1);

		let details = get_commit_details(repo_path, new_id)?;
		assert_eq!(details.message.unwrap().subject, "amended");

		let files = get_commit_files(repo_path, new_id, None)?;

		assert_eq!(files.len(), 2);

		let head = get_head(repo_path)?;

		assert_eq!(head, new_id);

		Ok(())
	}

	#[test]
	fn test_tag() -> Result<()> {
		let file_path = Path::new("foo");
		let (_td, repo) = repo_init_empty().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();

		File::create(root.join(file_path))?
			.write_all(b"test\nfoo")?;

		stage_add_file(repo_path, file_path)?;

		let new_id = commit(repo_path, "commit msg")?;

		tag_commit(repo_path, &new_id, "tag", None)?;

		assert_eq!(
			get_tags(repo_path).unwrap()[&new_id],
			vec![Tag::new("tag")]
		);

		assert!(matches!(
			tag_commit(repo_path, &new_id, "tag", None),
			Err(_)
		));

		assert_eq!(
			get_tags(repo_path).unwrap()[&new_id],
			vec![Tag::new("tag")]
		);

		tag_commit(repo_path, &new_id, "second-tag", None)?;

		assert_eq!(
			get_tags(repo_path).unwrap()[&new_id],
			vec![Tag::new("second-tag"), Tag::new("tag")]
		);

		Ok(())
	}

	#[test]
	fn test_tag_with_message() -> Result<()> {
		let file_path = Path::new("foo");
		let (_td, repo) = repo_init_empty().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();

		File::create(root.join(file_path))?
			.write_all(b"test\nfoo")?;

		stage_add_file(repo_path, file_path)?;

		let new_id = commit(repo_path, "commit msg")?;

		tag_commit(repo_path, &new_id, "tag", Some("tag-message"))?;

		assert_eq!(
			get_tags(repo_path).unwrap()[&new_id][0]
				.annotation
				.as_ref()
				.unwrap(),
			"tag-message"
		);

		Ok(())
	}

	/// Beware: this test has to be run with a `$HOME/.gitconfig` that has
	/// `user.email` not set. Otherwise, git falls back to the value of
	/// `user.email` in `$HOME/.gitconfig` and this test fails.
	///
	/// As of February 2021, `repo_init_empty` sets all git config locations
	/// to an empty temporary directory, so this constraint is met.
	#[test]
	fn test_empty_email() -> Result<()> {
		let file_path = Path::new("foo");
		let (_td, repo) = repo_init_empty().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();

		File::create(root.join(file_path))?
			.write_all(b"test\nfoo")?;

		stage_add_file(repo_path, file_path)?;

		repo.config()?.remove("user.email")?;

		let error = commit(repo_path, "commit msg");

		assert!(matches!(error, Err(_)));

		repo.config()?.set_str("user.email", "email")?;

		let success = commit(repo_path, "commit msg");

		assert!(matches!(success, Ok(_)));
		assert_eq!(count_commits(&repo, 10), 1);

		let details =
			get_commit_details(repo_path, success.unwrap()).unwrap();

		assert_eq!(details.author.name, "name");
		assert_eq!(details.author.email, "email");

		Ok(())
	}

	/// See comment to `test_empty_email`.
	#[test]
	fn test_empty_name() -> Result<()> {
		let file_path = Path::new("foo");
		let (_td, repo) = repo_init_empty().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();

		File::create(root.join(file_path))?
			.write_all(b"test\nfoo")?;

		stage_add_file(repo_path, file_path)?;

		repo.config()?.remove("user.name")?;

		let mut success = commit(repo_path, "commit msg");

		assert!(matches!(success, Ok(_)));
		assert_eq!(count_commits(&repo, 10), 1);

		let mut details =
			get_commit_details(repo_path, success.unwrap()).unwrap();

		assert_eq!(details.author.name, "unknown");
		assert_eq!(details.author.email, "email");

		repo.config()?.set_str("user.name", "name")?;

		success = commit(repo_path, "commit msg");

		assert!(matches!(success, Ok(_)));
		assert_eq!(count_commits(&repo, 10), 2);

		details =
			get_commit_details(repo_path, success.unwrap()).unwrap();

		assert_eq!(details.author.name, "name");
		assert_eq!(details.author.email, "email");

		Ok(())
	}
}
