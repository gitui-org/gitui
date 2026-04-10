use crate::components::{
	command_pump, event_pump, visibility_blocking, CommandBlocking,
	CommandInfo, CommitDetailsComponent, Component, DiffComponent,
	DrawableComponent, EventState,
};
use crate::{
	accessors,
	app::Environment,
	keys::{key_match, SharedKeyConfig},
	options::SharedOptions,
	popups::InspectCommitOpen,
	queue::{InternalEvent, Queue, StackablePopupOpen},
	strings,
};
use anyhow::Result;
use asyncgit::{
	sync::{self, commit_files::OldNew, CommitId, RepoPathRef},
	AsyncDiff, AsyncGitNotification, CommitFilesParams, DiffParams,
	DiffType,
};
use crossterm::event::Event;
use ratatui::{
	layout::{Constraint, Direction, Layout, Rect},
	widgets::Clear,
	Frame,
};

pub struct CompareCommitsPopup {
	repo: RepoPathRef,
	open_request: Option<InspectCommitOpen>,
	diff: DiffComponent,
	details: CommitDetailsComponent,
	git_diff: AsyncDiff,
	visible: bool,
	key_config: SharedKeyConfig,
	queue: Queue,
	options: SharedOptions,
}

impl DrawableComponent for CompareCommitsPopup {
	fn draw(&self, f: &mut Frame, rect: Rect) -> Result<()> {
		if self.is_visible() {
			let percentages = if self.diff.focused() {
				(0, 100)
			} else {
				(50, 50)
			};

			let chunks = Layout::default()
				.direction(Direction::Horizontal)
				.constraints(
					[
						Constraint::Percentage(percentages.0),
						Constraint::Percentage(percentages.1),
					]
					.as_ref(),
				)
				.split(rect);

			f.render_widget(Clear, rect);

			self.details.draw(f, chunks[0])?;
			self.diff.draw(f, chunks[1])?;
		}

		Ok(())
	}
}

impl Component for CompareCommitsPopup {
	fn commands(
		&self,
		out: &mut Vec<CommandInfo>,
		force_all: bool,
	) -> CommandBlocking {
		if self.is_visible() || force_all {
			command_pump(
				out,
				force_all,
				self.components().as_slice(),
			);

			out.push(
				CommandInfo::new(
					strings::commands::close_popup(&self.key_config),
					true,
					true,
				)
				.order(1),
			);

			out.push(CommandInfo::new(
				strings::commands::diff_focus_right(&self.key_config),
				self.can_focus_diff(),
				!self.diff.focused() || force_all,
			));

			out.push(CommandInfo::new(
				strings::commands::diff_focus_left(&self.key_config),
				true,
				self.diff.focused() || force_all,
			));
		}

		visibility_blocking(self)
	}

	fn event(&mut self, ev: &Event) -> Result<EventState> {
		if self.is_visible() {
			if event_pump(ev, self.components_mut().as_mut_slice())?
				.is_consumed()
			{
				if !self.details.is_visible() {
					self.hide_stacked(true);
				}
				return Ok(EventState::Consumed);
			}

			if let Event::Key(e) = ev {
				if key_match(e, self.key_config.keys.exit_popup) {
					if self.diff.focused() {
						self.details.focus(true);
						self.diff.focus(false);
					} else {
						self.hide_stacked(false);
					}
				} else if key_match(
					e,
					self.key_config.keys.move_right,
				) && self.can_focus_diff()
				{
					self.details.focus(false);
					self.diff.focus(true);
				} else if key_match(e, self.key_config.keys.move_left)
				{
					self.hide_stacked(false);
				}

				return Ok(EventState::Consumed);
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
		self.details.show()?;
		self.details.focus(true);
		self.diff.focus(false);
		self.update()?;
		Ok(())
	}
}

impl CompareCommitsPopup {
	accessors!(self, [diff, details]);

	///
	pub fn new(env: &Environment) -> Self {
		Self {
			repo: env.repo.clone(),
			details: CommitDetailsComponent::new(env),
			diff: DiffComponent::new(env, true),
			open_request: None,
			git_diff: AsyncDiff::new(
				env.repo.borrow().clone(),
				&env.sender_git,
			),
			visible: false,
			key_config: env.key_config.clone(),
			queue: env.queue.clone(),
			options: env.options.clone(),
		}
	}

	///
	pub fn open(&mut self, open: InspectCommitOpen) -> Result<()> {
		let compare_id = if let Some(compare_id) = open.compare_id {
			compare_id
		} else {
			sync::get_head_tuple(&self.repo.borrow())?.id
		};
		self.open_request = Some(InspectCommitOpen {
			commit_id: open.commit_id,
			compare_id: Some(compare_id),
			tags: open.tags,
		});
		self.show()?;

		Ok(())
	}

	///
	pub fn any_work_pending(&self) -> bool {
		self.git_diff.is_pending() || self.details.any_work_pending()
	}

	///
	pub fn update_git(
		&mut self,
		ev: AsyncGitNotification,
	) -> Result<()> {
		if self.is_visible() {
			if ev == AsyncGitNotification::CommitFiles {
				self.update()?;
			} else if ev == AsyncGitNotification::Diff {
				self.update_diff()?;
			}
		}

		Ok(())
	}

	fn get_ids(&self) -> Option<OldNew<CommitId>> {
		let other = self
			.open_request
			.as_ref()
			.and_then(|open| open.compare_id);

		let this =
			self.open_request.as_ref().map(|open| open.commit_id);

		Some(OldNew {
			old: other?,
			new: this?,
		})
	}

	/// called when any tree component changed selection
	pub fn update_diff(&mut self) -> Result<()> {
		if self.is_visible() {
			if let Some(ids) = self.get_ids() {
				if let Some(f) = self.details.files().selection_file()
				{
					let diff_params = DiffParams {
						path: f.path.clone(),
						diff_type: DiffType::Commits(ids),
						options: self.options.borrow().diff_options(),
					};

					if let Some((params, last)) =
						self.git_diff.last()?
					{
						if params == diff_params {
							self.diff.update(f.path, false, last);
							return Ok(());
						}
					}

					self.git_diff.request(diff_params)?;
					self.diff.clear(true);
					return Ok(());
				}
			}

			self.diff.clear(false);
		}

		Ok(())
	}

	fn update(&mut self) -> Result<()> {
		self.details.set_commits(
			self.get_ids().map(CommitFilesParams::from),
			None,
		)?;
		self.update_diff()?;

		Ok(())
	}

	fn can_focus_diff(&self) -> bool {
		self.details.files().selection_file().is_some()
	}

	fn hide_stacked(&mut self, stack: bool) {
		self.hide();
		if stack {
			if let Some(request) = self.open_request.clone() {
				self.queue.push(InternalEvent::PopupStackPush(
					StackablePopupOpen::CompareCommits(request),
				));
			}
		} else {
			self.queue.push(InternalEvent::PopupStackPop);
		}
	}
}
