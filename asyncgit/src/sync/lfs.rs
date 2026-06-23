use std::{fs::File, io::Read, path::Path};

use git2::Repository;
use git_lfs_filter::{clean, smudge_object_to};
use git_lfs_pointer::Pointer;
use git_lfs_store::Store;

use crate::error::{Error, Result};

/// Whether a specific path is subject to the LFS `filter` attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LfsFilter {
	/// The `filter` attribute is absent or set to something other than `lfs`.
	Passthrough,
	/// The `filter` attribute IS `lfs`
	Lfs,
}

impl LfsFilter {
	/// Returns `true` if this path should be handled by the LFS filter.
	pub const fn is_lfs(self) -> bool {
		matches!(self, Self::Lfs)
	}
}

/// The outcome of smudging a single working-tree file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SmudgeOutcome {
	/// The file was not an LFS pointer (or is not LFS-tracked); unchanged.
	Skipped,
	/// The pointer's object was not in the local store; left as a pointer.
	ObjectMissing,
	/// The file was smudged successfully; pointer replaced with real content.
	Restored,
}

/// Return the local LFS object store for `repo` (`.git/lfs`).
pub fn lfs_store(repo: &Repository) -> Store {
	Store::new(repo.path().join("lfs"))
}

/// Resolve the LFS filter attribute for `path` (relative to the repo root).
///
/// This delegates to libgit2's attribute lookup and returns an [`LfsFilter`]
/// variant so downstream code can pattern-match instead of inspecting strings
/// or booleans.
pub fn lfs_filter_for(repo: &Repository, path: &Path) -> LfsFilter {
	match repo.get_attr(path, "filter", git2::AttrCheckFlags::empty())
	{
		Ok(Some("lfs")) => LfsFilter::Lfs,
		_ => LfsFilter::Passthrough,
	}
}

/// Apply the LFS *clean* filter to `file_path`
///
/// The real bytes are written to `.git/lfs/objects/`; keeping the  index entry holding
/// just the small pointer text.
pub fn large_file_storage_clean_and_stage(
	repository: &Repository,
	index: &mut git2::Index,
	file_path: &Path,
) -> Result<()> {
	let working_directory =
		repository.workdir().ok_or(Error::NoWorkDir)?;
	let absolute_path = working_directory.join(file_path);

	let mut file = File::open(&absolute_path)?;
	let store = lfs_store(repository);

	// Extract the path string once to avoid repeating the unwrap logic later
	let path_string = file_path.to_str().unwrap_or("");
	let mut pointer_bytes = Vec::new();

	// Produce the canonical pointer text into `pointer_bytes`.
	clean(&store, &mut file, &mut pointer_bytes, path_string, &[])?;

	let mode = file_mode_for(&absolute_path);

	let entry =
		create_index_entry(mode, pointer_bytes.len(), path_string)?;

	index.add_frombuffer(&entry, &pointer_bytes)?;

	Ok(())
}

fn create_index_entry(
	mode: git2::FileMode,
	size: usize,
	path: &str,
) -> Result<git2::IndexEntry> {
	/// The lower 12 bits of the `flags` field in a git index entry store the
	/// path length (capped at this mask value). (<https://git-scm.com/docs/index-format>)
	const GIT_INDEX_ENTRY_NAMEMASK: u16 = 0x0FFF;

	Ok(git2::IndexEntry {
		ctime: git2::IndexTime::new(0, 0),
		mtime: git2::IndexTime::new(0, 0),
		dev: 0,
		ino: 0,
		mode: u32::from(mode),
		uid: 0,
		gid: 0,
		file_size: u32::try_from(size).map_err(|_| {
			Error::Generic("pointer too large".into())
		})?,
		id: git2::Oid::ZERO_SHA1,
		flags: u16::try_from(path.len())
			.unwrap_or(GIT_INDEX_ENTRY_NAMEMASK)
			& GIT_INDEX_ENTRY_NAMEMASK,
		flags_extended: 0,
		path: path.as_bytes().to_vec(),
	})
}

/// Determine the git file mode for an on-disk path.
#[allow(clippy::missing_const_for_fn)] // not const everywhere
fn file_mode_for(path: &Path) -> git2::FileMode {
	#[cfg(unix)]
	{
		use std::os::unix::fs::PermissionsExt;
		let perms = match path.metadata() {
			Ok(meta) => meta.permissions().mode(),
			Err(_) => return git2::FileMode::Blob,
		};
		if perms & 0o111 != 0 {
			git2::FileMode::BlobExecutable
		} else {
			git2::FileMode::Blob
		}
	}
	#[cfg(not(unix))]
	{
		let _ = path;
		git2::FileMode::Blob
	}
}

pub fn needs_lfs_cleaning(
	repository: &Repository,
	index: &git2::Index,
	file_path: &Path,
) -> bool {
	if !lfs_filter_for(repository, file_path).is_lfs() {
		return false;
	}

	let Some(index_entry) = index.get_path(file_path, 0) else {
		return false;
	};

	// Use the ODB to read just the object header. This avoids loading
	// potentially huge blobs entirely into memory just to check their size.
	let Ok(odb) = repository.odb() else {
		return false;
	};
	let Ok((size, _)) = odb.read_header(index_entry.id) else {
		return false;
	};

	if size >= git_lfs_pointer::MAX_POINTER_SIZE {
		// It's too big to be a pointer, so it must be raw data that needs cleaning
		return true;
	}

	let Ok(file_blob) = repository.find_blob(index_entry.id) else {
		return false;
	};

	let content = file_blob.content();

	// 3. Check if it's already a valid pointer
	let is_valid_pointer =
		git_lfs_pointer::Pointer::parse(content).is_ok();

	// If it's NOT a valid pointer, it's raw data that needs cleaning
	!is_valid_pointer
}

/// Walk `tree` and smudge every LFS pointer file in the working tree.
///
/// Returns `Ok(())` even when individual files cannot be smudged, so a partial LFS store only warns.
pub fn large_file_storage_smudge_tree(
	repository: &Repository,
	target_tree: &git2::Tree,
	previous_tree: Option<&git2::Tree>,
) -> Result<()> {
	let working_directory =
		repository.workdir().ok_or(Error::NoWorkDir)?;
	let store = lfs_store(repository);

	let Some(previous) = previous_tree else {
		smudge_tree_entries(
			repository,
			target_tree,
			working_directory,
			&store,
			Path::new(""),
		)?;
		return Ok(());
	};

	// Only smudge files that changed between previous and target tree.
	let difference = repository.diff_tree_to_tree(
		Some(previous),
		Some(target_tree),
		None,
	)?;

	let changed_file_paths = difference
		.deltas()
		.filter(|delta| {
			matches!(
				delta.status(),
				git2::Delta::Added
					| git2::Delta::Modified
					| git2::Delta::Copied
					| git2::Delta::Renamed
			)
		})
		.filter_map(|delta| delta.new_file().path());

	// Process only the filtered paths
	for file_path in changed_file_paths {
		// Staging will clean content into the local LFS store
		// Checkout will smudge pointer files into their OG content
		if lfs_filter_for(repository, file_path).is_lfs() {
			let full_path = working_directory.join(file_path);
			let outcome =
				smudge_file(&store, full_path.as_path(), file_path);

			log_smudge_outcome(file_path, outcome);
		}
	}

	Ok(())
}

fn log_smudge_outcome(
	entry_path: &Path,
	outcome: Result<SmudgeOutcome>,
) {
	match outcome {
		Ok(SmudgeOutcome::Restored) => {
			log::debug!("lfs smudge: restored {entry_path:?}");
		}
		Ok(SmudgeOutcome::ObjectMissing) => {
			log::info!(
				"lfs smudge: object not in local store for \
				 {entry_path:?}, left as pointer"
			);
		}
		Ok(SmudgeOutcome::Skipped) => {}
		Err(e) => {
			log::warn!(
				"lfs smudge: failed to smudge {entry_path:?}: {e}"
			);
		}
	}
}

fn smudge_tree_entries(
	repository: &Repository,
	tree: &git2::Tree,
	working_directory: &Path,
	store: &Store,
	prefix: &Path,
) -> Result<()> {
	for entry in tree {
		let name = entry.name().unwrap_or("");
		let entry_path = prefix.join(name);

		match entry.kind() {
			Some(git2::ObjectType::Tree) => {
				let subtree =
					entry.to_object(repository)?.peel_to_tree()?;

				smudge_tree_entries(
					repository,
					&subtree,
					working_directory,
					store,
					entry_path.as_path(),
				)?;
			}
			// Use a match guard to gracefully filter out non-LFS blobs immediately
			Some(git2::ObjectType::Blob)
				if lfs_filter_for(
					repository,
					entry_path.as_path(),
				)
				.is_lfs() =>
			{
				let full_path = working_directory.join(&entry_path);
				let outcome = smudge_file(
					store,
					full_path.as_path(),
					entry_path.as_path(),
				);

				log_smudge_outcome(entry_path.as_path(), outcome);
			}
			_ => {}
		}
	}
	Ok(())
}

/// Smudge a single file: replace its pointer content with real bytes.
fn smudge_file(
	store: &Store,
	file_path: &Path,
	logical_path: &Path,
) -> Result<SmudgeOutcome> {
	let Some(pointer) = parse_lfs_pointer_from_file(file_path)?
	else {
		return Ok(SmudgeOutcome::Skipped);
	};

	if !store.contains_with_size(pointer.oid, pointer.size) {
		return Ok(SmudgeOutcome::ObjectMissing);
	}

	replace_file_with_smudged_content(
		store,
		&pointer,
		file_path,
		logical_path,
	)?;

	Ok(SmudgeOutcome::Restored)
}

fn parse_lfs_pointer_from_file(
	file_path: &Path,
) -> Result<Option<Pointer>> {
	let mut head = vec![0u8; git_lfs_pointer::MAX_POINTER_SIZE];
	let mut file = File::open(file_path)?;

	let bytes_read = fill_buffer(&mut file, &mut head)?;
	head.truncate(bytes_read);

	if bytes_read >= git_lfs_pointer::MAX_POINTER_SIZE {
		// File is larger than any possible pointer — not LFS.
		return Ok(None);
	}

	Ok(Pointer::parse(&head).ok())
}

fn replace_file_with_smudged_content(
	store: &Store,
	pointer: &Pointer,
	file_path: &Path,
	logical_path: &Path,
) -> Result<()> {
	let parent = file_path.parent().unwrap_or_else(|| Path::new("."));
	let mut temp_file = tempfile::NamedTempFile::new_in(parent)?;

	let path_hint = logical_path.to_str().unwrap_or("");
	smudge_object_to(
		store,
		pointer,
		temp_file.as_file_mut(),
		path_hint,
		&[],
		None,
	)?;

	temp_file.persist(file_path)?;

	Ok(())
}

/// Fill `buf` from `reader`, stopping at EOF or when the buffer is full.
/// Returns the number of bytes actually read.
fn fill_buffer(
	reader: &mut impl Read,
	buf: &mut [u8],
) -> Result<usize> {
	let mut filled = 0;
	while filled < buf.len() {
		match reader.read(&mut buf[filled..])? {
			0 => break,
			n => filled += n,
		}
	}
	Ok(filled)
}

#[cfg(test)]
mod tests {
	use super::*;
	use git_lfs_filter::clean;
	use git_lfs_store::Store;
	use tempfile::TempDir;

	fn tmp_store() -> (TempDir, Store) {
		let td = TempDir::new().unwrap();
		let store = Store::new(td.path().join("lfs"));
		(td, store)
	}

	#[test]
	fn lfs_filter_passthrough_is_not_lfs() {
		assert!(!LfsFilter::Passthrough.is_lfs());
	}

	#[test]
	fn lfs_filter_lfs_is_lfs() {
		assert!(LfsFilter::Lfs.is_lfs());
	}

	/// Cleaning non-pointer content stores the bytes and produces a pointer.
	#[test]
	fn clean_content_stores_and_produces_pointer() {
		let (_td, store) = tmp_store();

		let content = b"large binary payload";
		let mut out = Vec::new();
		let outcome =
			clean(&store, &mut &content[..], &mut out, "", &[])
				.unwrap();

		// The pointer text should be in `out`.
		let pointer_text = String::from_utf8(out).unwrap();
		assert!(
			pointer_text
				.starts_with("version https://git-lfs.github.com"),
			"unexpected pointer text: {pointer_text:?}"
		);
		assert!(
			outcome.pointer().size == content.len() as u64,
			"size mismatch"
		);
		// The object must be stored locally.
		assert!(store.contains(outcome.pointer().oid));
	}

	/// Cleaning an already-canonical pointer is a passthrough — no new store
	/// entry is created.
	#[test]
	fn clean_existing_pointer_is_passthrough() {
		let (_td, store) = tmp_store();

		// First, create a valid pointer via a clean pass.
		let content = b"some content";
		let mut pointer_bytes = Vec::new();
		clean(&store, &mut &content[..], &mut pointer_bytes, "", &[])
			.unwrap();

		// Clean the pointer itself — must pass through unchanged.
		let mut out2 = Vec::new();
		let outcome2 = clean(
			&store,
			&mut pointer_bytes.as_slice(),
			&mut out2,
			"",
			&[],
		)
		.unwrap();
		assert!(
			outcome2.was_passthrough(),
			"expected Passthrough, got {:?}",
			outcome2
		);
		assert_eq!(out2, pointer_bytes);
	}

	/// A written pointer file that has its object in the local store is
	/// replaced with the real content.
	#[test]
	fn smudge_file_restores_content() {
		let (_td, store) = tmp_store();

		let content = b"restored content here";

		// Put the content in the store and obtain the pointer.
		let mut pointer_bytes = Vec::new();
		clean(&store, &mut &content[..], &mut pointer_bytes, "", &[])
			.unwrap();

		// Write the pointer to a temp file (simulating a git2 checkout).
		let dir = TempDir::new().unwrap();
		let file_path = dir.path().join("file.bin");
		std::fs::write(&file_path, &pointer_bytes).unwrap();

		let outcome =
			smudge_file(&store, &file_path, Path::new("file.bin"))
				.unwrap();

		assert_eq!(
			outcome,
			SmudgeOutcome::Restored,
			"expected Restored"
		);

		let on_disk = std::fs::read(&file_path).unwrap();
		assert_eq!(on_disk, content);
	}

	/// When the object is not in the store, `smudge_file` returns
	/// `ObjectMissing` and leaves the pointer file untouched.
	#[test]
	fn smudge_file_missing_object_leaves_pointer() {
		let (_td, store) = tmp_store();

		// Build a valid pointer that references an OID we never store.
		let content = b"this content is never stored";
		let mut pointer_bytes = Vec::new();
		clean(&store, &mut &content[..], &mut pointer_bytes, "", &[])
			.unwrap();
		let oid = git_lfs_pointer::Pointer::parse(&pointer_bytes)
			.unwrap()
			.oid;
		// Remove the just-stored object to simulate a missing download.
		std::fs::remove_file(store.object_path(oid)).unwrap();

		let dir = TempDir::new().unwrap();
		let file_path = dir.path().join("missing.bin");
		std::fs::write(&file_path, &pointer_bytes).unwrap();

		let outcome =
			smudge_file(&store, &file_path, Path::new("missing.bin"))
				.unwrap();

		assert_eq!(outcome, SmudgeOutcome::ObjectMissing);
		// Pointer file must still be there and unchanged.
		assert_eq!(std::fs::read(&file_path).unwrap(), pointer_bytes);
	}

	/// Non-pointer data is skipped entirely.
	#[test]
	fn smudge_file_non_pointer_is_skipped() {
		let (_td, store) = tmp_store();

		let dir = TempDir::new().unwrap();
		let file_path = dir.path().join("normal.txt");
		std::fs::write(&file_path, b"just a plain text file")
			.unwrap();

		let outcome =
			smudge_file(&store, &file_path, Path::new("normal.txt"))
				.unwrap();

		assert_eq!(outcome, SmudgeOutcome::Skipped);
	}
}
