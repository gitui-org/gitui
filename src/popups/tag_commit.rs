use crate::components::{
	visibility_blocking, CommandBlocking, CommandInfo, Component,
	DrawableComponent, EventState, InputType, TextInputComponent,
};
use crate::{
	app::Environment,
	keys::{key_match, SharedKeyConfig},
	queue::{InternalEvent, NeedsUpdate, Queue},
	strings, try_or_popup,
};
use anyhow::Result;
use asyncgit::sync::{
	self, get_config_string, CommitId, RepoPathRef,
};
use crossterm::event::Event;
use ratatui::{layout::Rect, Frame};

enum Mode {
	Name,
	Annotation { tag_name: String },
}

pub struct TagCommitPopup {
	repo: RepoPathRef,
	mode: Mode,
	input: TextInputComponent,
	commit_id: Option<CommitId>,
	queue: Queue,
	key_config: SharedKeyConfig,
}

impl DrawableComponent for TagCommitPopup {
	fn draw(&self, f: &mut Frame, rect: Rect) -> Result<()> {
		self.input.draw(f, rect)?;

		Ok(())
	}
}

impl Component for TagCommitPopup {
	fn commands(
		&self,
		out: &mut Vec<CommandInfo>,
		force_all: bool,
	) -> CommandBlocking {
		if self.is_visible() || force_all {
			self.input.commands(out, force_all);

			let is_annotation_mode =
				matches!(self.mode, Mode::Annotation { .. });

			out.push(CommandInfo::new(
				strings::commands::tag_commit_confirm_msg(
					&self.key_config,
					is_annotation_mode,
				),
				self.is_valid_tag(),
				true,
			));

			out.push(CommandInfo::new(
				strings::commands::tag_annotate_msg(&self.key_config),
				self.is_valid_tag(),
				matches!(self.mode, Mode::Name),
			));
		}

		visibility_blocking(self)
	}

	fn event(&mut self, ev: &Event) -> Result<EventState> {
		if self.is_visible() {
			if let Event::Key(e) = ev {
				let is_annotation_mode =
					matches!(self.mode, Mode::Annotation { .. });

				if !is_annotation_mode
					&& key_match(e, self.key_config.keys.enter)
					&& self.is_valid_tag()
				{
					try_or_popup!(self, "tag error:", self.tag());
					return Ok(EventState::Consumed);
				}
				if is_annotation_mode
					&& key_match(e, self.key_config.keys.commit)
				{
					try_or_popup!(self, "tag error:", self.tag());
					return Ok(EventState::Consumed);
				} else if key_match(
					e,
					self.key_config.keys.tag_annotate,
				) && self.is_valid_tag()
				{
					self.start_annotate_mode();
					return Ok(EventState::Consumed);
				}
			}

			self.input.event(ev)?;
			return Ok(EventState::Consumed);
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
		self.mode = Mode::Name;
		self.input.set_input_type(InputType::Singleline);
		self.input.set_title(strings::tag_popup_name_title());
		self.input.set_default_msg(strings::tag_popup_name_msg());
		self.input.show()?;

		Ok(())
	}
}

impl TagCommitPopup {
	///
	pub fn new(env: &Environment) -> Self {
		Self {
			queue: env.queue.clone(),
			input: TextInputComponent::new(
				env,
				&strings::tag_popup_name_title(),
				&strings::tag_popup_name_msg(),
				true,
			)
			.with_input_type(InputType::Singleline),
			commit_id: None,
			key_config: env.key_config.clone(),
			repo: env.repo.clone(),
			mode: Mode::Name,
		}
	}

	///
	pub fn open(&mut self, id: CommitId) -> Result<()> {
		self.commit_id = Some(id);
		self.show()?;

		Ok(())
	}

	fn is_valid_tag(&self) -> bool {
		!self.input.get_text().is_empty()
	}

	fn tag_info(&self) -> (String, Option<String>) {
		match &self.mode {
			Mode::Name => (self.input.get_text().into(), None),
			Mode::Annotation { tag_name } => {
				(tag_name.clone(), Some(self.input.get_text().into()))
			}
		}
	}

	pub fn tag(&mut self) -> Result<()> {
		let gpgsign =
			get_config_string(&self.repo.borrow(), "tag.gpgsign")
				.ok()
				.flatten()
				.and_then(|val| val.parse::<bool>().ok())
				.unwrap_or_default();

		anyhow::ensure!(!gpgsign, "config tag.gpgsign=true detected.\ngpg signing not supported.\ndeactivate in your repo/gitconfig to be able to tag without signing.");

		let (tag_name, tag_annotation) = self.tag_info();

		if let Some(commit_id) = self.commit_id {
			let result = sync::tag_commit(
				&self.repo.borrow(),
				&commit_id,
				&tag_name,
				tag_annotation.as_deref(),
			);
			match result {
				Ok(_) => {
					self.input.clear();
					self.hide();

					self.queue.push(InternalEvent::Update(
						NeedsUpdate::ALL,
					));
				}
				Err(e) => {
					// go back to tag name if something goes wrong
					self.input.set_text(tag_name);
					self.hide();

					log::error!("e: {e}");
					self.queue.push(InternalEvent::ShowErrorMsg(
						format!("tag error:\n{e}",),
					));
				}
			}
		}

		Ok(())
	}

	fn start_annotate_mode(&mut self) {
		let tag_name: String = self.input.get_text().into();

		self.input.clear();
		self.input.set_input_type(InputType::Multiline);
		self.input.set_title(strings::tag_popup_annotation_title(
			&tag_name,
		));
		self.input
			.set_default_msg(strings::tag_popup_annotation_msg());
		self.mode = Mode::Annotation { tag_name };
	}
}
