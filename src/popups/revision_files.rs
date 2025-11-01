use crate::components::{
	visibility_blocking, CommandBlocking, CommandInfo, Component,
	DrawableComponent, EventState, RevisionFilesComponent,
};
use crate::{
	app::Environment,
	keys::{key_match, SharedKeyConfig},
	queue::{InternalEvent, Queue, StackablePopupOpen},
	strings::{self},
	AsyncNotification,
};
use anyhow::Result;
use asyncgit::sync::CommitId;
use crossterm::event::Event;
use ratatui::{layout::Rect, widgets::Clear, Frame};
use std::path::Path;

#[derive(Clone, Debug)]
pub struct FileTreeOpen {
	pub commit_id: CommitId,
}

impl FileTreeOpen {
	pub const fn new(commit_id: CommitId) -> Self {
		Self { commit_id }
	}
}

pub struct RevisionFilesPopup {
	open_request: Option<FileTreeOpen>,
	visible: bool,
	key_config: SharedKeyConfig,
	files: RevisionFilesComponent,
	queue: Queue,
}

impl RevisionFilesPopup {
	///
	pub fn new(env: &Environment) -> Self {
		Self {
			files: RevisionFilesComponent::new(env, None),
			visible: false,
			key_config: env.key_config.clone(),
			open_request: None,
			queue: env.queue.clone(),
		}
	}

	///
	pub fn open(&mut self, request: FileTreeOpen) -> Result<()> {
		self.files.set_commit(request.commit_id)?;
		self.open_request = Some(request);
		self.show()?;

		Ok(())
	}

	///
	pub fn update(&mut self, ev: AsyncNotification) -> Result<()> {
		self.files.update(ev)
	}

	///
	pub fn any_work_pending(&self) -> bool {
		self.files.any_work_pending()
	}

	pub fn file_finder_update(&mut self, file: &Path) {
		self.files.find_file(file);
	}

	fn hide_stacked(&mut self, stack: bool) {
		self.hide();

		if stack {
			if let Some(revision) = self.files.revision() {
				self.queue.push(InternalEvent::PopupStackPush(
					StackablePopupOpen::FileTree(FileTreeOpen {
						commit_id: revision.id,
					}),
				));
			}
		} else {
			self.queue.push(InternalEvent::PopupStackPop);
		}
	}
}

impl DrawableComponent for RevisionFilesPopup {
	fn draw(&self, f: &mut Frame, area: Rect) -> Result<()> {
		if self.is_visible() {
			f.render_widget(Clear, area);

			self.files.draw(f, area)?;
		}

		Ok(())
	}
}

impl Component for RevisionFilesPopup {
	fn commands(
		&self,
		out: &mut Vec<CommandInfo>,
		force_all: bool,
	) -> CommandBlocking {
		if self.is_visible() || force_all {
			out.push(
				CommandInfo::new(
					strings::commands::close_popup(&self.key_config),
					true,
					true,
				)
				.order(1),
			);

			self.files.commands(out, force_all);
		}

		visibility_blocking(self)
	}

	fn event(
		&mut self,
		event: &crossterm::event::Event,
	) -> Result<EventState> {
		if self.is_visible() {
			if let Event::Key(key) = event {
				if key_match(key, self.key_config.keys.exit_popup) {
					self.hide_stacked(false);
				}
			}

			let res = self.files.event(event)?;
			//Note: if this made the files hide we need to stack the popup
			if res == EventState::Consumed && !self.files.is_visible()
			{
				self.hide_stacked(true);
			}

			return Ok(res);
		}

		Ok(EventState::NotConsumed)
	}

	fn is_visible(&self) -> bool {
		self.visible
	}

	fn hide(&mut self) {
		self.visible = false;
	}

	fn show(&mut self) -> Result<()> {
		self.visible = true;

		Ok(())
	}
}
