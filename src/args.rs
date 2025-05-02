use crate::bug_report;
use anyhow::{anyhow, Result};
use asyncgit::sync::RepoPath;
use clap::{
	builder::ArgPredicate, crate_authors, crate_description,
	crate_name, Arg, Command as ClapApp,
};
use simplelog::{Config, LevelFilter, WriteLogger};
use std::{
	env,
	fs::{self, File},
	path::PathBuf,
};

pub struct CliArgs {
	pub theme: PathBuf,
	pub repo_path: RepoPath,
	pub notify_watcher: bool,
}

pub fn process_cmdline() -> Result<CliArgs> {
	let app = app();

	let arg_matches = app.get_matches();

	if arg_matches.get_flag("bugreport") {
		bug_report::generate_bugreport();
		std::process::exit(0);
	}
	if arg_matches.get_flag("logging") {
		let logfile = arg_matches.get_one::<String>("logfile");
		setup_logging(logfile.map(PathBuf::from))?;
	}

	let workdir =
		arg_matches.get_one::<String>("workdir").map(PathBuf::from);
	let gitdir = arg_matches
		.get_one::<String>("directory")
		.map_or_else(|| PathBuf::from("."), PathBuf::from);

	let repo_path = if let Some(w) = workdir {
		RepoPath::Workdir { gitdir, workdir: w }
	} else {
		RepoPath::Path(gitdir)
	};

	let arg_theme = arg_matches
		.get_one::<String>("theme")
		.map_or_else(|| PathBuf::from("theme.ron"), PathBuf::from);

	let confpath = get_app_config_path()?;
	let theme = confpath.join(arg_theme);

	let notify_watcher: bool =
		*arg_matches.get_one("watcher").unwrap_or(&false);

	Ok(CliArgs {
		theme,
		repo_path,
		notify_watcher,
	})
}

fn app() -> ClapApp {
	ClapApp::new(crate_name!())
		.author(crate_authors!())
		.version(env!("GITUI_BUILD_NAME"))
		.about(crate_description!())
		.help_template(
			"\
{before-help}gitui {version}
{author}
{about}

{usage-heading} {usage}

{all-args}{after-help}
		",
		)
		.arg(
			Arg::new("theme")
				.help("Set color theme filename loaded from config directory")
				.short('t')
				.long("theme")
				.value_name("THEME_FILE")
				.default_value("theme.ron")
				.num_args(1),
		)
		.arg(
			Arg::new("logging")
				.help("Store logging output into a file (in the cache directory by default)")
				.short('l')
				.long("logging")
                .default_value_if("logfile", ArgPredicate::IsPresent, "true")
				.action(clap::ArgAction::SetTrue),
		)
        .arg(Arg::new("logfile")
            .help("Store logging output into the specified file (implies --logging)")
            .long("logfile")
            .value_name("LOG_FILE"))
		.arg(
			Arg::new("watcher")
				.help("Use notify-based file system watcher instead of tick-based update. This is more performant, but can cause issues on some platforms. See https://github.com/gitui-org/gitui/blob/master/FAQ.md#watcher for details.")
				.long("watcher")
				.action(clap::ArgAction::SetTrue),
		)
		.arg(
			Arg::new("bugreport")
				.help("Generate a bug report")
				.long("bugreport")
				.action(clap::ArgAction::SetTrue),
		)
		.arg(
			Arg::new("directory")
				.help("Set the git directory")
				.short('d')
				.long("directory")
				.env("GIT_DIR")
				.num_args(1),
		)
		.arg(
			Arg::new("workdir")
				.help("Set the working directory")
				.short('w')
				.long("workdir")
				.env("GIT_WORK_TREE")
				.num_args(1),
		)
}

fn setup_logging(path_override: Option<PathBuf>) -> Result<()> {
	let path = if let Some(path) = path_override {
		path
	} else {
		let mut path = get_app_cache_path()?;
		path.push("gitui.log");
		path
	};

	println!("Logging enabled. Log written to: {}", path.display());

	WriteLogger::init(
		LevelFilter::Trace,
		Config::default(),
		File::create(path)?,
	)?;

	Ok(())
}

fn ensure_path_exists(path: Option<PathBuf>) -> Option<PathBuf> {
	path.and_then(|p| {
		if p.is_absolute() && fs::create_dir_all(&p).is_ok() {
			return Some(p);
		}
		None
	})
}

fn get_path_from_candidates(
	candidates: impl IntoIterator<Item = Option<PathBuf>>,
) -> Result<PathBuf> {
	let mut target_dir = None;

	// Filter into existing directories
	for potential_dir in candidates
		.into_iter()
		.flatten()
		.filter(|p| p.is_dir() && p.is_absolute())
	{
		let search_path = potential_dir.join("gitui");

		// Prefer preexisting gitui directory
		if search_path.is_dir() {
			target_dir = Some(search_path);
			break;
		}

		// Fallback to first existing directory
		target_dir.get_or_insert(search_path);
	}

	target_dir.ok_or_else(|| {
		anyhow!("failed to find valid path within candidates")
	})
}

fn get_app_cache_path() -> Result<PathBuf> {
	let cache_dir_candidates = [
		ensure_path_exists(
			env::var_os("XDG_CACHE_HOME").map(PathBuf::from),
		),
		dirs::cache_dir(),
	];

	let cache_dir = get_path_from_candidates(cache_dir_candidates)
		.map_err(|_| anyhow!("failed to find valid cache dir."))?;

	fs::create_dir_all(&cache_dir)?;
	Ok(cache_dir)
}

pub fn get_app_config_path() -> Result<PathBuf> {
	// List of potential config directories in order of priority
	let config_dir_candidates = [
		env::var_os("XDG_CONFIG_HOME").map(PathBuf::from),
		// This is in the list since it was the hardcoded behavior on macos before
		// I expect this to be what most people have XDG_CONFIG_HOME set to already
		// But explicitly including this will avoid breaking anyone's existing config
		dirs::home_dir().map(|p| p.join(".config")),
		dirs::config_dir(),
	];

	get_path_from_candidates(config_dir_candidates)
		.map_err(|_| anyhow!("failed to find valid config dir."))
}

#[cfg(test)]
mod tests {
	use std::fs;

	use super::{app, get_path_from_candidates};
	use tempfile::tempdir;

	#[test]
	fn verify_app() {
		app().debug_assert();
	}

	#[test]
	fn test_config_dir_candidates_from_preexisting() {
		let temp_dummy_1 = tempdir().expect("should create temp dir");
		let temp_dummy_2 = tempdir().expect("should create temp dir");
		let temp_target = tempdir().expect("should create temp dir");
		let temp_goal = temp_target.path().join("gitui");

		fs::create_dir_all(&temp_goal)
			.expect("should create temp target directory");

		let candidates = [
			Some(temp_dummy_1.path().to_path_buf()),
			Some(temp_target.path().to_path_buf()),
			Some(temp_dummy_2.path().to_path_buf()),
		];
		let result = get_path_from_candidates(candidates)
			.expect("should find the included target");
		assert_eq!(result, temp_goal);
	}

	#[test]
	fn test_config_dir_candidates_no_preexisting() {
		let temp_dummy_1 = tempdir().expect("should create temp dir");
		let temp_dummy_2 = tempdir().expect("should create temp dir");

		let candidates = [
			Some(temp_dummy_1.path().to_path_buf()),
			Some(temp_dummy_2.path().to_path_buf()),
		];

		let result = get_path_from_candidates(candidates)
			.expect("should return first candidate");
		assert_eq!(result, temp_dummy_1.path().join("gitui"));
	}
}
