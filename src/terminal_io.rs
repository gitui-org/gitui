//! Terminal output selection for embedding gitui in editors (e.g. Helix).

use crossterm::{Command, ExecutableCommand};
use std::{
	io::{self, IsTerminal, Stdout, Write},
	sync::{Arc, Mutex, OnceLock},
};

/// The output stream used for the TUI (stdout or `/dev/tty` on Unix).
pub enum TerminalWriter {
	Stdout(Stdout),
	#[cfg(unix)]
	Tty(std::fs::File),
}

impl TerminalWriter {
	/// Opens the terminal output stream.
	///
	/// On Unix, when `force_tty` is set or stdout is not a terminal, `/dev/tty`
	/// is used so interactive programs work when stdout is captured (e.g. Helix
	/// `:insert-output`).
	pub fn open(force_tty: bool) -> io::Result<Self> {
		#[cfg(unix)]
		{
			let use_tty = force_tty || !io::stdout().is_terminal();
			if use_tty {
				match std::fs::File::open("/dev/tty") {
					Ok(file) => return Ok(Self::Tty(file)),
					Err(err) if force_tty => return Err(err),
					Err(_) => {}
				}
			}
		}

		#[cfg(not(unix))]
		if force_tty {
			return Err(io::Error::new(
				io::ErrorKind::Unsupported,
				"--tty is only supported on Unix",
			));
		}

		Ok(Self::Stdout(io::stdout()))
	}
}

impl Write for TerminalWriter {
	fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
		match self {
			Self::Stdout(writer) => writer.write(buf),
			#[cfg(unix)]
			Self::Tty(writer) => writer.write(buf),
		}
	}

	fn flush(&mut self) -> io::Result<()> {
		match self {
			Self::Stdout(writer) => writer.flush(),
			#[cfg(unix)]
			Self::Tty(writer) => writer.flush(),
		}
	}
}

/// Shared handle to the terminal writer for crossterm/ratatui and shutdown hooks.
#[derive(Clone)]
pub struct SharedTerminalWriter(pub Arc<Mutex<TerminalWriter>>);

impl Write for SharedTerminalWriter {
	fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
		self.0.lock().expect("terminal writer poisoned").write(buf)
	}

	fn flush(&mut self) -> io::Result<()> {
		self.0
			.lock()
			.expect("terminal writer poisoned")
			.flush()
	}
}

static TERMINAL_WRITER: OnceLock<Arc<Mutex<TerminalWriter>>> = OnceLock::new();

/// Registers the process-wide terminal writer (call once at startup).
pub fn init(writer: Arc<Mutex<TerminalWriter>>) -> Result<(), Arc<Mutex<TerminalWriter>>> {
	TERMINAL_WRITER.set(writer)
}

/// Runs a closure against the terminal writer.
pub fn with_writer<F, R>(f: F) -> io::Result<R>
where
	F: FnOnce(&mut TerminalWriter) -> io::Result<R>,
{
	let writer = TERMINAL_WRITER
		.get()
		.ok_or_else(|| io::Error::other("terminal writer not initialized"))?;
	f(&mut writer.lock().expect("terminal writer poisoned"))
}

/// Executes a crossterm command on the active terminal writer.
pub fn execute<C: Command>(cmd: C) -> io::Result<()> {
	with_writer(|writer| {
		writer.execute(cmd)?;
		Ok(())
	})
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn open_stdout_when_not_forcing_tty() {
		let writer = TerminalWriter::open(false).unwrap();
		assert!(matches!(writer, TerminalWriter::Stdout(_)));
	}

	#[test]
	#[cfg(not(unix))]
	fn open_tty_errors_on_non_unix() {
		let err = TerminalWriter::open(true).unwrap_err();
		assert_eq!(err.kind(), io::ErrorKind::Unsupported);
	}
}
