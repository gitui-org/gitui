use git2::Repository;
use log::debug;

use crate::{error::Result, HookResult, HooksError};

use std::{
	env,
	path::{Path, PathBuf},
	process::{Child, Command, Stdio},
	str::FromStr,
	thread,
	time::Duration,
};

pub struct HookPaths {
	pub git: PathBuf,
	pub hook: PathBuf,
	pub pwd: PathBuf,
}

const CONFIG_HOOKS_PATH: &str = "core.hooksPath";
const DEFAULT_HOOKS_PATH: &str = "hooks";

impl HookPaths {
	/// `core.hooksPath` always takes precedence.
	/// If its defined and there is no hook `hook` this is not considered
	/// an error or a reason to search in other paths.
	/// If the config is not set we go into search mode and
	/// first check standard `.git/hooks` folder and any sub path provided in `other_paths`.
	///
	/// Note: we try to model as closely as possible what git shell is doing.
	pub fn new(
		repo: &Repository,
		other_paths: Option<&[&str]>,
		hook: &str,
	) -> Result<Self> {
		let pwd = repo
			.workdir()
			.unwrap_or_else(|| repo.path())
			.to_path_buf();

		let git_dir = repo.path().to_path_buf();

		if let Some(config_path) = Self::config_hook_path(repo)? {
			let hooks_path = PathBuf::from(config_path);

			let hook = hooks_path.join(hook);

			let hook = shellexpand::full(
				hook.as_os_str()
					.to_str()
					.ok_or(HooksError::PathToString)?,
			)?;

			let hook = PathBuf::from_str(hook.as_ref())
				.map_err(|_| HooksError::PathToString)?;

			return Ok(Self {
				git: git_dir,
				hook,
				pwd,
			});
		}

		Ok(Self {
			git: git_dir,
			hook: Self::find_hook(repo, other_paths, hook),
			pwd,
		})
	}

	fn config_hook_path(repo: &Repository) -> Result<Option<String>> {
		Ok(repo.config()?.get_string(CONFIG_HOOKS_PATH).ok())
	}

	/// check default hook path first and then followed by `other_paths`.
	/// if no hook is found we return the default hook path
	fn find_hook(
		repo: &Repository,
		other_paths: Option<&[&str]>,
		hook: &str,
	) -> PathBuf {
		let mut paths = vec![DEFAULT_HOOKS_PATH.to_string()];
		if let Some(others) = other_paths {
			paths.extend(
				others
					.iter()
					.map(|p| p.trim_end_matches('/').to_string()),
			);
		}

		for p in paths {
			let p = repo.path().to_path_buf().join(p).join(hook);
			if p.exists() {
				return p;
			}
		}

		repo.path()
			.to_path_buf()
			.join(DEFAULT_HOOKS_PATH)
			.join(hook)
	}

	/// was a hook file found and is it executable
	pub fn found(&self) -> bool {
		self.hook.exists() && is_executable(&self.hook)
	}

	/// this function calls hook scripts based on conventions documented here
	/// see <https://git-scm.com/docs/githooks>
	pub fn run_hook(&self, args: &[&str]) -> Result<HookResult> {
		let hook = self.hook.clone();
		let output = spawn_hook_process(&self.pwd, &hook, args)?
			.wait_with_output()?;

		Ok(hook_result_from_output(hook, &output))
	}

	/// this function calls hook scripts based on conventions documented here
	/// see <https://git-scm.com/docs/githooks>
	///
	/// With the addition of a timeout for the execution of the script.
	/// If the script takes longer than the specified timeout it will be killed.
	///
	/// This will add an additional 1ms at a minimum, up to a maximum of 50ms.
	/// see `timeout_with_quadratic_backoff` for more information
	pub fn run_hook_with_timeout(
		&self,
		args: &[&str],
		timeout: Duration,
	) -> Result<HookResult> {
		let hook = self.hook.clone();
		let mut child = spawn_hook_process(&self.pwd, &hook, args)?;

		let output = if timeout.is_zero() {
			child.wait_with_output()?
		} else {
			if !timeout_with_quadratic_backoff(timeout, || {
				Ok(child.try_wait()?.is_some())
			})? {
				debug!("killing hook process");
				child.kill()?;
				return Ok(HookResult::TimedOut { hook });
			}

			child.wait_with_output()?
		};

		Ok(hook_result_from_output(hook, &output))
	}
}

/// This will loop, sleeping with exponentially increasing time until completion or timeout has been reached.
///
/// Formula:
///   Base Duration: `BASE_MILLIS` is set to 1 millisecond.
///   Max Sleep Duration: `MAX_SLEEP_MILLIS` is set to 50 milliseconds.
///   Quadratic Calculation: Sleep time = (attempt^2) * `BASE_MILLIS`, capped by `MAX_SLEEP_MILLIS`.
///
/// The timing for each attempt up to the cap is as follows.
///
/// Attempt 1:
///     Sleep Time=(1^2)×1=1
///         Actual Sleep: 1 millisecond
///         Total Sleep: 1 millisecond
///
/// Attempt 2:
///     Sleep Time=(2^2)×1=4
///         Actual Sleep: 4 milliseconds
///         Total Sleep: 5 milliseconds
///
/// Attempt 3:
///     Sleep Time=(3^2)×1=9
///         Actual Sleep: 9 milliseconds
///         Total Sleep: 14 milliseconds
///
/// Attempt 4:
///     Sleep Time=(4^2)×1=16
///         Actual Sleep: 16 milliseconds
///         Total Sleep: 30 milliseconds
///
/// Attempt 5:
///     Sleep Time=(5^2)×1=25
///         Actual Sleep: 25 milliseconds
///         Total Sleep: 55 milliseconds
///
/// Attempt 6:
///     Sleep Time=(6^2)×1=36
///         Actual Sleep: 36 milliseconds
///         Total Sleep: 91 milliseconds
///
/// Attempt 7:
///     Sleep Time=(7^2)×1=49
///         Actual Sleep: 49 milliseconds
///         Total Sleep: 140 milliseconds
///
/// Attempt 8:
//     Sleep Time=(8^2)×1=64, capped by `MAX_SLEEP_MILLIS` of 50
//          Actual Sleep: 50 milliseconds
//          Total Sleep: 190 milliseconds
fn timeout_with_quadratic_backoff<F>(
	timeout: Duration,
	mut is_complete: F,
) -> Result<bool>
where
	F: FnMut() -> Result<bool>,
{
	const BASE_MILLIS: u64 = 1;
	const MAX_SLEEP_MILLIS: u64 = 50;

	let timer = std::time::Instant::now();
	let mut attempt: i32 = 1;

	loop {
		if is_complete()? {
			return Ok(true);
		}

		if timer.elapsed() > timeout {
			return Ok(false);
		}

		let mut sleep_time = Duration::from_millis(
			(attempt.pow(2) as u64)
				.saturating_mul(BASE_MILLIS)
				.min(MAX_SLEEP_MILLIS),
		);

		// Ensure we do not sleep more than the remaining time
		let remaining_time = timeout - timer.elapsed();
		if remaining_time < sleep_time {
			sleep_time = remaining_time;
		}

		thread::sleep(sleep_time);
		attempt += 1;
	}
}

fn hook_result_from_output(
	hook: PathBuf,
	output: &std::process::Output,
) -> HookResult {
	if output.status.success() {
		HookResult::Ok { hook }
	} else {
		let stderr =
			String::from_utf8_lossy(&output.stderr).to_string();
		let stdout =
			String::from_utf8_lossy(&output.stdout).to_string();

		HookResult::RunNotSuccessful {
			code: output.status.code(),
			stdout,
			stderr,
			hook,
		}
	}
}

fn spawn_hook_process(
	directory: &PathBuf,
	hook: &PathBuf,
	args: &[&str],
) -> Result<Child> {
	let arg_str = format!("{:?} {}", hook, args.join(" "));
	// Use -l to avoid "command not found" on Windows.
	let bash_args = vec!["-l".to_string(), "-c".to_string(), arg_str];

	log::trace!("run hook '{:?}' in '{:?}'", hook, directory);

	let git_shell = find_bash_executable()
		.or_else(find_default_unix_shell)
		.unwrap_or_else(|| "bash".into());
	let child = Command::new(git_shell)
		.args(bash_args)
		.with_no_window()
		.current_dir(directory)
		// This call forces Command to handle the Path environment correctly on windows,
		// the specific env set here does not matter
		// see https://github.com/rust-lang/rust/issues/37519
		.env(
			"DUMMY_ENV_TO_FIX_WINDOWS_CMD_RUNS",
			"FixPathHandlingOnWindows",
		)
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()?;

	Ok(child)
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
	use std::os::unix::fs::PermissionsExt;

	let metadata = match path.metadata() {
		Ok(metadata) => metadata,
		Err(e) => {
			log::error!("metadata error: {}", e);
			return false;
		}
	};

	let permissions = metadata.permissions();

	permissions.mode() & 0o111 != 0
}

#[cfg(windows)]
/// windows does not consider bash scripts to be executable so we consider everything
/// to be executable (which is not far from the truth for windows platform.)
const fn is_executable(_: &Path) -> bool {
	true
}

// Find bash.exe, and avoid finding wsl's bash.exe on Windows.
// None for non-Windows.
fn find_bash_executable() -> Option<PathBuf> {
	if cfg!(windows) {
		Command::new("where.exe")
			.arg("git")
			.output()
			.ok()
			.map(|out| {
				PathBuf::from(Into::<String>::into(
					String::from_utf8_lossy(&out.stdout),
				))
			})
			.as_deref()
			.and_then(Path::parent)
			.and_then(Path::parent)
			.map(|p| p.join("usr/bin/bash.exe"))
			.filter(|p| p.exists())
	} else {
		None
	}
}

// Find default shell on Unix-like OS.
fn find_default_unix_shell() -> Option<PathBuf> {
	env::var_os("SHELL").map(PathBuf::from)
}

trait CommandExt {
	/// The process is a console application that is being run without a
	/// console window. Therefore, the console handle for the application is
	/// not set.
	///
	/// This flag is ignored if the application is not a console application,
	/// or if it used with either `CREATE_NEW_CONSOLE` or `DETACHED_PROCESS`.
	///
	/// See: <https://learn.microsoft.com/en-us/windows/win32/procthread/process-creation-flags>
	const CREATE_NO_WINDOW: u32 = 0x0800_0000;

	fn with_no_window(&mut self) -> &mut Self;
}

impl CommandExt for Command {
	/// On Windows, CLI applications that aren't the window's subsystem will
	/// create and show a console window that pops up next to the main
	/// application window when run. We disable this behavior by setting the
	/// `CREATE_NO_WINDOW` flag.
	#[inline]
	fn with_no_window(&mut self) -> &mut Self {
		#[cfg(windows)]
		{
			use std::os::windows::process::CommandExt;
			self.creation_flags(Self::CREATE_NO_WINDOW);
		}

		self
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use pretty_assertions::assert_eq;

	/// Ensures that the `timeout_with_quadratic_backoff` function
	/// does not cause the total execution time does not grealy increase the total execution time.
	#[test]
	fn test_timeout_with_quadratic_backoff_cost() {
		let timeout = Duration::from_millis(100);
		let start = std::time::Instant::now();
		let result =
			timeout_with_quadratic_backoff(timeout, || Ok(false));
		let elapsed = start.elapsed();

		assert_eq!(result.unwrap(), false);
		assert!(elapsed < timeout + Duration::from_millis(10));
	}

	/// Ensures that the `timeout_with_quadratic_backoff` function
	/// does not cause the execution time wait for much longer than the reason we are waiting.
	#[test]
	fn test_timeout_with_quadratic_backoff_timeout() {
		let timeout = Duration::from_millis(100);
		let wait_time = Duration::from_millis(5); // Attempt 1 + 2 = 5 ms

		let start = std::time::Instant::now();
		let _ = timeout_with_quadratic_backoff(timeout, || {
			Ok(start.elapsed() > wait_time)
		});

		let elapsed = start.elapsed();
		assert_eq!(5, elapsed.as_millis());
	}

	/// Ensures that the overhead of the `timeout_with_quadratic_backoff` function
	/// does not exceed 15 microseconds per attempt.
	///
	/// This will obviously vary depending on the system, but this is a rough estimate.
	/// The overhead on an AMD 5900x is roughly 1 - 1.5 microseconds per attempt.
	#[test]
	fn test_timeout_with_quadratic_backoff_overhead() {
		// A timeout of 50 milliseconds should take 8 attempts to reach the timeout.
		const TARGET_ATTEMPTS: u128 = 8;
		const TIMEOUT: Duration = Duration::from_millis(190);

		let start = std::time::Instant::now();
		let _ = timeout_with_quadratic_backoff(TIMEOUT, || Ok(false));
		let elapsed = start.elapsed();

		let overhead = (elapsed - TIMEOUT).as_micros();
		assert!(overhead < TARGET_ATTEMPTS * 15);
	}
}
