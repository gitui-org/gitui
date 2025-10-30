use crate::components::{
	visibility_blocking, CommandBlocking, CommandInfo, Component,
	DrawableComponent, EventState, InputType, TextInputComponent,
};
use crate::ui::style::SharedTheme;
use crate::{
	app::Environment,
	keys::{key_match, SharedKeyConfig},
	queue::{InternalEvent, NeedsUpdate, Queue},
	strings,
};
use anyhow::Result;
use asyncgit::sync::{self, RepoPathRef};
use crossterm::event::Event;
use easy_cast::Cast;
use ratatui::{layout::Rect, widgets::Paragraph, Frame};

pub struct RenameBranchPopup {
	repo: RepoPathRef,
	input: TextInputComponent,
	branch_ref: Option<String>,
	queue: Queue,
	key_config: SharedKeyConfig,
	theme: SharedTheme,
}

impl DrawableComponent for RenameBranchPopup {
	fn draw(&self, f: &mut Frame, rect: Rect) -> Result<()> {
		if self.is_visible() {
			self.input.draw(f, rect)?;
			self.draw_warnings(f);
		}
		Ok(())
	}
}

impl Component for RenameBranchPopup {
	fn commands(
		&self,
		out: &mut Vec<CommandInfo>,
		force_all: bool,
	) -> CommandBlocking {
		if self.is_visible() || force_all {
			self.input.commands(out, force_all);

			out.push(CommandInfo::new(
				strings::commands::rename_branch_confirm_msg(
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
					self.rename_branch();
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

impl RenameBranchPopup {
	///
	pub fn new(env: &Environment) -> Self {
		Self {
			repo: env.repo.clone(),
			queue: env.queue.clone(),
			input: TextInputComponent::new(
				env,
				&strings::rename_branch_popup_title(&env.key_config),
				&strings::rename_branch_popup_msg(&env.key_config),
				true,
			)
			.with_input_type(InputType::Singleline),
			branch_ref: None,
			key_config: env.key_config.clone(),
			theme: env.theme.clone(),
		}
	}

	///
	pub fn open(
		&mut self,
		branch_ref: String,
		cur_name: String,
	) -> Result<()> {
		self.branch_ref = None;
		self.branch_ref = Some(branch_ref);
		self.input.set_text(cur_name);
		self.show()?;

		Ok(())
	}

	///
	pub fn rename_branch(&mut self) {
		if let Some(br) = &self.branch_ref {
			let res = sync::rename_branch(
				&self.repo.borrow(),
				br,
				self.input.get_text(),
			);

			match res {
				Ok(()) => {
					self.queue.push(InternalEvent::Update(
						NeedsUpdate::ALL,
					));
					self.hide();
					self.queue.push(InternalEvent::SelectBranch);
				}
				Err(e) => {
					log::error!("create branch: {e}");
					self.queue.push(InternalEvent::ShowErrorMsg(
						format!("rename branch error:\n{e}",),
					));
				}
			}
		} else {
			log::error!("create branch: No branch selected");
			self.queue.push(InternalEvent::ShowErrorMsg(
				"rename branch error: No branch selected to rename"
					.to_string(),
			));
		}

		self.input.clear();
	}

	fn draw_warnings(&self, f: &mut Frame) {
		let current_text = self.input.get_text();

		if !current_text.is_empty() {
			let valid = sync::validate_branch_name(current_text)
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
