use anyhow::Result;
use asyncgit::sync::{self, RepoPathRef};
use crossterm::event::Event;

use crate::{
	app::Environment,
	components::{
		visibility_blocking, CommandBlocking, CommandInfo, Component,
		DrawableComponent, EventState, InputType, TextInputComponent,
	},
	keys::{key_match, SharedKeyConfig},
	queue::{InternalEvent, NeedsUpdate, Queue},
	strings,
};

pub struct UpdateRemoteUrlPopup {
	repo: RepoPathRef,
	input: TextInputComponent,
	key_config: SharedKeyConfig,
	queue: Queue,
	remote_name: Option<String>,
	initial_url: Option<String>,
}

impl DrawableComponent for UpdateRemoteUrlPopup {
	fn draw(
		&self,
		f: &mut ratatui::Frame,
		rect: ratatui::prelude::Rect,
	) -> anyhow::Result<()> {
		if self.is_visible() {
			self.input.draw(f, rect)?;
		}
		Ok(())
	}
}

impl Component for UpdateRemoteUrlPopup {
	fn commands(
		&self,
		out: &mut Vec<crate::components::CommandInfo>,
		force_all: bool,
	) -> CommandBlocking {
		if self.is_visible() || force_all {
			self.input.commands(out, force_all);

			out.push(CommandInfo::new(
				strings::commands::remote_confirm_url_msg(
					&self.key_config,
				),
				true,
				true,
			));
		}
		visibility_blocking(self)
	}

	fn event(&mut self, ev: &Event) -> Result<EventState> {
		if self.is_visible() {
			if self.input.event(ev)?.is_consumed() {
				return Ok(EventState::Consumed);
			}

			if let Event::Key(e) = ev {
				if key_match(e, self.key_config.keys.enter) {
					self.update_remote_url();
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
	}

	fn show(&mut self) -> Result<()> {
		self.input.show()?;

		Ok(())
	}
}

impl UpdateRemoteUrlPopup {
	pub fn new(env: &Environment) -> Self {
		Self {
			repo: env.repo.clone(),
			input: TextInputComponent::new(
				env,
				&strings::update_remote_url_popup_title(
					&env.key_config,
				),
				&strings::update_remote_url_popup_msg(
					&env.key_config,
				),
				true,
			)
			.with_input_type(InputType::Singleline),
			key_config: env.key_config.clone(),
			queue: env.queue.clone(),
			initial_url: None,
			remote_name: None,
		}
	}

	///
	pub fn open(
		&mut self,
		remote_name: String,
		cur_url: String,
	) -> Result<()> {
		self.input.set_text(cur_url.clone());
		self.remote_name = Some(remote_name);
		self.initial_url = Some(cur_url);
		self.show()?;

		Ok(())
	}

	///
	pub fn update_remote_url(&mut self) {
		if let Some(remote_name) = &self.remote_name {
			let res = sync::update_remote_url(
				&self.repo.borrow(),
				remote_name,
				self.input.get_text(),
			);
			match res {
				Ok(()) => {
					self.queue.push(InternalEvent::Update(
						NeedsUpdate::ALL | NeedsUpdate::REMOTES,
					));
				}
				Err(e) => {
					log::error!("update remote url: {e}");
					self.queue.push(InternalEvent::ShowErrorMsg(
						format!("update remote url error:\n{e}",),
					));
				}
			}
		}
		self.input.clear();
		self.initial_url = None;
		self.hide();
	}
}
