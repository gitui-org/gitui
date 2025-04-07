use crate::{
	app::Environment,
	components::{
		visibility_blocking, CommandBlocking, CommandInfo,
		CommitList, Component, DrawableComponent, EventState,
	},
	keys::{key_match, SharedKeyConfig},
	popups::InspectCommitOpen,
	queue::{Action, InternalEvent, Queue, StackablePopupOpen},
	strings,
};
use anyhow::Result;
use asyncgit::sync::{self, CommitId, RepoPath, RepoPathRef};
use crossterm::event::Event;

pub struct StashList {
	repo: RepoPathRef,
	list: CommitList,
	visible: bool,
	queue: Queue,
	key_config: SharedKeyConfig,
}

impl StashList {
	///
	pub fn new(env: &Environment) -> Self {
		Self {
			visible: false,
			list: CommitList::new(
				env,
				&strings::stashlist_title(&env.key_config),
			),
			queue: env.queue.clone(),
			key_config: env.key_config.clone(),
			repo: env.repo.clone(),
		}
	}

	///
	pub fn update(&mut self) -> Result<()> {
		if self.is_visible() {
			let stashes = sync::get_stashes(&self.repo.borrow())?;
			self.list.set_commits(stashes.into_iter().collect());
		}

		Ok(())
	}

	fn apply_stash(&self) {
		if let Some(e) = self.list.selected_entry() {
			match sync::stash_apply(&self.repo.borrow(), e.id, false)
			{
				Ok(()) => {
					self.queue.push(InternalEvent::TabSwitchStatus);
				}
				Err(e) => {
					self.queue.push(InternalEvent::ShowErrorMsg(
						format!("stash apply error:\n{e}",),
					));
				}
			}
		}
	}

	fn drop_stash(&self) {
		if self.list.marked_count() > 0 {
			self.queue.push(InternalEvent::ConfirmAction(
				Action::StashDrop(self.list.marked_commits()),
			));
		} else if let Some(e) = self.list.selected_entry() {
			self.queue.push(InternalEvent::ConfirmAction(
				Action::StashDrop(vec![e.id]),
			));
		}
	}

	fn pop_stash(&self) {
		if let Some(e) = self.list.selected_entry() {
			self.queue.push(InternalEvent::ConfirmAction(
				Action::StashPop(e.id),
			));
		}
	}

	fn inspect(&self) {
		if let Some(e) = self.list.selected_entry() {
			self.queue.push(InternalEvent::OpenPopup(
				StackablePopupOpen::InspectCommit(
					InspectCommitOpen::new(e.id),
				),
			));
		}
	}

	/// Called when a pending stash action has been confirmed
	pub fn action_confirmed(
		&mut self,
		repo: &RepoPath,
		action: &Action,
	) -> Result<()> {
		match action {
			Action::StashDrop(ids) => self.drop(repo, ids)?,
			Action::StashPop(id) => self.pop(repo, *id)?,
			_ => (),
		}

		Ok(())
	}

	fn drop(
		&mut self,
		repo: &RepoPath,
		ids: &[CommitId],
	) -> Result<()> {
		for id in ids {
			sync::stash_drop(repo, *id)?;
		}

		self.list.clear_marked();
		self.update()?;

		Ok(())
	}

	fn pop(&mut self, repo: &RepoPath, id: CommitId) -> Result<()> {
		sync::stash_pop(repo, id)?;

		self.list.clear_marked();
		self.update()?;

		self.queue.push(InternalEvent::TabSwitchStatus);

		Ok(())
	}
}

impl DrawableComponent for StashList {
	fn draw(
		&self,
		f: &mut ratatui::Frame,
		rect: ratatui::layout::Rect,
	) -> Result<()> {
		self.list.draw(f, rect)?;

		Ok(())
	}
}

impl Component for StashList {
	fn commands(
		&self,
		out: &mut Vec<CommandInfo>,
		force_all: bool,
	) -> CommandBlocking {
		if self.visible || force_all {
			self.list.commands(out, force_all);

			let selection_valid =
				self.list.selected_entry().is_some();
			out.push(CommandInfo::new(
				strings::commands::stashlist_pop(&self.key_config),
				selection_valid,
				true,
			));
			out.push(CommandInfo::new(
				strings::commands::stashlist_apply(&self.key_config),
				selection_valid,
				true,
			));
			out.push(CommandInfo::new(
				strings::commands::stashlist_drop(
					&self.key_config,
					self.list.marked_count(),
				),
				selection_valid,
				true,
			));
			out.push(CommandInfo::new(
				strings::commands::stashlist_inspect(
					&self.key_config,
				),
				selection_valid,
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
			if self.list.event(ev)?.is_consumed() {
				return Ok(EventState::Consumed);
			}

			if let Event::Key(k) = ev {
				if key_match(k, self.key_config.keys.enter) {
					self.pop_stash();
				} else if key_match(
					k,
					self.key_config.keys.stash_apply,
				) {
					self.apply_stash();
				} else if key_match(
					k,
					self.key_config.keys.stash_drop,
				) {
					self.drop_stash();
				} else if key_match(
					k,
					self.key_config.keys.stash_open,
				) {
					self.inspect();
				}
			}
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
