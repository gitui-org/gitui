use super::{utils::work_dir, RepoPath};
use crate::{
	error::{Error, Result},
	sync::repository::repo,
};
use scopetime::scope_time;
use std::{
	fs::{File, OpenOptions},
	io::{Read, Seek, SeekFrom, Write},
	path::Path,
};

static GITIGNORE: &str = ".gitignore";

/// add file or path to root ignore file
pub fn add_to_ignore(
	repo_path: &RepoPath,
	path_to_ignore: &str,
) -> Result<()> {
	scope_time!("add_to_ignore");

	let repo = repo(repo_path)?;

	if Path::new(path_to_ignore).file_name()
		== Path::new(GITIGNORE).file_name()
	{
		return Err(Error::Generic(String::from(
			"cannot ignore gitignore",
		)));
	}

	let ignore_file = work_dir(&repo)?.join(GITIGNORE);

	let optional_newline = ignore_file.exists()
		&& !file_ends_with_newline(&ignore_file)?;

	let mut file = OpenOptions::new()
		.append(true)
		.create(true)
		.open(ignore_file)?;

	writeln!(
		file,
		"{}{}",
		if optional_newline { "\n" } else { "" },
		path_to_ignore
	)?;

	Ok(())
}

fn file_ends_with_newline(file: &Path) -> Result<bool> {
	let mut file = File::open(file)?;
	let size = file.metadata()?.len();

	file.seek(SeekFrom::Start(size.saturating_sub(1)))?;
	let mut last_char = String::with_capacity(1);
	file.read_to_string(&mut last_char)?;

	Ok(last_char == "\n")
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::sync::{tests::repo_init, utils::repo_write_file};
	use io::BufRead;
	use pretty_assertions::assert_eq;
	use std::{fs::File, io, path::Path};

	#[test]
	fn test_empty() -> Result<()> {
		let ignore_file_path = Path::new(".gitignore");
		let file_path = Path::new("foo.txt");
		let (_td, repo) = repo_init()?;
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();

		File::create(root.join(file_path))?.write_all(b"test")?;

		assert_eq!(root.join(ignore_file_path).exists(), false);
		add_to_ignore(repo_path, file_path.to_str().unwrap())?;
		assert_eq!(root.join(ignore_file_path).exists(), true);

		Ok(())
	}

	fn read_lines<P>(
		filename: P,
	) -> io::Result<io::Lines<io::BufReader<File>>>
	where
		P: AsRef<Path>,
	{
		let file = File::open(filename)?;
		Ok(io::BufReader::new(file).lines())
	}

	#[test]
	fn test_append() -> Result<()> {
		let ignore_file_path = Path::new(".gitignore");
		let file_path = Path::new("foo.txt");
		let (_td, repo) = repo_init()?;
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();

		File::create(root.join(file_path))?.write_all(b"test")?;
		File::create(root.join(ignore_file_path))?
			.write_all(b"foo\n")?;

		add_to_ignore(repo_path, file_path.to_str().unwrap())?;

		let mut lines =
			read_lines(root.join(ignore_file_path)).unwrap();
		assert_eq!(&lines.nth(1).unwrap().unwrap(), "foo.txt");

		Ok(())
	}

	#[test]
	fn test_append_no_newline_at_end() -> Result<()> {
		let ignore_file_path = Path::new(".gitignore");
		let file_path = Path::new("foo.txt");
		let (_td, repo) = repo_init()?;
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();

		File::create(root.join(file_path))?.write_all(b"test")?;
		File::create(root.join(ignore_file_path))?
			.write_all(b"foo")?;

		add_to_ignore(repo_path, file_path.to_str().unwrap())?;

		let mut lines =
			read_lines(root.join(ignore_file_path)).unwrap();
		assert_eq!(&lines.nth(1).unwrap().unwrap(), "foo.txt");

		Ok(())
	}

	#[test]
	fn test_ignore_ignore() {
		let ignore_file_path = Path::new(".gitignore");
		let (_td, repo) = repo_init().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();

		repo_write_file(&repo, ".gitignore", "#foo").unwrap();

		let res = add_to_ignore(repo_path, ".gitignore");
		assert!(res.is_err());

		let lines = read_lines(root.join(ignore_file_path)).unwrap();
		assert_eq!(lines.count(), 1);
	}
}
