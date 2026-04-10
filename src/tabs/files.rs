use std::path::{Path, PathBuf};

use crate::{
	app::Environment,
	components::{
		visibility_blocking, CommandBlocking, CommandInfo, Component,
		DrawableComponent, EventState, RevisionFilesComponent,
	},
	AsyncNotification,
};
use anyhow::Result;
use asyncgit::sync::{self, RepoPathRef};

pub struct FilesTab {
	repo: RepoPathRef,
	visible: bool,
	files: RevisionFilesComponent,
}

impl FilesTab {
	///
	pub fn new(
		env: &Environment,
		select_file: Option<PathBuf>,
	) -> Self {
		Self {
			visible: false,
			files: RevisionFilesComponent::new(env, select_file),
			repo: env.repo.clone(),
		}
	}

	///
	pub fn update(&mut self) -> Result<()> {
		if self.is_visible() {
			if let Ok(head) = sync::get_head(&self.repo.borrow()) {
				self.files.set_commit(head)?;
			}
		}

		Ok(())
	}

	///
	pub fn anything_pending(&self) -> bool {
		self.files.any_work_pending()
	}

	///
	pub fn update_async(
		&mut self,
		ev: AsyncNotification,
	) -> Result<()> {
		if self.is_visible() {
			self.files.update(ev)?;
		}

		Ok(())
	}

	pub fn file_finder_update(&mut self, file: &Path) {
		self.files.find_file(file);
	}
}

impl DrawableComponent for FilesTab {
	fn draw(
		&self,
		f: &mut ratatui::Frame,
		rect: ratatui::layout::Rect,
	) -> Result<()> {
		if self.is_visible() {
			self.files.draw(f, rect)?;
		}
		Ok(())
	}
}

impl Component for FilesTab {
	fn commands(
		&self,
		out: &mut Vec<CommandInfo>,
		force_all: bool,
	) -> CommandBlocking {
		if self.visible || force_all {
			return self.files.commands(out, force_all);
		}

		visibility_blocking(self)
	}

	fn event(
		&mut self,
		ev: &crossterm::event::Event,
	) -> Result<EventState> {
		if self.visible {
			return self.files.event(ev);
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
		self.update()?;
		Ok(())
	}
}
