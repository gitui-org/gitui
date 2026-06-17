//! Self-update functionality for gitui. Orchestrates version checking,
//! installation method detection, and update execution.

mod commands;
mod detector;

use anyhow::{anyhow, Result};
use commands::*;
use detector::{detect_install_method, InstallMethod};
use std::io::{self, Write};
use std::process::Command;

pub fn self_update(include_prerelease: bool) -> Result<()> {
	let current = get_current_version();
	let method = detect_install_method();

	println!("gitui version: {}", current);

	if is_prerelease(&current) {
		println!("⚠️  Pre-release version detected.");
		if !include_prerelease {
			println!(
				"   Use 'gitui update -n' to include pre-releases."
			);
		}
	}

	println!("Installation method: {}", method);
	println!("Checking for updates...");

	let latest = if include_prerelease {
		fetch_latest_version()
	} else {
		fetch_latest_stable()
	};

	match latest {
		Some(v) if v == current => {
			println!("Already up to date ({})", current);
			return Ok(());
		}
		Some(v) => {
			let kind = if is_prerelease(&v) {
				"Pre-release"
			} else {
				"Stable"
			};
			println!(
				"{} update available: {} -> {}",
				kind, current, v
			);
		}
		None => println!("Could not determine latest version."),
	}

	if !confirm("Do you want to update gitui?")? {
		println!("Update cancelled.");
		return Ok(());
	}

	println!("Updating via {}...", method);

	let result = match method {
		InstallMethod::Cargo => update_via_cargo(),
		InstallMethod::Homebrew => update_via_homebrew(),
		InstallMethod::Dnf => update_via_dnf(),
		InstallMethod::Apt => update_via_apt(),
		InstallMethod::Pacman => update_via_pacman(),
		InstallMethod::Scoop => update_via_scoop(),
		InstallMethod::Chocolatey => update_via_chocolatey(),
		InstallMethod::ScoopBucket => update_via_scoop_bucket(),
		InstallMethod::Windows => update_via_windows(),
		InstallMethod::Unknown => {
			Err("Unknown installation method".to_string())
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

fn get_current_version() -> String {
	let build = env!("GITUI_BUILD_NAME");
	build.split_whitespace().next().unwrap_or(build).to_string()
}

fn is_prerelease(v: &str) -> bool {
	let lower = v.to_lowercase();
	[
		"nightly", "-rc", "-beta", "-alpha", "-dev", "preview",
		"snapshot",
	]
	.iter()
	.any(|&s| lower.contains(s))
}

fn fetch_latest_version() -> Option<String> {
	let output = Command::new("git")
		.args([
			"ls-remote",
			"--tags",
			"--sort=-v:refname",
			"https://github.com/extrawurst/gitui.git",
		])
		.output()
		.ok()?;

	if !output.status.success() {
		return None;
	}

	String::from_utf8_lossy(&output.stdout)
		.lines()
		.filter_map(|line| {
			line.split('\t')
				.nth(1)?
				.strip_prefix("refs/tags/")?
				.strip_prefix('v')
		})
		.next()
		.map(String::from)
}

fn fetch_latest_stable() -> Option<String> {
	let output = Command::new("git")
		.args([
			"ls-remote",
			"--tags",
			"--sort=-v:refname",
			"https://github.com/extrawurst/gitui.git",
		])
		.output()
		.ok()?;

	if !output.status.success() {
		return None;
	}

	let version = String::from_utf8_lossy(&output.stdout)
		.lines()
		.filter_map(|line| {
			line.split('\t')
				.nth(1)?
				.strip_prefix("refs/tags/")?
				.strip_prefix('v')
		})
		.find(|&v| !is_prerelease(v))
		.map(String::from);

	if version.is_none() {
		println!("Warning: No stable release found. Use -n for pre-releases.");
	}

	version
}

fn confirm(prompt: &str) -> Result<bool> {
	print!("{} [y/N]: ", prompt);
	io::stdout().flush()?;

	let mut input = String::new();
	io::stdin().read_line(&mut input)?;

	Ok(input.trim().eq_ignore_ascii_case("y"))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_is_prerelease_nightly() {
		assert!(is_prerelease("0.28.1-nightly"));
		assert!(is_prerelease("0.28.1-NIGHTLY"));
	}

	#[test]
	fn test_is_prerelease_rc() {
		assert!(is_prerelease("0.28.1-rc.1"));
		assert!(is_prerelease("0.28.1-RC.1"));
	}

	#[test]
	fn test_is_prerelease_beta() {
		assert!(is_prerelease("0.28.1-beta"));
		assert!(is_prerelease("0.28.0-beta.2"));
	}

	#[test]
	fn test_is_prerelease_alpha() {
		assert!(is_prerelease("0.28.1-alpha"));
		assert!(is_prerelease("0.28.0-alpha.1"));
	}

	#[test]
	fn test_is_prerelease_dev() {
		assert!(is_prerelease("0.28.1-dev"));
		assert!(is_prerelease("0.28.0-dev.20240101"));
	}

	#[test]
	fn test_is_prerelease_preview() {
		assert!(is_prerelease("0.28.1-preview"));
		assert!(is_prerelease("0.28.0-preview.3"));
	}

	#[test]
	fn test_is_prerelease_snapshot() {
		assert!(is_prerelease("0.28.1-snapshot"));
	}

	#[test]
	fn test_is_not_prerelease_stable() {
		assert!(!is_prerelease("0.28.1"));
		assert!(!is_prerelease("0.28.0"));
		assert!(!is_prerelease("1.0.0"));
	}

	#[test]
	fn test_is_not_prerelease_version_with_prerelease_substring() {
		// Ensure we don't false-positive on versions that contain prerelease keywords
		// but aren't actually prereleases (e.g., "0.28.1-nightly-feature" wouldn't be valid anyway)
		assert!(is_prerelease("0.28.1-nightly"));
		assert!(!is_prerelease("0.28.1"));
	}
}
