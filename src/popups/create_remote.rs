use anyhow::Result;
use asyncgit::sync::{self, validate_remote_name, RepoPathRef};
use crossterm::event::Event;
use easy_cast::Cast;
use ratatui::{widgets::Paragraph, Frame};

use crate::{
	app::Environment,
	components::{
		visibility_blocking, CommandBlocking, CommandInfo, Component,
		DrawableComponent, EventState, InputType, TextInputComponent,
	},
	keys::{key_match, SharedKeyConfig},
	queue::{InternalEvent, NeedsUpdate, Queue},
	strings,
	ui::style::SharedTheme,
};

#[derive(Default)]
enum State {
	// first we ask for a name for a new remote
	#[default]
	Name,
	// second we ask for a url and carry with us the name previously entered
	Url {
		name: String,
	},
}

pub struct CreateRemotePopup {
	repo: RepoPathRef,
	input: TextInputComponent,
	queue: Queue,
	key_config: SharedKeyConfig,
	state: State,
	theme: SharedTheme,
}

impl DrawableComponent for CreateRemotePopup {
	fn draw(
		&self,
		f: &mut ratatui::Frame,
		rect: ratatui::prelude::Rect,
	) -> anyhow::Result<()> {
		if self.is_visible() {
			self.input.draw(f, rect)?;
			self.draw_warnings(f);
		}
		Ok(())
	}
}

impl Component for CreateRemotePopup {
	fn commands(
		&self,
		out: &mut Vec<CommandInfo>,
		force_all: bool,
	) -> CommandBlocking {
		if self.is_visible() || force_all {
			self.input.commands(out, force_all);

			out.push(CommandInfo::new(
				strings::commands::remote_confirm_name_msg(
					&self.key_config,
				),
				true,
				true,
			));
		}
		visibility_blocking(self)
	}

	fn event(
		&mut self,
		ev: &crossterm::event::Event,
	) -> Result<EventState> {
		if self.is_visible() {
			if self.input.event(ev)?.is_consumed() {
				return Ok(EventState::Consumed);
			}

			if let Event::Key(e) = ev {
				if key_match(e, self.key_config.keys.enter) {
					self.handle_submit();
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
		self.input.clear();
		self.input.set_title(
			strings::create_remote_popup_title_name(&self.key_config),
		);
		self.input.set_default_msg(
			strings::create_remote_popup_msg_name(&self.key_config),
		);

		self.input.show()?;

		Ok(())
	}
}

impl CreateRemotePopup {
	pub fn new(env: &Environment) -> Self {
		Self {
			repo: env.repo.clone(),
			queue: env.queue.clone(),
			input: TextInputComponent::new(env, "", "", true)
				.with_input_type(InputType::Singleline),
			key_config: env.key_config.clone(),
			state: State::Name,
			theme: env.theme.clone(),
		}
	}

	pub fn open(&mut self) -> Result<()> {
		self.state = State::Name;
		self.input.clear();
		self.show()?;

		Ok(())
	}

	fn draw_warnings(&self, f: &mut Frame) {
		let remote_name = match self.state {
			State::Name => self.input.get_text(),
			State::Url { .. } => return,
		};

		if !remote_name.is_empty() {
			let valid = validate_remote_name(remote_name);

			if !valid {
				let msg = strings::remote_name_invalid();
				let msg_length: u16 = msg.len().cast();
				let w = Paragraph::new(msg)
					.style(self.theme.text_danger());

				let rect = {
					let mut rect = self.input.get_area();
					rect.y += rect.height.saturating_sub(1);
					rect.height = 1;
					let offset =
						rect.width.saturating_sub(msg_length + 1);
					rect.width =
						rect.width.saturating_sub(offset + 1);
					rect.x += offset;

					rect
				};

				f.render_widget(w, rect);
			}
		}
	}

	fn handle_submit(&mut self) {
		match &self.state {
			State::Name => {
				self.state = State::Url {
					name: self.input.get_text().to_string(),
				};

				self.input.clear();
				self.input.set_title(
					strings::create_remote_popup_title_url(
						&self.key_config,
					),
				);
				self.input.set_default_msg(
					strings::create_remote_popup_msg_url(
						&self.key_config,
					),
				);
			}
			State::Url { name } => {
				let res = sync::add_remote(
					&self.repo.borrow(),
					name,
					self.input.get_text(),
				);

				match res {
					Ok(()) => {
						self.queue.push(InternalEvent::Update(
							NeedsUpdate::ALL | NeedsUpdate::REMOTES,
						));
					}
					Err(e) => {
						log::error!("create remote: {}", e,);
						self.queue.push(InternalEvent::ShowErrorMsg(
							format!("create remote error:\n{e}",),
						));
					}
				}

				self.hide();
			}
		}
	}
}
