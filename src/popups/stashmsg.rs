use crate::components::{
	visibility_blocking, CommandBlocking, CommandInfo, Component,
	DrawableComponent, EventState, InputType, TextInputComponent,
};
use crate::{
	app::Environment,
	keys::{key_match, SharedKeyConfig},
	queue::{AppTabs, InternalEvent, NeedsUpdate, Queue},
	strings,
	tabs::StashingOptions,
	AsyncAppNotification,
};
use anyhow::Result;
use asyncgit::sync::{self, RepoPathRef};
use crossbeam_channel::Sender;
use crossterm::event::Event;
use ratatui::{layout::Rect, Frame};
use std::{
	sync::{Arc, Mutex},
	thread,
};

const STASHING_MSG: &str = "Stashing...";

pub struct StashMsgPopup {
	repo: RepoPathRef,
	options: StashingOptions,
	input: TextInputComponent,
	queue: Queue,
	key_config: SharedKeyConfig,
	sender_app: Sender<AsyncAppNotification>,
	stashing: bool,
	stash_result: Arc<Mutex<Option<Result<(), String>>>>,
}

impl DrawableComponent for StashMsgPopup {
	fn draw(&self, f: &mut Frame, rect: Rect) -> Result<()> {
		self.input.draw(f, rect)?;

		Ok(())
	}
}

impl Component for StashMsgPopup {
	fn commands(
		&self,
		out: &mut Vec<CommandInfo>,
		force_all: bool,
	) -> CommandBlocking {
		if self.is_visible() || force_all {
			self.input.commands(out, force_all);

			if !self.stashing {
				out.push(CommandInfo::new(
					strings::commands::stashing_confirm_msg(
						&self.key_config,
					),
					true,
					true,
				));
			}
		}

		visibility_blocking(self)
	}

	fn event(&mut self, ev: &Event) -> Result<EventState> {
		if self.is_visible() {
			if self.stashing {
				return Ok(EventState::Consumed);
			}

			if self.input.event(ev)?.is_consumed() {
				return Ok(EventState::Consumed);
			}

			if let Event::Key(e) = ev {
				if key_match(e, self.key_config.keys.enter) {
					self.start_stash_async()?;
				}

				return Ok(EventState::Consumed);
			}
		}
		Ok(EventState::NotConsumed)
	}

	fn is_visible(&self) -> bool {
		self.input.is_visible()
	}

	fn hide(&mut self) {
		self.input.hide();
		self.stashing = false;
	}

	fn show(&mut self) -> Result<()> {
		self.stashing = false;
		self.input.show()?;

		Ok(())
	}
}

impl StashMsgPopup {
	///
	pub fn new(env: &Environment) -> Self {
		Self {
			options: StashingOptions::default(),
			queue: env.queue.clone(),
			input: TextInputComponent::new(
				env,
				&strings::stash_popup_title(&env.key_config),
				&strings::stash_popup_msg(&env.key_config),
				true,
			)
			.with_input_type(InputType::Singleline),
			key_config: env.key_config.clone(),
			repo: env.repo.clone(),
			sender_app: env.sender_app.clone(),
			stashing: false,
			stash_result: Arc::new(Mutex::new(None)),
		}
	}

	///
	pub const fn options(&mut self, options: StashingOptions) {
		self.options = options;
	}

	///
	pub fn on_stash_save_done(&mut self) {
		if !self.stashing {
			return;
		}

		let error = self
			.stash_result
			.lock()
			.ok()
			.and_then(|mut guard| guard.take())
			.and_then(Result::err);

		self.complete_stash(error);
	}

	fn complete_stash(&mut self, error: Option<String>) {
		self.stashing = false;

		if let Some(msg) = error {
			self.hide();
			self.queue.push(InternalEvent::ShowErrorMsg(msg));
		} else {
			self.input.clear();
			self.hide();
			self.queue
				.push(InternalEvent::TabSwitch(AppTabs::Stashlist));
			self.queue.push(InternalEvent::Update(NeedsUpdate::ALL));
		}
	}

	fn start_stash_async(&mut self) -> Result<()> {
		let repo_path = self.repo.borrow().clone();
		let message = if self.input.get_text().is_empty() {
			None
		} else {
			Some(self.input.get_text().to_string())
		};

		self.stashing = true;
		self.input.enabled(false);
		self.input.set_text(STASHING_MSG.into());

		let options = self.options;
		let sender_app = self.sender_app.clone();
		let stash_result = Arc::clone(&self.stash_result);
		*stash_result.lock().map_err(|_| {
			anyhow::anyhow!("stash result lock poisoned")
		})? = None;

		thread::spawn(move || {
			let result = sync::stash_save(
				&repo_path,
				message.as_deref(),
				options.stash_untracked,
				options.keep_index,
			)
			.map(|_| ())
			.map_err(|e| {
				format!(
					"stash error:\n{e}\noptions:\n{options:?}",
				)
			});

			if let Ok(mut guard) = stash_result.lock() {
				*guard = Some(result);
			}
			let _ = sender_app.send(AsyncAppNotification::StashSaveDone);
		});

		Ok(())
	}
}
