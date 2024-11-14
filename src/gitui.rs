use std::{cell::RefCell, path::PathBuf};

use asyncgit::{sync::RepoPath, AsyncGitNotification};
use crossbeam_channel::{unbounded, Receiver};
use crossterm::event::{KeyCode, KeyModifiers};

use crate::{
	app::App, draw, input::Input, keys::KeyConfig, ui::style::Theme,
	AsyncAppNotification,
};

struct Gitui {
	app: crate::app::App,
	_rx_git: Receiver<AsyncGitNotification>,
	_rx_app: Receiver<AsyncAppNotification>,
}

impl Gitui {
	fn new(path: RepoPath) -> Self {
		let (tx_git, rx_git) = unbounded();
		let (tx_app, rx_app) = unbounded();

		let input = Input::new();

		let theme = Theme::init(&PathBuf::new());
		let key_config = KeyConfig::default();

		let app = App::new(
			RefCell::new(path),
			tx_git,
			tx_app,
			input.clone(),
			theme,
			key_config.clone(),
		)
		.unwrap();

		Self {
			app,
			_rx_git: rx_git,
			_rx_app: rx_app,
		}
	}

	fn draw<B: ratatui::backend::Backend>(
		&mut self,
		terminal: &mut ratatui::Terminal<B>,
	) {
		draw(terminal, &self.app).unwrap();
	}

	fn update_async(&mut self, event: crate::AsyncNotification) {
		self.app.update_async(event).unwrap();
	}

	fn input_event(
		&mut self,
		code: KeyCode,
		modifiers: KeyModifiers,
	) {
		let event = crossterm::event::KeyEvent::new(code, modifiers);
		self.app
			.event(crate::input::InputEvent::Input(
				crossterm::event::Event::Key(event),
			))
			.unwrap();
	}

	fn update(&mut self) {
		self.app.update().unwrap();
	}
}

#[cfg(test)]
mod tests {
	use std::{thread::sleep, time::Duration};

	use asyncgit::{sync::RepoPath, AsyncGitNotification};
	use crossterm::event::{KeyCode, KeyModifiers};
	use git2_testing::repo_init;
	use insta::assert_snapshot;
	use ratatui::{backend::TestBackend, Terminal};

	use crate::{gitui::Gitui, AsyncNotification};

	// Macro adapted from: https://insta.rs/docs/cmd/
	macro_rules! apply_common_filters {
		{} => {
			let mut settings = insta::Settings::clone_current();
			// MacOS Temp Folder
			settings.add_filter(r" *\[…\]\S+?/T/\S+", "[TEMP_FILE]");
			// Linux Temp Folder
			settings.add_filter(r" */tmp/\.tmp\S+", "[TEMP_FILE]");
			// Windows Temp folder
			settings.add_filter(r" *\[…\].*/Local/Temp/\S+", "[TEMP_FILE]");
			// Commit ids that follow a vertical bar
			settings.add_filter(r"│[a-z0-9]{7} ", "│[AAAAA] ");
			let _bound = settings.bind_to_scope();
		}
	}

	#[test]
	fn gitui_starts() {
		apply_common_filters!();

		let (temp_dir, _repo) = repo_init();
		let path: RepoPath = temp_dir.path().to_str().unwrap().into();

		let mut terminal =
			Terminal::new(TestBackend::new(120, 40)).unwrap();

		let mut gitui = Gitui::new(path);

		gitui.draw(&mut terminal);

		sleep(Duration::from_millis(500));

		assert_snapshot!("app_loading", terminal.backend());

		let event =
			AsyncNotification::Git(AsyncGitNotification::Status);
		gitui.update_async(event);

		sleep(Duration::from_millis(500));

		gitui.draw(&mut terminal);

		assert_snapshot!("app_loading_finished", terminal.backend());

		gitui.input_event(KeyCode::Char('2'), KeyModifiers::empty());

		sleep(Duration::from_millis(500));

		gitui.update();

		gitui.draw(&mut terminal);

		assert_snapshot!(
			"app_log_tab_showing_one_commit",
			terminal.backend()
		);
	}
}
