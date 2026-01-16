use anyhow::Result;
use asyncgit::sync::{self, RepoPathRef};
use crossterm::event::Event;
use easy_cast::Cast;
use ratatui::{layout::Rect, widgets::Paragraph, Frame};

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

pub struct RenameRemotePopup {
	repo: RepoPathRef,
	input: TextInputComponent,
	theme: SharedTheme,
	key_config: SharedKeyConfig,
	queue: Queue,
	initial_name: Option<String>,
}

impl DrawableComponent for RenameRemotePopup {
	fn draw(&self, f: &mut Frame, rect: Rect) -> Result<()> {
		if self.is_visible() {
			self.input.draw(f, rect)?;
			self.draw_warnings(f);
		}
		Ok(())
	}
}

impl Component for RenameRemotePopup {
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

	fn event(&mut self, ev: &Event) -> Result<EventState> {
		if self.is_visible() {
			if self.input.event(ev)?.is_consumed() {
				return Ok(EventState::Consumed);
			}

			if let Event::Key(e) = ev {
				if key_match(e, self.key_config.keys.enter) {
					self.rename_remote();
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

impl RenameRemotePopup {
	///
	pub fn new(env: &Environment) -> Self {
		Self {
			repo: env.repo.clone(),
			input: TextInputComponent::new(
				env,
				&strings::rename_remote_popup_title(&env.key_config),
				&strings::rename_remote_popup_msg(&env.key_config),
				true,
			)
			.with_input_type(InputType::Singleline),
			theme: env.theme.clone(),
			key_config: env.key_config.clone(),
			queue: env.queue.clone(),
			initial_name: None,
		}
	}

	///
	pub fn open(&mut self, cur_name: String) -> Result<()> {
		self.input.set_text(cur_name.clone());
		self.initial_name = Some(cur_name);
		self.show()?;

		Ok(())
	}

	fn draw_warnings(&self, f: &mut Frame) {
		let current_text = self.input.get_text();

		if !current_text.is_empty() {
			let valid = sync::validate_remote_name(current_text);

			if !valid {
				let msg = strings::branch_name_invalid();
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

	///
	pub fn rename_remote(&mut self) {
		if let Some(init_name) = &self.initial_name {
			if init_name != self.input.get_text() {
				let res = sync::rename_remote(
					&self.repo.borrow(),
					init_name,
					self.input.get_text(),
				);
				match res {
					Ok(()) => {
						self.queue.push(InternalEvent::Update(
							NeedsUpdate::ALL | NeedsUpdate::REMOTES,
						));
					}
					Err(e) => {
						log::error!("rename remote: {e}");
						self.queue.push(InternalEvent::ShowErrorMsg(
							format!("rename remote error:\n{e}",),
						));
					}
				}
			}
		}
		self.input.clear();
		self.initial_name = None;
		self.hide();
	}
}
