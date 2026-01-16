use super::{get_commits_info, CommitId, RepoPath};
use crate::{
	error::Result,
	sync::{gix_repo, repository::repo},
};
use scopetime::scope_time;
use std::collections::{BTreeMap, HashMap, HashSet};

///
#[derive(Clone, Hash, PartialEq, Eq, Debug)]
pub struct Tag {
	/// tag name
	pub name: String,
	/// tag annotation
	pub annotation: Option<String>,
}

impl Tag {
	///
	pub fn new(name: &str) -> Self {
		Self {
			name: name.into(),
			annotation: None,
		}
	}
}

/// all tags pointing to a single commit
pub type CommitTags = Vec<Tag>;
/// hashmap of tag target commit hash to tag names
pub type Tags = BTreeMap<CommitId, CommitTags>;

///
pub struct TagWithMetadata {
	///
	pub name: String,
	///
	pub author: String,
	///
	pub time: i64,
	///
	pub message: String,
	///
	pub commit_id: CommitId,
	///
	pub annotation: Option<String>,
}

static MAX_MESSAGE_WIDTH: usize = 100;

/// returns `Tags` type filled with all tags found in repo
pub fn get_tags(repo_path: &RepoPath) -> Result<Tags> {
	scope_time!("get_tags");

	let mut res = Tags::new();
	let mut adder = |key, value: Tag| {
		if let Some(key) = res.get_mut(&key) {
			key.push(value);
		} else {
			res.insert(key, vec![value]);
		}
	};

	let repo: gix::Repository = gix_repo(repo_path)?;
	let platform = repo.references()?;
	for mut reference in (platform.tags()?).flatten() {
		let commit = reference.peel_to_commit();
		let tag = reference.peel_to_tag();

		if let Ok(commit) = commit {
			let tag_ref = tag.as_ref().map(gix::Tag::decode);

			let name = match tag_ref {
				Ok(Ok(tag)) => tag.name.to_string(),
				_ => reference.name().shorten().to_string(),
			};
			let annotation = match tag_ref {
				Ok(Ok(tag)) => Some(tag.message.to_string()),
				_ => None,
			};

			adder(commit.into(), Tag { name, annotation });
		}
	}

	Ok(res)
}

///
pub fn get_tags_with_metadata(
	repo_path: &RepoPath,
) -> Result<Vec<TagWithMetadata>> {
	scope_time!("get_tags_with_metadata");

	let tags_grouped_by_commit_id = get_tags(repo_path)?;

	let tags_with_commit_id: Vec<(&str, Option<&str>, &CommitId)> =
		tags_grouped_by_commit_id
			.iter()
			.flat_map(|(commit_id, tags)| {
				tags.iter()
					.map(|tag| {
						(
							tag.name.as_ref(),
							tag.annotation.as_deref(),
							commit_id,
						)
					})
					.collect::<Vec<_>>()
			})
			.collect();

	let unique_commit_ids: HashSet<_> = tags_with_commit_id
		.iter()
		.copied()
		.map(|(_, _, &commit_id)| commit_id)
		.collect();
	let mut commit_ids = Vec::with_capacity(unique_commit_ids.len());
	commit_ids.extend(unique_commit_ids);

	let commit_infos =
		get_commits_info(repo_path, &commit_ids, MAX_MESSAGE_WIDTH)?;
	let unique_commit_infos: HashMap<_, _> = commit_infos
		.iter()
		.map(|commit_info| (commit_info.id, commit_info))
		.collect();

	let mut tags: Vec<TagWithMetadata> = tags_with_commit_id
		.into_iter()
		.filter_map(|(tag, annotation, commit_id)| {
			unique_commit_infos.get(commit_id).map(|commit_info| {
				TagWithMetadata {
					name: String::from(tag),
					author: commit_info.author.clone(),
					time: commit_info.time,
					message: commit_info.message.clone(),
					commit_id: *commit_id,
					annotation: annotation.map(String::from),
				}
			})
		})
		.collect();

	tags.sort_unstable_by(|a, b| b.time.cmp(&a.time));

	Ok(tags)
}

///
pub fn delete_tag(
	repo_path: &RepoPath,
	tag_name: &str,
) -> Result<()> {
	scope_time!("delete_tag");

	let repo = repo(repo_path)?;
	repo.tag_delete(tag_name)?;

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::sync::tests::repo_init;
	use git2::ObjectType;

	#[test]
	fn test_smoke() {
		let (_td, repo) = repo_init().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();

		assert!(get_tags(repo_path).unwrap().is_empty());
	}

	#[test]
	fn test_multitags() {
		let (_td, repo) = repo_init().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();

		let sig = repo.signature().unwrap();
		let head_id = repo.head().unwrap().target().unwrap();
		let target = repo
			.find_object(
				repo.head().unwrap().target().unwrap(),
				Some(ObjectType::Commit),
			)
			.unwrap();

		repo.tag("a", &target, &sig, "", false).unwrap();
		repo.tag("b", &target, &sig, "", false).unwrap();

		assert_eq!(
			get_tags(repo_path).unwrap()[&CommitId::new(head_id)]
				.iter()
				.map(|t| &t.name)
				.collect::<Vec<_>>(),
			vec!["a", "b"]
		);

		let tags = get_tags_with_metadata(repo_path).unwrap();

		assert_eq!(tags.len(), 2);
		assert_eq!(tags[0].name, "a");
		assert_eq!(tags[0].message, "initial");
		assert_eq!(tags[1].name, "b");
		assert_eq!(tags[1].message, "initial");
		assert_eq!(tags[0].commit_id, tags[1].commit_id);

		delete_tag(repo_path, "a").unwrap();

		let tags = get_tags(repo_path).unwrap();

		assert_eq!(tags.len(), 1);

		delete_tag(repo_path, "b").unwrap();

		let tags = get_tags(repo_path).unwrap();

		assert_eq!(tags.len(), 0);
	}
}
