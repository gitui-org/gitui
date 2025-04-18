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
	fs::create_dir_all(&confpath)?;
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

fn get_app_cache_path() -> Result<PathBuf> {
	let mut path = dirs::cache_dir()
		.ok_or_else(|| anyhow!("failed to find os cache dir."))?;

	path.push("gitui");
	fs::create_dir_all(&path)?;
	Ok(path)
}

pub fn get_app_config_path() -> Result<PathBuf> {
	let mut path = if cfg!(target_os = "macos") {
		dirs::home_dir().map(|h| h.join(".config"))
	} else {
		dirs::config_dir()
	}
	.ok_or_else(|| anyhow!("failed to find os config dir."))?;

	path.push("gitui");
	Ok(path)
}

#[test]
fn verify_app() {
	app().debug_assert();
}
