use crate::components::{
	visibility_blocking, CommandBlocking, CommandInfo, Component,
	DrawableComponent, EventState, InputType, TextInputComponent,
};
use crate::{
	app::Environment,
	keys::{key_match, SharedKeyConfig},
	queue::{InternalEvent, NeedsUpdate, Queue},
	strings,
	ui::style::SharedTheme,
};
use anyhow::Result;
use asyncgit::sync::{self, RepoPathRef};
use crossterm::event::Event;
use easy_cast::Cast;
use ratatui::{layout::Rect, widgets::Paragraph, Frame};

pub struct CreateWorktreePopup {
	repo: RepoPathRef,
	input: TextInputComponent,
	queue: Queue,
	key_config: SharedKeyConfig,
	theme: SharedTheme,
}

impl DrawableComponent for CreateWorktreePopup {
	fn draw(&self, f: &mut Frame, rect: Rect) -> Result<()> {
		if self.is_visible() {
			self.input.draw(f, rect)?;
			self.draw_warnings(f);
		}

		Ok(())
	}
}

impl Component for CreateWorktreePopup {
	fn commands(
		&self,
		out: &mut Vec<CommandInfo>,
		force_all: bool,
	) -> CommandBlocking {
		if self.is_visible() || force_all {
			self.input.commands(out, force_all);

			out.push(CommandInfo::new(
				strings::commands::create_worktree_confirm_msg(
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
					self.create_worktree();
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

impl CreateWorktreePopup {
	///
	pub fn new(env: &Environment) -> Self {
		Self {
			queue: env.queue.clone(),
			input: TextInputComponent::new(
				env,
				&strings::create_worktree_popup_title(
					&env.key_config,
				),
				&strings::create_worktree_popup_msg(&env.key_config),
				true,
			)
			.with_input_type(InputType::Singleline),
			theme: env.theme.clone(),
			key_config: env.key_config.clone(),
			repo: env.repo.clone(),
		}
	}

	///
	pub fn open(&mut self) -> Result<()> {
		self.show()?;

		Ok(())
	}

	///
	pub fn create_worktree(&mut self) {
		let path = self.input.get_text().to_string();

		if path.trim().is_empty() {
			self.hide();
			return;
		}

		let res = sync::create_worktree(&self.repo.borrow(), &path);

		self.input.clear();
		self.hide();

		match res {
			Ok(_) => {
				self.queue
					.push(InternalEvent::Update(NeedsUpdate::ALL));
			}
			Err(e) => {
				log::error!("create worktree: {e}");
				self.queue.push(InternalEvent::ShowErrorMsg(
					format!("create worktree error:\n{e}"),
				));
			}
		}
	}

	fn draw_warnings(&self, f: &mut Frame) {
		let current_text = self.input.get_text();

		let derived_name = std::path::Path::new(current_text)
			.file_name()
			.and_then(|s| s.to_str());

		if let Some(derived_name) = derived_name {
			let valid = sync::validate_branch_name(derived_name)
				.unwrap_or_default();

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
}
