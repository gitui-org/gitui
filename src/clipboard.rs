use anyhow::{anyhow, Result};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use which::which;

fn exec_copy_with_args(
	command: &str,
	args: &[&str],
	text: &str,
	pipe_stderr: bool,
) -> Result<()> {
	let binary = which(command)
		.ok()
		.unwrap_or_else(|| PathBuf::from(command));

	let mut process = Command::new(binary)
		.args(args)
		.stdin(Stdio::piped())
		.stdout(Stdio::null())
		.stderr(if pipe_stderr {
			Stdio::piped()
		} else {
			Stdio::null()
		})
		.spawn()
		.map_err(|e| anyhow!("`{:?}`: {}", command, e))?;

	process
		.stdin
		.as_mut()
		.ok_or_else(|| anyhow!("`{:?}`", command))?
		.write_all(text.as_bytes())
		.map_err(|e| anyhow!("`{:?}`: {}", command, e))?;

	let out = process
		.wait_with_output()
		.map_err(|e| anyhow!("`{:?}`: {}", command, e))?;

	if out.status.success() {
		Ok(())
	} else {
		let msg = if out.stderr.is_empty() {
			format!("{}", out.status).into()
		} else {
			String::from_utf8_lossy(&out.stderr)
		};
		Err(anyhow!("`{command:?}`: {msg}"))
	}
}

// Implementation taken from https://crates.io/crates/wsl.
// Using /proc/sys/kernel/osrelease as an authoratative source
// based on this comment: https://github.com/microsoft/WSL/issues/423#issuecomment-221627364
#[cfg(all(target_family = "unix", not(target_os = "macos")))]
fn is_wsl() -> bool {
	if let Ok(b) = std::fs::read("/proc/sys/kernel/osrelease") {
		if let Ok(s) = std::str::from_utf8(&b) {
			let a = s.to_ascii_lowercase();
			return a.contains("microsoft") || a.contains("wsl");
		}
	}
	false
}

// Copy text using escape sequence Ps = 5 2.
// This enables copying even if there is no Wayland or X socket available,
// e.g. via SSH, as long as it supported by the terminal.
// See https://invisible-island.net/xterm/ctlseqs/ctlseqs.html#h3-Operating-System-Commands
#[cfg(any(
	all(target_family = "unix", not(target_os = "macos")),
	test
))]
fn copy_string_osc52(text: &str, out: &mut impl Write) -> Result<()> {
	use base64::prelude::{Engine, BASE64_STANDARD};
	const OSC52_DESTINATION_CLIPBOARD: char = 'c';
	write!(
		out,
		"\x1b]52;{destination};{encoded_text}\x07",
		destination = OSC52_DESTINATION_CLIPBOARD,
		encoded_text = BASE64_STANDARD.encode(text)
	)?;
	Ok(())
}

#[cfg(all(target_family = "unix", not(target_os = "macos")))]
fn copy_string_wayland(text: &str) -> Result<()> {
	if exec_copy_with_args("wl-copy", &[], text, false).is_ok() {
		return Ok(());
	}

	copy_string_osc52(text, &mut std::io::stdout())
}

#[cfg(all(target_family = "unix", not(target_os = "macos")))]
fn copy_string_x(text: &str) -> Result<()> {
	if exec_copy_with_args(
		"xclip",
		&["-selection", "clipboard"],
		text,
		false,
	)
	.is_ok()
	{
		return Ok(());
	}

	if exec_copy_with_args("xsel", &["--clipboard"], text, true)
		.is_ok()
	{
		return Ok(());
	}

	copy_string_osc52(text, &mut std::io::stdout())
}

#[cfg(all(target_family = "unix", not(target_os = "macos")))]
pub fn copy_string(text: &str) -> Result<()> {
	if std::env::var("WAYLAND_DISPLAY").is_ok() {
		return copy_string_wayland(text);
	}

	if is_wsl() {
		return exec_copy_with_args("clip.exe", &[], text, false);
	}

	copy_string_x(text)
}

#[cfg(any(target_os = "macos", windows))]
fn exec_copy(command: &str, text: &str) -> Result<()> {
	exec_copy_with_args(command, &[], text, true)
}

#[cfg(target_os = "macos")]
pub fn copy_string(text: &str) -> Result<()> {
	exec_copy("pbcopy", text)
}

#[cfg(windows)]
pub fn copy_string(text: &str) -> Result<()> {
	exec_copy("clip", text)
}

#[cfg(test)]
mod tests {
	#[test]
	fn test_copy_string_osc52() {
		let mut buffer = Vec::<u8>::new();
		{
			let mut cursor = std::io::Cursor::new(&mut buffer);
			super::copy_string_osc52("foo", &mut cursor).unwrap();
		}
		let output = String::from_utf8(buffer).unwrap();
		assert_eq!(output, "\x1b]52;c;Zm9v\x07");
	}
}
