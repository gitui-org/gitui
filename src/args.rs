use crate::bug_report;
use anyhow::{anyhow, Context, Result};
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
const UPDATE_NIGHTLY_FLAG_ID: &str = "nightly";
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
	let app = app();

	let arg_matches = app.get_matches();

	if arg_matches.get_flag(BUG_REPORT_FLAG_ID) {
		bug_report::generate_bugreport();
		std::process::exit(0);
	}

	// Handle update subcommand
	if let Some(update_cmd) = arg_matches.subcommand_matches("update")
	{
		let include_prerelease =
			update_cmd.get_flag(UPDATE_NIGHTLY_FLAG_ID);
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
	let gitdir =
		arg_matches.get_one::<String>(GIT_DIR_FLAG_ID).map_or_else(
			|| PathBuf::from(DEFAULT_GIT_DIR),
			PathBuf::from,
		);

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
		format!(
			"failed to create config directory: {}",
			confpath.display()
		)
	})?;
	let theme = confpath.join(arg_theme);

	let notify_watcher: bool =
		*arg_matches.get_one(WATCHER_FLAG_ID).unwrap_or(&false);

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
				.help("Store logging output into a file (in the cache directory by default)")
				.short('l')
				.long("logging")
                .default_value_if("logfile", ArgPredicate::IsPresent, "true")
				.action(clap::ArgAction::SetTrue),
		)
        .arg(Arg::new(LOG_FILE_FLAG_ID)
            .help("Store logging output into the specified file (implies --logging)")
            .long("logfile")
            .value_name("LOG_FILE"))
		.arg(
			Arg::new(WATCHER_FLAG_ID)
				.help("Use notify-based file system watcher instead of tick-based update. This is more performant, but can cause issues on some platforms. See https://github.com/gitui-org/gitui/blob/master/FAQ.md#watcher for details.")
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
					Arg::new(UPDATE_NIGHTLY_FLAG_ID)
						.help("Allow updating to pre-release versions (nightly, rc, beta, dev)")
						.short('n')
						.long("nightly")
						.action(clap::ArgAction::SetTrue),
				),
		)
}

/// Represents the installation method of gitui
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum InstallMethod {
	Cargo,
	Homebrew,
	Apt,
	Dnf,
	Pacman,
	Windows,
	Scoop,
	Chocolatey,
	ScoopBucket,
	Unknown,
}

impl std::fmt::Display for InstallMethod {
	fn fmt(
		&self,
		f: &mut std::fmt::Formatter<'_>,
	) -> std::fmt::Result {
		match self {
			InstallMethod::Cargo => write!(f, "cargo"),
			InstallMethod::Homebrew => write!(f, "homebrew"),
			InstallMethod::Apt => write!(f, "apt"),
			InstallMethod::Dnf => write!(f, "dnf"),
			InstallMethod::Pacman => write!(f, "pacman"),
			InstallMethod::Windows => write!(f, "windows"),
			InstallMethod::Scoop => write!(f, "scoop"),
			InstallMethod::Chocolatey => write!(f, "chocolatey"),
			InstallMethod::ScoopBucket => write!(f, "scoop-bucket"),
			InstallMethod::Unknown => write!(f, "unknown"),
		}
	}
}

/// Detect how gitui was installed
fn detect_install_method() -> InstallMethod {
	use std::path::Path;

	let current_exe = std::env::current_exe().ok();
	let exe_path = current_exe.as_ref().map(|p| p.as_path());

	// Check if running from cargo install or cargo build
	let is_cargo_build = if let Some(path) = &exe_path {
		let path_str = path.to_string_lossy();
		path_str.contains(".cargo/bin")
			|| path_str.contains("cargo/registry")
			|| path_str.contains("target/release")
			|| path_str.contains("target/debug")
	} else {
		false
	};

	// Even if running from cargo build, check if there's a system-installed gitui
	// that the user might want to update instead
	if is_cargo_build {
		// Check if there's a dnf-installed gitui in the system
		if Path::new("/usr/bin/dnf").exists()
			|| Path::new("/usr/bin/rpm").exists()
		{
			if is_installed_via_dnf() {
				// There's a dnf-installed gitui - prefer updating that
				return InstallMethod::Dnf;
			}
		}
		// Check for apt-installed gitui
		if Path::new("/usr/bin/apt").exists()
			|| Path::new("/usr/bin/dpkg").exists()
		{
			if is_installed_via_apt() {
				return InstallMethod::Apt;
			}
		}
		// Check for pacman-installed gitui
		if Path::new("/usr/bin/pacman").exists() {
			if is_installed_via_pacman() {
				return InstallMethod::Pacman;
			}
		}
		// No system installation found, use cargo
		return InstallMethod::Cargo;
	}

	// Check for homebrew (macOS/Linux)
	if let Some(path) = &exe_path {
		let path_str = path.to_string_lossy();
		if path_str.contains("homebrew")
			|| path_str.contains("Cellar")
		{
			return InstallMethod::Homebrew;
		}
	}

	// Check for Windows package managers
	if let Some(path) = &exe_path {
		let path_str = path.to_string_lossy();
		if path_str.contains("scoop") {
			if path_str.contains("scoop-bucket") {
				return InstallMethod::ScoopBucket;
			}
			return InstallMethod::Scoop;
		}
		if path_str.contains("chocolatey") {
			return InstallMethod::Chocolatey;
		}
		// Generic Windows binary
		if cfg!(target_os = "windows") {
			return InstallMethod::Windows;
		}
	}

	// Check for Linux package managers
	if let Some(path) = &exe_path {
		let path_str = path.to_string_lossy();

		// Check various package manager paths
		if path_str.contains("/usr/bin")
			|| path_str.contains("/usr/local/bin")
		{
			// Could be APT, DNF, or Pacman - try to detect via package managers
			if Path::new("/usr/bin/apt").exists()
				|| Path::new("/usr/bin/dpkg").exists()
			{
				// Check if installed via apt
				if is_installed_via_apt() {
					return InstallMethod::Apt;
				}
			}
			if Path::new("/usr/bin/dnf").exists()
				|| Path::new("/usr/bin/rpm").exists()
			{
				// Check if installed via dnf
				if is_installed_via_dnf() {
					return InstallMethod::Dnf;
				}
			}
			if Path::new("/usr/bin/pacman").exists() {
				// Check if installed via pacman
				if is_installed_via_pacman() {
					return InstallMethod::Pacman;
				}
			}
		}
	}

	InstallMethod::Unknown
}

#[cfg(target_os = "linux")]
fn is_installed_via_apt() -> bool {
	use std::process::Command;
	Command::new("dpkg")
		.args(["-l", "gitui"])
		.output()
		.map(|output| output.status.success())
		.unwrap_or(false)
}

#[cfg(not(target_os = "linux"))]
fn is_installed_via_apt() -> bool {
	false
}

#[cfg(target_os = "linux")]
fn is_installed_via_dnf() -> bool {
	use std::process::Command;
	Command::new("rpm")
		.args(["-q", "gitui"])
		.output()
		.map(|output| output.status.success())
		.unwrap_or(false)
}

#[cfg(not(target_os = "linux"))]
fn is_installed_via_dnf() -> bool {
	false
}

#[cfg(target_os = "linux")]
fn is_installed_via_pacman() -> bool {
	use std::process::Command;
	Command::new("pacman")
		.args(["-Q", "gitui"])
		.output()
		.map(|output| output.status.success())
		.unwrap_or(false)
}

#[cfg(not(target_os = "linux"))]
fn is_installed_via_pacman() -> bool {
	false
}

/// Fetch the latest version from GitHub releases
fn fetch_latest_version() -> Option<String> {
	use std::process::Command;

	// Try to use git to check the latest tag
	let output = Command::new("git")
		.args([
			"ls-remote",
			"--tags",
			"--sort=-v:refname",
			"https://github.com/extrawurst/gitui.git",
		])
		.output()
		.ok()?;

	if output.status.success() {
		let stdout = String::from_utf8_lossy(&output.stdout);
		// Parse the first tag line
		for line in stdout.lines() {
			// Format: <sha>\trefs/tags/<tag>
			if let Some(tag_part) = line.split('\t').nth(1) {
				if let Some(tag) = tag_part.strip_prefix("refs/tags/")
				{
					// Extract just the version number (e.g., "v0.28.0" -> "0.28.0")
					let version =
						tag.trim_start_matches('v').to_string();
					return Some(version);
				}
			}
		}
	}

	None
}

/// Fetch the latest stable version (non pre-release)
fn fetch_latest_stable_version() -> Option<String> {
	use std::process::Command;

	// Fetch more tags to find a stable one
	let output = Command::new("git")
		.args([
			"ls-remote",
			"--tags",
			"--sort=-v:refname",
			"https://github.com/extrawurst/gitui.git",
		])
		.output()
		.ok()?;

	if output.status.success() {
		let stdout = String::from_utf8_lossy(&output.stdout);
		// Parse all tag lines and find the first stable one
		for line in stdout.lines() {
			// Format: <sha>\trefs/tags/<tag>
			if let Some(tag_part) = line.split('\t').nth(1) {
				if let Some(tag) = tag_part.strip_prefix("refs/tags/")
				{
					let version =
						tag.trim_start_matches('v').to_string();
					// Skip pre-release versions
					if !is_prerelease(&version) {
						return Some(version);
					}
				}
			}
		}
	}

	// Fallback: if no stable version found in first batch, return None
	// In production, you'd want to query the GitHub API for releases
	println!(
		"Warning: Could not find a stable release in recent tags."
	);
	println!("Consider using --update-nightly to update to a pre-release version.");
	None
}

/// Get the current gitui version
fn get_current_version() -> String {
	// env!("GITUI_BUILD_NAME") contains version info like "0.28.1"
	// Extract just the version number
	let build_name = env!("GITUI_BUILD_NAME");
	build_name
		.split_whitespace()
		.next()
		.unwrap_or(build_name)
		.to_string()
}

/// Check if a version is a pre-release (contains nightly, rc, beta, alpha, dev)
fn is_prerelease(version: &str) -> bool {
	let version_lower = version.to_lowercase();
	version_lower.contains("nightly")
		|| version_lower.contains("-rc")
		|| version_lower.contains("-beta")
		|| version_lower.contains("-alpha")
		|| version_lower.contains("-dev")
		|| version_lower.contains("preview")
		|| version_lower.contains("snapshot")
}

/// Perform self-update based on installation method
fn self_update(include_prerelease: bool) -> Result<()> {
	let current_version = get_current_version();
	let install_method = detect_install_method();

	println!("gitui version: {}", current_version);

	// Warn if on a pre-release version
	if is_prerelease(&current_version) {
		println!("⚠️  Warning: You are running a pre-release version ({}).", current_version);
		if !include_prerelease {
			println!("   Use 'gitui update -n' to include pre-releases.");
			println!("   Or use 'gitui update' to switch to the latest stable version.");
		}
	}

	println!("Installation method: {}", install_method);

	// Check for updates
	println!("Checking for updates...");
	let latest_version = fetch_latest_version();

	let latest_version = match latest_version {
		Some(latest) => {
			let is_latest_prerelease = is_prerelease(&latest);

			if !include_prerelease && is_latest_prerelease {
				// Find the latest stable version instead
				println!(
					"Latest pre-release found: {} (use --update-nightly to upgrade)",
					latest
				);
				// For now, we don't have a way to find the latest stable from git tags alone
				// In production, you'd query the GitHub API for releases
				println!("Searching for latest stable version...");
				// Try to find a stable version by fetching more tags
				fetch_latest_stable_version()
			} else {
				Some(latest)
			}
		}
		None => {
			println!("Could not check for latest version. Proceeding with update anyway...");
			None
		}
	};

	match &latest_version {
		Some(latest) => {
			if latest == &current_version {
				println!(
					"You're already up to date! ({})",
					current_version
				);
				return Ok(());
			}

			let is_latest_prerelease = is_prerelease(latest);
			if is_latest_prerelease {
				println!(
					"⚠️  Pre-release update available: {} -> {}",
					current_version, latest
				);
			} else {
				println!(
					"Update available: {} -> {}",
					current_version, latest
				);
			}
		}
		None => {
			println!("Could not determine latest version.");
		}
	}

	// Confirm update
	print!("Do you want to update gitui? [y/N]: ");
	use std::io::{self, Write};
	io::stdout().flush()?;

	let mut input = String::new();
	io::stdin().read_line(&mut input)?;

	if !input.trim().eq_ignore_ascii_case("y") {
		println!("Update cancelled.");
		return Ok(());
	}

	println!("Updating gitui via {}...", install_method);

	// Perform the update based on installation method
	let result = match install_method {
		InstallMethod::Cargo => update_via_cargo(),
		InstallMethod::Homebrew => update_via_homebrew(),
		InstallMethod::Dnf => update_via_dnf(),
		InstallMethod::Apt => update_via_apt(),
		InstallMethod::Pacman => update_via_pacman(),
		InstallMethod::Scoop => update_via_scoop(),
		InstallMethod::Chocolatey => update_via_chocolatey(),
		InstallMethod::ScoopBucket => update_via_scoop_bucket(),
		InstallMethod::Windows => {
			Err("Windows binary update not supported. Please download the latest release from GitHub.".to_string())
		}
		InstallMethod::Unknown => {
			Err("Could not detect installation method. Please update manually.".to_string())
		}
	};

	match result {
		Ok(_) => {
			println!("Update complete! Please restart gitui.");
			Ok(())
		}
		Err(e) => Err(anyhow!("Update failed: {}", e)),
	}
}

fn update_via_cargo() -> Result<(), String> {
	use std::process::Command;

	println!("Running: cargo install gitui --force");

	let output = Command::new("cargo")
		.args(["install", "gitui", "--force"])
		.output()
		.map_err(|e| format!("Failed to run cargo install: {}", e))?;

	if output.status.success() {
		println!("Successfully updated via cargo!");
		Ok(())
	} else {
		let stderr = String::from_utf8_lossy(&output.stderr);
		Err(format!("Cargo install failed:\n{}", stderr))
	}
}

#[cfg(target_os = "macos")]
fn update_via_homebrew() -> Result<(), String> {
	use std::process::Command;

	println!("Running: brew upgrade gitui");

	let output = Command::new("brew")
		.args(["upgrade", "gitui"])
		.output()
		.map_err(|e| format!("Failed to run brew upgrade: {}", e))?;

	if output.status.success() {
		println!("Successfully updated via homebrew!");
		Ok(())
	} else {
		let stderr = String::from_utf8_lossy(&output.stderr);
		// Check if already up to date
		if stderr.contains("already installed") {
			println!("Already up to date!");
			Ok(())
		} else {
			Err(format!("Brew upgrade failed:\n{}", stderr))
		}
	}
}

#[cfg(not(target_os = "macos"))]
fn update_via_homebrew() -> Result<(), String> {
	Err("Homebrew is only supported on macOS".to_string())
}

fn update_via_dnf() -> Result<(), String> {
	use std::process::Command;

	println!("Running: sudo dnf upgrade gitui -y");

	let output = Command::new("sudo")
		.args(["dnf", "upgrade", "gitui", "-y"])
		.output()
		.map_err(|e| format!("Failed to run dnf upgrade: {}", e))?;

	if output.status.success() {
		println!("Successfully updated via dnf!");
		Ok(())
	} else {
		let stderr = String::from_utf8_lossy(&output.stderr);
		Err(format!("DNF upgrade failed:\n{}", stderr))
	}
}

fn update_via_apt() -> Result<(), String> {
	use std::process::Command;

	println!("Running: sudo apt update && sudo apt upgrade gitui -y");

	// First update package list
	let _ = Command::new("sudo").args(["apt", "update"]).output();

	let output = Command::new("sudo")
		.args(["apt", "upgrade", "gitui", "-y"])
		.output()
		.map_err(|e| format!("Failed to run apt upgrade: {}", e))?;

	if output.status.success() {
		println!("Successfully updated via apt!");
		Ok(())
	} else {
		let stderr = String::from_utf8_lossy(&output.stderr);
		Err(format!("APT upgrade failed:\n{}", stderr))
	}
}

fn update_via_pacman() -> Result<(), String> {
	use std::process::Command;

	println!("Running: sudo pacman -Syu gitui --noconfirm");

	let output = Command::new("sudo")
		.args(["pacman", "-Syu", "gitui", "--noconfirm"])
		.output()
		.map_err(|e| format!("Failed to run pacman -Syu: {}", e))?;

	if output.status.success() {
		println!("Successfully updated via pacman!");
		Ok(())
	} else {
		let stderr = String::from_utf8_lossy(&output.stderr);
		Err(format!("Pacman upgrade failed:\n{}", stderr))
	}
}

#[cfg(target_os = "windows")]
fn update_via_scoop() -> Result<(), String> {
	use std::process::Command;

	println!("Running: scoop update gitui");

	let output = Command::new("scoop")
		.args(["update", "gitui"])
		.output()
		.map_err(|e| format!("Failed to run scoop update: {}", e))?;

	if output.status.success() {
		println!("Successfully updated via scoop!");
		Ok(())
	} else {
		let stderr = String::from_utf8_lossy(&output.stderr);
		Err(format!("Scoop update failed:\n{}", stderr))
	}
}

#[cfg(not(target_os = "windows"))]
fn update_via_scoop() -> Result<(), String> {
	Err("Scoop is only supported on Windows".to_string())
}

#[cfg(target_os = "windows")]
fn update_via_scoop_bucket() -> Result<(), String> {
	use std::process::Command;

	println!("Running: scoop update gitui (from bucket)");

	let output = Command::new("scoop")
		.args(["update", "gitui"])
		.output()
		.map_err(|e| format!("Failed to run scoop update: {}", e))?;

	if output.status.success() {
		println!("Successfully updated via scoop bucket!");
		Ok(())
	} else {
		let stderr = String::from_utf8_lossy(&output.stderr);
		Err(format!("Scoop bucket update failed:\n{}", stderr))
	}
}

#[cfg(not(target_os = "windows"))]
fn update_via_scoop_bucket() -> Result<(), String> {
	Err("Scoop is only supported on Windows".to_string())
}

#[cfg(target_os = "windows")]
fn update_via_chocolatey() -> Result<(), String> {
	use std::process::Command;

	println!("Running: choco upgrade gitui -y");

	let output = Command::new("choco")
		.args(["upgrade", "gitui", "-y"])
		.output()
		.map_err(|e| format!("Failed to run choco upgrade: {}", e))?;

	if output.status.success() {
		println!("Successfully updated via chocolatey!");
		Ok(())
	} else {
		let stderr = String::from_utf8_lossy(&output.stderr);
		Err(format!("Chocolatey upgrade failed:\n{}", stderr))
	}
}

#[cfg(not(target_os = "windows"))]
fn update_via_chocolatey() -> Result<(), String> {
	Err("Chocolatey is only supported on Windows".to_string())
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
	fs::create_dir_all(&path).with_context(|| {
		format!(
			"failed to create cache directory: {}",
			path.display()
		)
	})?;
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
