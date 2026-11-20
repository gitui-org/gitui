use super::{
	commit_files::get_commit_diff, get_stashes, repository::repo,
	CommitId, RepoPath,
};
use crate::error::Result;

/// Text-line totals relative to the first parent (or an empty tree).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CommitStats {
	/// Added text lines; binary changes do not contribute.
	pub insertions: usize,
	/// Removed text lines; binary changes do not contribute.
	pub deletions: usize,
	/// Whether to explain the first-parent comparison in commit Info.
	pub is_merge: bool,
}

/// Get shortstats using the same diff, including untracked stash files,
/// as commit inspection. Whitespace changes count; unchanged renames do not.
pub fn get_commit_stats(
	repo_path: &RepoPath,
	id: CommitId,
) -> Result<CommitStats> {
	let repo = repo(repo_path)?;
	let commit = repo.find_commit(id.into())?;
	// The inspection diff tolerates missing parents. Stats must not mistake
	// an unavailable parent object or tree for a genuine root commit.
	if commit.parent_count() > 0 {
		commit.parent(0)?.tree()?;
	}
	let stashes = get_stashes(repo_path)?.into_iter().collect();
	let mut diff =
		get_commit_diff(&repo, id, None, None, Some(&stashes))?;
	diff.find_similar(Some(
		git2::DiffFindOptions::new().renames(true),
	))?;
	let stats = diff.stats()?;
	Ok(CommitStats {
		insertions: stats.insertions(),
		deletions: stats.deletions(),
		is_merge: commit.parent_count() > 1 && !stashes.contains(&id),
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::sync::{
		get_commit_details, stage_add_file, stash_save,
		tests::{repo_init, repo_init_empty, write_commit_file},
	};
	use git2::{ObjectType, Repository};
	use std::{fs, path::Path};

	fn make_commit(
		repo: &Repository,
		files: &[(&str, &[u8])],
		parents: &[CommitId],
	) -> CommitId {
		let mut builder = repo.treebuilder(None).unwrap();
		for (name, content) in files {
			builder
				.insert(*name, repo.blob(content).unwrap(), 0o100644)
				.unwrap();
		}
		let tree = repo.find_tree(builder.write().unwrap()).unwrap();
		let parents: Vec<_> = parents
			.iter()
			.map(|id| repo.find_commit((*id).into()).unwrap())
			.collect();
		let sig = repo.signature().unwrap();
		repo.commit(
			None,
			&sig,
			&sig,
			"stats test",
			&tree,
			&parents.iter().collect::<Vec<_>>(),
		)
		.unwrap()
		.into()
	}

	fn check(
		repo: &Repository,
		id: CommitId,
		insertions: usize,
		deletions: usize,
		is_merge: bool,
	) {
		let path = repo.path().to_path_buf().into();
		assert_eq!(
			get_commit_stats(&path, id).unwrap(),
			CommitStats {
				insertions,
				deletions,
				is_merge
			}
		);
	}

	#[test]
	fn root_ordinary_empty_and_whitespace() {
		let (_dir, repo) = repo_init_empty().unwrap();
		let root =
			make_commit(&repo, &[("file", b"one\ntwo\n")], &[]);
		check(&repo, root, 2, 0, false);
		let ordinary = make_commit(
			&repo,
			&[("file", b"one\nthree\nfour\n")],
			&[root],
		);
		check(&repo, ordinary, 2, 1, false);
		let empty = make_commit(
			&repo,
			&[("file", b"one\nthree\nfour\n")],
			&[ordinary],
		);
		check(&repo, empty, 0, 0, false);
		let whitespace = make_commit(
			&repo,
			&[("file", b"one \nthree\nfour\n")],
			&[empty],
		);
		check(&repo, whitespace, 1, 1, false);
		let empty_root = make_commit(&repo, &[], &[]);
		check(&repo, empty_root, 0, 0, false);
	}

	#[test]
	fn binary_changes_have_no_text_counts() {
		let (_dir, repo) = repo_init_empty().unwrap();
		let root = make_commit(&repo, &[("binary", b"a\0b\n")], &[]);
		check(&repo, root, 0, 0, false);
		let changed =
			make_commit(&repo, &[("binary", b"c\0d\n")], &[root]);
		check(&repo, changed, 0, 0, false);
		let deleted = make_commit(&repo, &[], &[changed]);
		check(&repo, deleted, 0, 0, false);
	}

	#[test]
	fn renamed_files_do_not_inflate_counts() {
		let (_dir, repo) = repo_init_empty().unwrap();
		let root = make_commit(
			&repo,
			&[("before", b"one\ntwo\nthree\nfour\n")],
			&[],
		);
		let renamed = make_commit(
			&repo,
			&[("after", b"one\ntwo\nthree\nfour\n")],
			&[root],
		);
		check(&repo, renamed, 0, 0, false);
		let edited = make_commit(
			&repo,
			&[("again", b"one\ntwo\nthree\nfive\n")],
			&[renamed],
		);
		check(&repo, edited, 1, 1, false);
	}

	#[test]
	fn merge_and_octopus_use_only_first_parent() {
		let (_dir, repo) = repo_init_empty().unwrap();
		let first =
			make_commit(&repo, &[("file", b"one\ntwo\n")], &[]);
		let second =
			make_commit(&repo, &[("other", b"unrelated\n")], &[]);
		let third = make_commit(
			&repo,
			&[("third", b"not stash untracked\n")],
			&[],
		);
		for parents in
			[vec![first, second], vec![first, second, third]]
		{
			let merge = make_commit(
				&repo,
				&[("file", b"one\nmerged\nextra\n")],
				&parents,
			);
			check(&repo, merge, 2, 1, true);
		}
	}

	#[test]
	fn stash_includes_staged_unstaged_and_untracked() {
		let (dir, repo) = repo_init().unwrap();
		let path = dir.path().to_path_buf().into();
		write_commit_file(&repo, "tracked", "old\nkeep\n", "base");
		fs::write(dir.path().join("tracked"), "new\nkeep\n").unwrap();
		stage_add_file(&path, Path::new("tracked")).unwrap();
		fs::write(dir.path().join("tracked"), "new\nkeep\nextra\n")
			.unwrap();
		fs::write(dir.path().join("untracked"), "u1\nu2\n").unwrap();
		let stash = stash_save(&path, None, true, false).unwrap();
		check(&repo, stash, 4, 1, false);
		fs::write(dir.path().join("tracked"), "new\nkeep\n").unwrap();
		let tracked_only =
			stash_save(&path, None, false, false).unwrap();
		check(&repo, tracked_only, 1, 1, false);
	}

	#[test]
	fn missing_parent_or_parent_tree_is_unavailable_but_metadata_survives(
	) {
		let (_dir, repo) = repo_init_empty().unwrap();
		let root =
			make_commit(&repo, &[("file", b"one\ntwo\n")], &[]);
		let tree = repo.find_commit(root.into()).unwrap().tree_id();
		let missing = "ffffffffffffffffffffffffffffffffffffffff";
		let broken_parent = repo.odb().unwrap().write(ObjectType::Commit, format!("tree {missing}\nauthor A <a@b> 1 +0000\ncommitter A <a@b> 1 +0000\n\nparent\n").as_bytes()).unwrap();
		for parent in [missing.to_string(), broken_parent.to_string()]
		{
			let id = repo.odb().unwrap().write(ObjectType::Commit, format!("tree {tree}\nparent {parent}\nauthor A <a@b> 1 +0000\ncommitter A <a@b> 1 +0000\n\nchild\n").as_bytes()).unwrap().into();
			let path = repo.path().to_path_buf().into();
			assert!(get_commit_stats(&path, id).is_err());
			assert_eq!(
				get_commit_details(&path, id)
					.unwrap()
					.message
					.unwrap()
					.subject,
				"child"
			);
		}
	}
}
