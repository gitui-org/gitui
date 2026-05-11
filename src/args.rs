use crate::bug_report;
use crate::update::self_update;
use anyhow::{Context, Result};
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

const BUG_REPORT_FLAG_ID: &str = "bugreport";
const LOG_FILE_FLAG_ID: &str = "logfile";
const LOGGING_FLAG_ID: &str = "logging";
const THEME_FLAG_ID: &str = "theme";
const WORKDIR_FLAG_ID: &str = "workdir";
const FILE_FLAG_ID: &str = "file";
const GIT_DIR_FLAG_ID: &str = "directory";
const WATCHER_FLAG_ID: &str = "watcher";
const KEY_BINDINGS_FLAG_ID: &str = "key_bindings";
const KEY_SYMBOLS_FLAG_ID: &str = "key_symbols";
const DEFAULT_THEME: &str = "theme.ron";
const DEFAULT_GIT_DIR: &str = ".";

#[derive(Clone)]
pub struct CliArgs {
	pub theme: PathBuf,
	pub select_file: Option<PathBuf>,
	pub repo_path: RepoPath,
	pub notify_watcher: bool,
	pub key_bindings_path: Option<PathBuf>,
	pub key_symbols_path: Option<PathBuf>,
}

pub fn process_cmdline() -> Result<CliArgs> {
	let arg_matches = app().get_matches();

	if arg_matches.get_flag(BUG_REPORT_FLAG_ID) {
		bug_report::generate_bugreport();
		std::process::exit(0);
	}

	if let Some(update_cmd) = arg_matches.subcommand_matches("update") {
		let include_prerelease = update_cmd.get_flag("nightly");
		if let Err(e) = self_update(include_prerelease) {
			eprintln!("Update failed: {}", e);
			std::process::exit(1);
		}
		std::process::exit(0);
	}

	if arg_matches.get_flag(LOGGING_FLAG_ID) {
		let logfile = arg_matches.get_one::<String>(LOG_FILE_FLAG_ID);
		setup_logging(logfile.map(PathBuf::from))?;
	}

	let workdir = arg_matches
		.get_one::<String>(WORKDIR_FLAG_ID)
		.map(PathBuf::from);
	let gitdir = arg_matches
		.get_one::<String>(GIT_DIR_FLAG_ID)
		.map_or_else(|| PathBuf::from(DEFAULT_GIT_DIR), PathBuf::from);

	let select_file = arg_matches
		.get_one::<String>(FILE_FLAG_ID)
		.map(PathBuf::from);

	let repo_path = if let Some(w) = workdir {
		RepoPath::Workdir { gitdir, workdir: w }
	} else {
		RepoPath::Path(gitdir)
	};

	let arg_theme = arg_matches
		.get_one::<String>(THEME_FLAG_ID)
		.map_or_else(|| PathBuf::from(DEFAULT_THEME), PathBuf::from);

	let confpath = get_app_config_path()?;
	fs::create_dir_all(&confpath).with_context(|| {
		format!("failed to create config directory: {}", confpath.display())
	})?;
	let theme = confpath.join(arg_theme);

	let notify_watcher = *arg_matches.get_one(WATCHER_FLAG_ID).unwrap_or(&false);

	let key_bindings_path = arg_matches
		.get_one::<String>(KEY_BINDINGS_FLAG_ID)
		.map(PathBuf::from);

	let key_symbols_path = arg_matches
		.get_one::<String>(KEY_SYMBOLS_FLAG_ID)
		.map(PathBuf::from);

	Ok(CliArgs {
		theme,
		select_file,
		repo_path,
		notify_watcher,
		key_bindings_path,
		key_symbols_path,
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
			Arg::new(KEY_BINDINGS_FLAG_ID)
				.help("Use a custom keybindings file")
				.short('k')
				.long("key-bindings")
				.value_name("KEY_LIST_FILENAME")
				.num_args(1),
		)
		.arg(
			Arg::new(KEY_SYMBOLS_FLAG_ID)
				.help("Use a custom symbols file")
				.short('s')
				.long("key-symbols")
				.value_name("KEY_SYMBOLS_FILENAME")
				.num_args(1),
		)
		.arg(
			Arg::new(THEME_FLAG_ID)
				.help("Set color theme filename loaded from config directory")
				.short('t')
				.long("theme")
				.value_name("THEME_FILE")
				.default_value(DEFAULT_THEME)
				.num_args(1),
		)
		.arg(
			Arg::new(LOGGING_FLAG_ID)
				.help("Store logging output into a file")
				.short('l')
				.long("logging")
				.default_value_if(LOG_FILE_FLAG_ID, ArgPredicate::IsPresent, "true")
				.action(clap::ArgAction::SetTrue),
		)
		.arg(
			Arg::new(LOG_FILE_FLAG_ID)
				.help("Store logging output into the specified file")
				.long("logfile")
				.value_name("LOG_FILE"),
		)
		.arg(
			Arg::new(WATCHER_FLAG_ID)
				.help("Use notify-based file system watcher")
				.long("watcher")
				.action(clap::ArgAction::SetTrue),
		)
		.arg(
			Arg::new(BUG_REPORT_FLAG_ID)
				.help("Generate a bug report")
				.long("bugreport")
				.action(clap::ArgAction::SetTrue),
		)
		.arg(
			Arg::new(FILE_FLAG_ID)
				.help("Select the file in the file tab")
				.short('f')
				.long("file")
				.num_args(1),
		)
		.arg(
			Arg::new(GIT_DIR_FLAG_ID)
				.help("Set the git directory")
				.short('d')
				.long("directory")
				.env("GIT_DIR")
				.num_args(1),
		)
		.arg(
			Arg::new(WORKDIR_FLAG_ID)
				.help("Set the working directory")
				.short('w')
				.long("workdir")
				.env("GIT_WORK_TREE")
				.num_args(1),
		)
		.subcommand(
			ClapApp::new("update")
				.about("Update gitui to the latest version")
				.visible_short_flag_alias('U')
				.arg(
					Arg::new("nightly")
						.help("Include pre-release versions")
						.short('n')
						.long("nightly")
						.action(clap::ArgAction::SetTrue),
				),
		)
}

fn setup_logging(path_override: Option<PathBuf>) -> Result<()> {
	let path = path_override.unwrap_or_else(|| {
		let mut p = dirs::cache_dir().expect("cache dir");
		p.push("gitui");
		p.push("gitui.log");
		p
	});

	println!("Logging enabled. Log written to: {}", path.display());
	WriteLogger::init(LevelFilter::Trace, Config::default(), File::create(path)?)?;
	Ok(())
}

pub fn get_app_config_path() -> Result<PathBuf> {
	let mut path = if cfg!(target_os = "macos") {
		dirs::home_dir().map(|h| h.join(".config"))
	} else {
		dirs::config_dir()
	}
	.ok_or_else(|| anyhow::anyhow!("failed to find os config dir."))?;

	path.push("gitui");
	Ok(path)
}

#[test]
fn verify_app() {
	app().debug_assert();
}
