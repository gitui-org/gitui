//! Detects how gitui was installed by examining the executable path and
//! querying system package managers (dnf, apt, pacman, etc.).

use std::path::Path;
use std::process::Command;

/// Installation methods supported by the self-update system.
#[derive(Debug, Clone, PartialEq)]
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

pub fn detect_install_method() -> InstallMethod {
	let current_exe = std::env::current_exe().ok();
	let exe_path = current_exe.as_ref().map(|p| p.as_path());

	let is_cargo_build = exe_path.map_or(false, |p| {
		let s = p.to_string_lossy();
		s.contains(".cargo/bin")
			|| s.contains("cargo/registry")
			|| s.contains("target/release")
			|| s.contains("target/debug")
	});

	if is_cargo_build {
		if has_dnf_installation() {
			return InstallMethod::Dnf;
		}
		if has_apt_installation() {
			return InstallMethod::Apt;
		}
		if has_pacman_installation() {
			return InstallMethod::Pacman;
		}
		return InstallMethod::Cargo;
	}

	exe_path.map_or(InstallMethod::Unknown, |p| {
		let s = p.to_string_lossy();

		if s.contains("homebrew") || s.contains("Cellar") {
			return InstallMethod::Homebrew;
		}

		if s.contains("scoop") {
			return if s.contains("scoop-bucket") {
				InstallMethod::ScoopBucket
			} else {
				InstallMethod::Scoop
			};
		}

		if s.contains("chocolatey") {
			return InstallMethod::Chocolatey;
		}

		if cfg!(target_os = "windows") {
			return InstallMethod::Windows;
		}

		if s.contains("/usr/bin") || s.contains("/usr/local/bin") {
			if has_dnf_installation() {
				return InstallMethod::Dnf;
			}
			if has_apt_installation() {
				return InstallMethod::Apt;
			}
			if has_pacman_installation() {
				return InstallMethod::Pacman;
			}
		}

		InstallMethod::Unknown
	})
}

#[cfg(target_os = "linux")]
fn has_dnf_installation() -> bool {
	Path::new("/usr/bin/rpm").exists()
		&& Command::new("rpm")
			.args(["-q", "gitui"])
			.output()
			.map(|o| o.status.success())
			.unwrap_or(false)
}

#[cfg(not(target_os = "linux"))]
fn has_dnf_installation() -> bool {
	false
}

#[cfg(target_os = "linux")]
fn has_apt_installation() -> bool {
	Path::new("/usr/bin/dpkg").exists()
		&& Command::new("dpkg")
			.args(["-l", "gitui"])
			.output()
			.map(|o| o.status.success())
			.unwrap_or(false)
}

#[cfg(not(target_os = "linux"))]
fn has_apt_installation() -> bool {
	false
}

#[cfg(target_os = "linux")]
fn has_pacman_installation() -> bool {
	Path::new("/usr/bin/pacman").exists()
		&& Command::new("pacman")
			.args(["-Q", "gitui"])
			.output()
			.map(|o| o.status.success())
			.unwrap_or(false)
}

#[cfg(not(target_os = "linux"))]
fn has_pacman_installation() -> bool {
	false
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_install_method_display() {
		assert_eq!(InstallMethod::Cargo.to_string(), "cargo");
		assert_eq!(InstallMethod::Homebrew.to_string(), "homebrew");
		assert_eq!(InstallMethod::Apt.to_string(), "apt");
		assert_eq!(InstallMethod::Dnf.to_string(), "dnf");
		assert_eq!(InstallMethod::Pacman.to_string(), "pacman");
		assert_eq!(InstallMethod::Windows.to_string(), "windows");
		assert_eq!(InstallMethod::Scoop.to_string(), "scoop");
		assert_eq!(
			InstallMethod::Chocolatey.to_string(),
			"chocolatey"
		);
		assert_eq!(
			InstallMethod::ScoopBucket.to_string(),
			"scoop-bucket"
		);
		assert_eq!(InstallMethod::Unknown.to_string(), "unknown");
	}

	#[test]
	fn test_install_method_equality() {
		assert_eq!(InstallMethod::Cargo, InstallMethod::Cargo);
		assert_ne!(InstallMethod::Cargo, InstallMethod::Dnf);
	}

	#[test]
	fn test_install_method_clone() {
		let method = InstallMethod::Dnf;
		let cloned = method.clone();
		assert_eq!(method, cloned);
	}

	#[test]
	fn test_install_method_debug() {
		let debug_str = format!("{:?}", InstallMethod::Cargo);
		assert!(debug_str.contains("Cargo"));
	}
}
