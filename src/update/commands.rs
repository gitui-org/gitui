//! Executes update commands for supported package managers (cargo, dnf, apt,
//! etc.) using a macro to generate consistent command patterns.

use std::process::Command;

/// Generates an update function for a specific package manager.
///
/// The generated function:
/// - Executes the specified command with given arguments
/// - Checks for success or "already installed" states
/// - Returns descriptive error messages on failure
///
/// # Macro Parameters
///
/// - `$name` - Function name (e.g., `update_via_dnf`)
/// - `$cmd` - Command to execute (e.g., `"sudo"`)
/// - `$args` - Arguments array (e.g., `["dnf", "upgrade", "gitui", "-y"]`)
/// - `$success_msg` - Message printed on successful update
macro_rules! update_via {
    ($name:ident, $cmd:expr, $args:expr, $success_msg:literal) => {
        pub fn $name() -> Result<(), String> {
            let output = Command::new($cmd)
                .args($args)
                .output()
                .map_err(|e| format!("Failed to run {}: {}", $cmd, e))?;

            if output.status.success() {
                println!($success_msg);
                Ok(())
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                if stderr.contains("already installed") || stderr.contains("already up-to-date") {
                    println!("Already up to date!");
                    Ok(())
                } else {
                    Err(format!("{} failed:\n{}", $cmd, stderr))
                }
            }
        }
    };
}

update_via!(
    update_via_cargo,
    "cargo",
    ["install", "gitui", "--force"],
    "Successfully updated via cargo!"
);

update_via!(
    update_via_dnf,
    "sudo",
    ["dnf", "upgrade", "gitui", "-y"],
    "Successfully updated via dnf!"
);

update_via!(
    update_via_apt,
    "sudo",
    ["apt", "upgrade", "gitui", "-y"],
    "Successfully updated via apt!"
);

update_via!(
    update_via_pacman,
    "sudo",
    ["pacman", "-Syu", "gitui", "--noconfirm"],
    "Successfully updated via pacman!"
);

#[cfg(target_os = "macos")]
update_via!(
    update_via_homebrew,
    "brew",
    ["upgrade", "gitui"],
    "Successfully updated via homebrew!"
);

#[cfg(not(target_os = "macos"))]
pub fn update_via_homebrew() -> Result<(), String> {
    Err("Homebrew is only supported on macOS".to_string())
}

#[cfg(target_os = "windows")]
update_via!(
    update_via_scoop,
    "scoop",
    ["update", "gitui"],
    "Successfully updated via scoop!"
);

#[cfg(not(target_os = "windows"))]
pub fn update_via_scoop() -> Result<(), String> {
    Err("Scoop is only supported on Windows".to_string())
}

#[cfg(target_os = "windows")]
update_via!(
    update_via_scoop_bucket,
    "scoop",
    ["update", "gitui"],
    "Successfully updated via scoop bucket!"
);

#[cfg(not(target_os = "windows"))]
pub fn update_via_scoop_bucket() -> Result<(), String> {
    Err("Scoop is only supported on Windows".to_string())
}

#[cfg(target_os = "windows")]
update_via!(
    update_via_chocolatey,
    "choco",
    ["upgrade", "gitui", "-y"],
    "Successfully updated via chocolatey!"
);

#[cfg(not(target_os = "windows"))]
pub fn update_via_chocolatey() -> Result<(), String> {
    Err("Chocolatey is only supported on Windows".to_string())
}

pub fn update_via_windows() -> Result<(), String> {
    Err("Windows binary update not supported. Please download the latest release from GitHub.".to_string())
}
