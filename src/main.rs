#![forbid(unsafe_code)]
#![deny(
	unused_imports,
	unused_must_use,
	dead_code,
	unstable_name_collisions,
	unused_assignments
)]
#![deny(clippy::all, clippy::perf, clippy::nursery, clippy::pedantic)]
#![deny(
	clippy::unwrap_used,
	clippy::filetype_is_file,
	clippy::cargo,
	clippy::unwrap_used,
	clippy::panic,
	clippy::match_like_matches_macro
)]
#![allow(clippy::module_name_repetitions)]
#![allow(
	clippy::multiple_crate_versions,
	clippy::bool_to_int_with_if,
	clippy::module_name_repetitions,
	clippy::empty_docs
)]

//TODO:
// #![deny(clippy::expect_used)]

mod app;
mod args;
mod bug_report;
mod clipboard;
mod cmdbar;
mod components;
mod gitui;
mod input;
mod keys;
mod notify_mutex;
mod options;
mod popup_stack;
mod popups;
mod queue;
mod spinner;
mod string_utils;
mod strings;
mod tabs;
mod ui;
mod watcher;

use crate::{app::App, args::process_cmdline};
use anyhow::{bail, Result};
use app::QuitState;
use asyncgit::{sync::RepoPath, AsyncGitNotification};
use backtrace::Backtrace;
use crossbeam_channel::{Receiver, Select};
use crossterm::{
	terminal::{
		disable_raw_mode, enable_raw_mode, EnterAlternateScreen,
		LeaveAlternateScreen,
	},
	ExecutableCommand,
};
use gitui::Gitui;
use input::InputEvent;
use keys::KeyConfig;
use ratatui::backend::CrosstermBackend;
use scopeguard::defer;
use std::{
	io::{self, Stdout},
	panic, process,
	time::{Duration, Instant},
};
use ui::style::Theme;

type Terminal = ratatui::Terminal<CrosstermBackend<io::Stdout>>;

static TICK_INTERVAL: Duration = Duration::from_secs(5);
static SPINNER_INTERVAL: Duration = Duration::from_millis(80);

///
#[derive(Clone)]
pub enum QueueEvent {
	Tick,
	Notify,
	SpinnerUpdate,
	AsyncEvent(AsyncNotification),
	InputEvent(InputEvent),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyntaxHighlightProgress {
	Progress,
	Done,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AsyncAppNotification {
	///
	SyntaxHighlighting(SyntaxHighlightProgress),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AsyncNotification {
	///
	App(AsyncAppNotification),
	///
	Git(AsyncGitNotification),
}

#[derive(Clone, Copy, PartialEq)]
enum Updater {
	Ticker,
	NotifyWatcher,
}

fn main() -> Result<()> {
	let app_start = Instant::now();

	let cliargs = process_cmdline()?;

	asyncgit::register_tracing_logging();

	if !valid_path(&cliargs.repo_path) {
		eprintln!("invalid path\nplease run gitui inside of a non-bare git repository");
		return Ok(());
	}

	let key_config = KeyConfig::init()
		.map_err(|e| eprintln!("KeyConfig loading error: {e}"))
		.unwrap_or_default();
	let theme = Theme::init(&cliargs.theme);

	setup_terminal()?;
	defer! {
		shutdown_terminal();
	}

	set_panic_handlers()?;

	let mut terminal = start_terminal(io::stdout())?;
	let mut repo_path = cliargs.repo_path;

	let updater = if cliargs.notify_watcher {
		Updater::NotifyWatcher
	} else {
		Updater::Ticker
	};

	loop {
		let quit_state = run_app(
			app_start,
			repo_path.clone(),
			theme.clone(),
			key_config.clone(),
			updater,
			&mut terminal,
		)?;

		match quit_state {
			QuitState::OpenSubmodule(p) => {
				repo_path = p;
			}
			_ => break,
		}
	}

	Ok(())
}

fn run_app(
	app_start: Instant,
	repo: RepoPath,
	theme: Theme,
	key_config: KeyConfig,
	updater: Updater,
	terminal: &mut Terminal,
) -> Result<QuitState, anyhow::Error> {
	let mut gitui =
		Gitui::new(repo.clone(), theme, &key_config, updater)?;

	log::trace!("app start: {} ms", app_start.elapsed().as_millis());

	gitui.run_main_loop(terminal)
}

fn setup_terminal() -> Result<()> {
	enable_raw_mode()?;
	io::stdout().execute(EnterAlternateScreen)?;
	Ok(())
}

fn shutdown_terminal() {
	let leave_screen =
		io::stdout().execute(LeaveAlternateScreen).map(|_f| ());

	if let Err(e) = leave_screen {
		eprintln!("leave_screen failed:\n{e}");
	}

	let leave_raw_mode = disable_raw_mode();

	if let Err(e) = leave_raw_mode {
		eprintln!("leave_raw_mode failed:\n{e}");
	}
}

fn draw<B: ratatui::backend::Backend>(
	terminal: &mut ratatui::Terminal<B>,
	app: &App,
) -> io::Result<()> {
	if app.requires_redraw() {
		terminal.clear()?;
	}

	terminal.draw(|f| {
		if let Err(e) = app.draw(f) {
			log::error!("failed to draw: {:?}", e);
		}
	})?;

	Ok(())
}

fn valid_path(repo_path: &RepoPath) -> bool {
	let error = asyncgit::sync::repo_open_error(repo_path);
	if let Some(error) = &error {
		log::error!("repo open error: {error}");
	}
	error.is_none()
}

fn select_event(
	rx_input: &Receiver<InputEvent>,
	rx_git: &Receiver<AsyncGitNotification>,
	rx_app: &Receiver<AsyncAppNotification>,
	rx_ticker: &Receiver<Instant>,
	rx_notify: &Receiver<()>,
	rx_spinner: &Receiver<Instant>,
) -> Result<QueueEvent> {
	let mut sel = Select::new();

	sel.recv(rx_input);
	sel.recv(rx_git);
	sel.recv(rx_app);
	sel.recv(rx_ticker);
	sel.recv(rx_notify);
	sel.recv(rx_spinner);

	let oper = sel.select();
	let index = oper.index();

	let ev = match index {
		0 => oper.recv(rx_input).map(QueueEvent::InputEvent),
		1 => oper.recv(rx_git).map(|e| {
			QueueEvent::AsyncEvent(AsyncNotification::Git(e))
		}),
		2 => oper.recv(rx_app).map(|e| {
			QueueEvent::AsyncEvent(AsyncNotification::App(e))
		}),
		3 => oper.recv(rx_ticker).map(|_| QueueEvent::Notify),
		4 => oper.recv(rx_notify).map(|()| QueueEvent::Notify),
		5 => oper.recv(rx_spinner).map(|_| QueueEvent::SpinnerUpdate),
		_ => bail!("unknown select source"),
	}?;

	Ok(ev)
}

fn start_terminal(buf: Stdout) -> io::Result<Terminal> {
	let backend = CrosstermBackend::new(buf);
	let mut terminal = Terminal::new(backend)?;
	terminal.hide_cursor()?;
	terminal.clear()?;

	Ok(terminal)
}

// do log::error! and eprintln! in one line, pass string, error and backtrace
macro_rules! log_eprintln {
	($string:expr, $e:expr, $bt:expr) => {
		log::error!($string, $e, $bt);
		eprintln!($string, $e, $bt);
	};
}

fn set_panic_handlers() -> Result<()> {
	// regular panic handler
	panic::set_hook(Box::new(|e| {
		let backtrace = Backtrace::new();
		shutdown_terminal();
		log_eprintln!("\nGitUI was close due to an unexpected panic.\nPlease file an issue on https://github.com/extrawurst/gitui/issues with the following info:\n\n{:?}\ntrace:\n{:?}", e, backtrace);
	}));

	// global threadpool
	rayon_core::ThreadPoolBuilder::new()
		.panic_handler(|e| {
			let backtrace = Backtrace::new();
			shutdown_terminal();
			log_eprintln!("\nGitUI was close due to an unexpected panic.\nPlease file an issue on https://github.com/extrawurst/gitui/issues with the following info:\n\n{:?}\ntrace:\n{:?}", e, backtrace);
			process::abort();
		})
		.num_threads(4)
		.build_global()?;

	Ok(())
}
