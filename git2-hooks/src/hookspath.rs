use git2::Repository;

use crate::{error::Result, HookResult, HooksError};

use std::{
	ffi::{OsStr, OsString},
	io::Read,
	path::{Path, PathBuf},
	process::{Child, Command, Stdio},
	str::FromStr,
	thread,
	time::Duration,
};

#[cfg(unix)]
use {
	nix::{
		sys::signal::{killpg, SIGKILL},
		unistd::Pid,
	},
	std::os::unix::process::CommandExt as _,
};

pub struct HookPaths {
	pub git: PathBuf,
	pub hook: PathBuf,
	pub pwd: PathBuf,
}

const CONFIG_HOOKS_PATH: &str = "core.hooksPath";
const DEFAULT_HOOKS_PATH: &str = "hooks";
const ENOEXEC: i32 = 8;

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

			let hook =
				Self::expand_path(&hooks_path.join(hook), &pwd)?;

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

	/// Expand path according to the rule of githooks and config
	/// core.hooksPath
	fn expand_path(path: &Path, pwd: &Path) -> Result<PathBuf> {
		let hook_expanded = shellexpand::full(
			path.as_os_str()
				.to_str()
				.ok_or(HooksError::PathToString)?,
		)?;
		let hook_expanded = PathBuf::from_str(hook_expanded.as_ref())
			.map_err(|_| HooksError::PathToString)?;

		// `man git-config`:
		//
		// > A relative path is taken as relative to the
		// > directory where the hooks are run (see the
		// > "DESCRIPTION" section of githooks[5]).
		//
		// `man githooks`:
		//
		// > Before Git invokes a hook, it changes its
		// > working directory to either $GIT_DIR in a bare
		// > repository or the root of the working tree in a
		// > non-bare repository.
		//
		// I.e. relative paths in core.hooksPath in non-bare
		// repositories are always relative to GIT_WORK_TREE.
		Ok({
			if hook_expanded.is_absolute() {
				hook_expanded
			} else {
				pwd.join(hook_expanded)
			}
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
	#[inline]
	#[allow(unused)]
	pub fn run_hook(&self, args: &[&str]) -> Result<HookResult> {
		self.run_hook_os_str(args)
	}

	/// this function calls hook scripts based on conventions documented here
	/// see <https://git-scm.com/docs/githooks>
	#[inline]
	pub fn run_hook_os_str<I, S>(&self, args: I) -> Result<HookResult>
	where
		I: IntoIterator<Item = S> + Copy,
		S: AsRef<OsStr>,
	{
		self.run_hook_with_timeout_os_str(args, None)
	}

	/// this function calls hook scripts based on conventions documented here
	/// see <https://git-scm.com/docs/githooks>
	///
	/// With the addition of a timeout for the execution of the script.
	/// If the script takes longer than the specified timeout it will be killed.
	///
	/// This will add an additional 1ms at a minimum, up to a maximum of 50ms.
	/// see `timeout_with_quadratic_backoff` for more information
	#[inline]
	pub fn run_hook_with_timeout(
		&self,
		args: &[&str],
		timeout: Option<Duration>,
	) -> Result<HookResult> {
		self.run_hook_with_timeout_os_str(args, timeout)
	}

	/// this function calls hook scripts based on conventions documented here
	/// see <https://git-scm.com/docs/githooks>
	///
	/// With the addition of a timeout for the execution of the script.
	/// If the script takes longer than the specified timeout it will be killed.
	///
	/// This will add an additional 1ms at a minimum, up to a maximum of 50ms.
	/// see `timeout_with_quadratic_backoff` for more information
	pub fn run_hook_with_timeout_os_str<I, S>(
		&self,
		args: I,
		timeout: Option<Duration>,
	) -> Result<HookResult>
	where
		I: IntoIterator<Item = S> + Copy,
		S: AsRef<OsStr>,
	{
		let hook = self.hook.clone();
		let mut child = spawn_hook_process(&self.pwd, &hook, args)?;

		let output = if timeout.is_none()
			|| timeout.is_some_and(|t| t.is_zero())
		{
			child.wait_with_output()?
		} else {
			let timeout = timeout.unwrap();
			if !timeout_with_quadratic_backoff(timeout, || {
				Ok(child.try_wait()?.is_some())
			})? {
				if cfg!(unix) {
					match i32::try_from(child.id()) {
						Ok(pid) => {
							killpg(Pid::from_raw(pid), SIGKILL)
								.expect("killpg failed");
						}
						Err(_) => child.kill()?,
					}
				} else {
					child.kill()?;
				}

				let mut stdout = String::new();
				let mut stderr = String::new();
				if let Some(mut pipe) = child.stdout {
					pipe.read_to_string(&mut stdout)?;
				}
				if let Some(mut pipe) = child.stderr {
					pipe.read_to_string(&mut stderr)?;
				}

				return Ok(HookResult::TimedOut {
					hook,
					stdout,
					stderr,
				});
			}

			child.wait_with_output()?
		};

		Ok(hook_result_from_output(hook, &output))
	}
}

/// This will loop, sleeping with quadratically increasing time until completion or timeout has been reached.
///
/// Formula:
///   Base Duration: `TIMESCALE` is set to 1 millisecond.
///   Max Sleep Duration: `MAX_SLEEP` is set to 50 milliseconds.
///   Quadratic Calculation: Sleep time = (attempt^2) * `TIMESCALE`, capped by `MAX_SLEEP`.
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
///     Sleep Time=(8^2)×1=64, capped by `MAX_SLEEP`
///          Actual Sleep: 50 milliseconds
///          Total Sleep: 190 milliseconds
fn timeout_with_quadratic_backoff<F>(
	timeout: Duration,
	mut is_complete: F,
) -> Result<bool>
where
	F: FnMut() -> Result<bool>,
{
	const TIMESCALE: Duration = Duration::from_millis(1);
	const MAX_SLEEP: Duration = Duration::from_millis(50);

	let timer = std::time::Instant::now();
	let mut attempt: u32 = 1;

	while !is_complete()? {
		let Some(remaining_time) =
			timeout.checked_sub(timer.elapsed())
		else {
			return Ok(false);
		};

		let sleep_time = TIMESCALE
			.saturating_mul(attempt.saturating_pow(2))
			.min(MAX_SLEEP)
			.min(remaining_time);

		thread::sleep(sleep_time);
		attempt += 1;
	}

	Ok(true)
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

fn spawn_hook_process<I, S>(
	directory: &PathBuf,
	hook: &PathBuf,
	args: I,
) -> Result<Child>
where
	I: IntoIterator<Item = S> + Copy,
	S: AsRef<OsStr>,
{
	log::trace!("run hook '{:?}' in '{:?}'", hook, directory);

	let spawn_command = |command: &mut Command| {
		if cfg!(unix) {
			command.process_group(0);
		}

		command
			.args(args)
			.current_dir(directory)
			.with_no_window()
			.stdout(Stdio::piped())
			.stderr(Stdio::piped())
			.stdin(Stdio::piped())
			.spawn()
	};

	let child = if cfg!(windows) {
		// execute hook in shell
		let command = {
			// SEE: https://pubs.opengroup.org/onlinepubs/9699919799/utilities/V3_chap02.html#tag_18_02_02
			// Enclosing characters in single-quotes ( '' ) shall preserve the literal value of each character within the single-quotes.
			// A single-quote cannot occur within single-quotes.
			const REPLACEMENT: &str = concat!(
				"'",   // closing single-quote
				"\\'", // one escaped single-quote (outside of single-quotes)
				"'",   // new single-quote
			);

			let mut os_str = OsString::new();
			os_str.push("'");
			if let Some(hook) = hook.to_str() {
				os_str.push(hook.replace('\'', REPLACEMENT));
			} else {
				#[cfg(windows)]
				{
					use std::os::windows::ffi::OsStrExt;
					if hook
						.as_os_str()
						.encode_wide()
						.any(|x| x == u16::from(b'\''))
					{
						// TODO: escape single quotes instead of failing
						return Err(HooksError::PathToString);
					}
				}

				os_str.push(hook.as_os_str());
			}
			os_str.push("'");
			os_str.push(" \"$@\"");

			os_str
		};
		spawn_command(sh_command().arg("-c").arg(command).arg(hook))
	} else {
		// execute hook directly
		match spawn_command(&mut Command::new(hook)) {
			Err(err) if err.raw_os_error() == Some(ENOEXEC) => {
				spawn_command(sh_command().arg(hook))
			}
			result => result,
		}
	}?;

	Ok(child)
}

fn sh_command() -> Command {
	let mut command = Command::new(gix_path::env::shell());

	if cfg!(windows) {
		// This call forces Command to handle the Path environment correctly on windows,
		// the specific env set here does not matter
		// see https://github.com/rust-lang/rust/issues/37519
		command.env(
			"DUMMY_ENV_TO_FIX_WINDOWS_CMD_RUNS",
			"FixPathHandlingOnWindows",
		);

		// Use -l to avoid "command not found"
		command.arg("-l");
	}

	command
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
/// windows does not consider shell scripts to be executable so we consider everything
/// to be executable (which is not far from the truth for windows platform.)
const fn is_executable(_: &Path) -> bool {
	true
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
mod test {
	use super::*;
	use pretty_assertions::assert_eq;
	use std::path::Path;

	#[test]
	fn test_hookspath_relative() {
		assert_eq!(
			HookPaths::expand_path(
				Path::new("pre-commit"),
				Path::new("example_git_root"),
			)
			.unwrap(),
			Path::new("example_git_root").join("pre-commit")
		);
	}

	#[test]
	fn test_hookspath_absolute() {
		let absolute_hook =
			std::env::current_dir().unwrap().join("pre-commit");
		assert_eq!(
			HookPaths::expand_path(
				&absolute_hook,
				Path::new("example_git_root"),
			)
			.unwrap(),
			absolute_hook
		);
	}

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
		// Attempt 3 = 14ms so we want to ensure we didn't pass it.
		assert!(elapsed.as_millis() < 13);
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
