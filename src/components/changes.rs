use super::{
	status_tree::StatusTreeComponent,
	utils::filetree::{FileTreeItem, FileTreeItemKind},
	CommandBlocking, DrawableComponent,
};
use crate::{
	app::Environment,
	components::{CommandInfo, Component, EventState},
	keys::{key_match, SharedKeyConfig},
	options::SharedOptions,
	queue::{Action, InternalEvent, NeedsUpdate, Queue, ResetItem},
	strings, try_or_popup,
};
use anyhow::Result;
use asyncgit::{
	sync::{self, RepoPathRef},
	StatusItem, StatusItemType,
};
use crossterm::event::Event;
use ratatui::{layout::Rect, Frame};
use std::path::Path;

///
pub struct ChangesComponent {
	repo: RepoPathRef,
	files: StatusTreeComponent,
	is_working_dir: bool,
	queue: Queue,
	key_config: SharedKeyConfig,
	options: SharedOptions,
}

impl ChangesComponent {
	///
	pub fn new(
		env: &Environment,
		title: &str,
		focus: bool,
		is_working_dir: bool,
	) -> Self {
		Self {
			files: StatusTreeComponent::new(env, title, focus),
			is_working_dir,
			queue: env.queue.clone(),
			key_config: env.key_config.clone(),
			options: env.options.clone(),
			repo: env.repo.clone(),
		}
	}

	///
	pub fn set_items(&mut self, list: &[StatusItem]) -> Result<()> {
		self.files.show()?;
		self.files.update(list)?;
		Ok(())
	}

	///
	pub fn selection(&self) -> Option<FileTreeItem> {
		self.files.selection()
	}

	///
	pub fn focus_select(&mut self, focus: bool) {
		self.files.focus(focus);
		self.files.show_selection(focus);
	}

	/// returns true if list is empty
	pub fn is_empty(&self) -> bool {
		self.files.is_empty()
	}

	///
	pub fn is_file_selected(&self) -> bool {
		self.files.is_file_selected()
	}

	fn index_add_remove(&self) -> Result<bool> {
		if let Some(tree_item) = self.selection() {
			if self.is_working_dir {
				if let FileTreeItemKind::File(i) = tree_item.kind {
					let path = Path::new(i.path.as_str());
					match i.status {
						StatusItemType::Deleted => {
							sync::stage_addremoved(
								&self.repo.borrow(),
								path,
							)?;
						}
						_ => sync::stage_add_file(
							&self.repo.borrow(),
							path,
						)?,
					}
				} else {
					let config =
						self.options.borrow().status_show_untracked();

					//TODO: check if we can handle the one file case with it as well
					sync::stage_add_all(
						&self.repo.borrow(),
						tree_item.info.full_path.as_str(),
						config,
					)?;
				}

				//TODO: this might be slow in big repos,
				// in theory we should be able to ask the tree structure
				// if we are currently on a leaf or a lonely branch that
				// would mean that after staging the workdir becomes empty
				if sync::is_workdir_clean(
					&self.repo.borrow(),
					self.options.borrow().status_show_untracked(),
				)? {
					self.queue
						.push(InternalEvent::StatusLastFileMoved);
				}
			} else {
				// this is a staged entry, so lets unstage it
				let path = tree_item.info.full_path.as_str();
				sync::reset_stage(&self.repo.borrow(), path)?;
			}

			return Ok(true);
		}

		Ok(false)
	}

	fn index_add_all(&self) -> Result<()> {
		let config = self.options.borrow().status_show_untracked();

		sync::stage_add_all(&self.repo.borrow(), "*", config)?;

		self.queue.push(InternalEvent::Update(NeedsUpdate::ALL));

		Ok(())
	}

	fn stage_remove_all(&self) -> Result<()> {
		sync::reset_stage(&self.repo.borrow(), "*")?;

		self.queue.push(InternalEvent::Update(NeedsUpdate::ALL));

		Ok(())
	}

	fn dispatch_reset_workdir(&self) -> bool {
		if let Some(tree_item) = self.selection() {
			self.queue.push(InternalEvent::ConfirmAction(
				Action::Reset(ResetItem {
					path: tree_item.info.full_path,
				}),
			));

			return true;
		}
		false
	}

	fn add_to_ignore(&self) -> bool {
		if let Some(tree_item) = self.selection() {
			if let Err(e) = sync::add_to_ignore(
				&self.repo.borrow(),
				&tree_item.info.full_path,
			) {
				self.queue.push(InternalEvent::ShowErrorMsg(
					format!(
						"ignore error:\n{}\nfile:\n{:?}",
						e, tree_item.info.full_path
					),
				));
			} else {
				self.queue
					.push(InternalEvent::Update(NeedsUpdate::ALL));

				return true;
			}
		}

		false
	}
}

impl DrawableComponent for ChangesComponent {
	fn draw(&self, f: &mut Frame, r: Rect) -> Result<()> {
		self.files.draw(f, r)?;

		Ok(())
	}
}

impl Component for ChangesComponent {
	fn commands(
		&self,
		out: &mut Vec<CommandInfo>,
		force_all: bool,
	) -> CommandBlocking {
		self.files.commands(out, force_all);

		let some_selection = self.selection().is_some();

		if self.is_working_dir {
			out.push(CommandInfo::new(
				strings::commands::stage_all(&self.key_config),
				true,
				some_selection && self.focused(),
			));
			out.push(CommandInfo::new(
				strings::commands::stage_item(&self.key_config),
				true,
				some_selection && self.focused(),
			));
			out.push(CommandInfo::new(
				strings::commands::reset_item(&self.key_config),
				true,
				some_selection && self.focused(),
			));
			out.push(CommandInfo::new(
				strings::commands::ignore_item(&self.key_config),
				true,
				some_selection && self.focused(),
			));
		} else {
			out.push(CommandInfo::new(
				strings::commands::unstage_item(&self.key_config),
				true,
				some_selection && self.focused(),
			));
			out.push(CommandInfo::new(
				strings::commands::unstage_all(&self.key_config),
				true,
				some_selection && self.focused(),
			));
		}

		CommandBlocking::PassingOn
	}

	fn event(&mut self, ev: &Event) -> Result<EventState> {
		if self.files.event(ev)?.is_consumed() {
			return Ok(EventState::Consumed);
		}

		if self.focused() {
			if let Event::Key(e) = ev {
				return if key_match(
					e,
					self.key_config.keys.stage_unstage_item,
				) {
					try_or_popup!(
						self,
						"staging error:",
						self.index_add_remove()
					);

					self.queue.push(InternalEvent::Update(
						NeedsUpdate::ALL,
					));
					Ok(EventState::Consumed)
				} else if key_match(
					e,
					self.key_config.keys.status_stage_all,
				) && !self.is_empty()
				{
					if self.is_working_dir {
						try_or_popup!(
							self,
							"staging all error:",
							self.index_add_all()
						);
					} else {
						self.stage_remove_all()?;
					}
					self.queue
						.push(InternalEvent::StatusLastFileMoved);
					Ok(EventState::Consumed)
				} else if key_match(
					e,
					self.key_config.keys.status_reset_item,
				) && self.is_working_dir
				{
					Ok(self.dispatch_reset_workdir().into())
				} else if key_match(
					e,
					self.key_config.keys.status_ignore_file,
				) && self.is_working_dir
					&& !self.is_empty()
				{
					Ok(self.add_to_ignore().into())
				} else {
					Ok(EventState::NotConsumed)
				};
			}
		}

		Ok(EventState::NotConsumed)
	}

	fn focused(&self) -> bool {
		self.files.focused()
	}

	fn focus(&mut self, focus: bool) {
		self.files.focus(focus);
	}

	fn is_visible(&self) -> bool {
		self.files.is_visible()
	}

	fn hide(&mut self) {
		self.files.hide();
	}

	fn show(&mut self) -> Result<()> {
		self.files.show()?;
		Ok(())
	}
}
