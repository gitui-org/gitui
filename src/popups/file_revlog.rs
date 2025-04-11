use crate::{
	app::Environment,
	components::{
		event_pump, visibility_blocking, CommandBlocking,
		CommandInfo, Component, DiffComponent, DrawableComponent,
		EventState, ItemBatch, ScrollType,
	},
	keys::{key_match, SharedKeyConfig},
	options::SharedOptions,
	queue::{InternalEvent, NeedsUpdate, Queue, StackablePopupOpen},
	strings,
	ui::{draw_scrollbar, style::SharedTheme, Orientation},
};
use anyhow::Result;
use asyncgit::{
	sync::{
		diff_contains_file, get_commits_info, CommitId, RepoPathRef,
	},
	AsyncDiff, AsyncGitNotification, AsyncLog, DiffParams, DiffType,
};
use chrono::{DateTime, Local};
use crossbeam_channel::Sender;
use crossterm::event::Event;
use ratatui::{
	layout::{Constraint, Direction, Layout, Rect},
	text::{Line, Span, Text},
	widgets::{Block, Borders, Cell, Clear, Row, Table, TableState},
	Frame,
};

use super::{BlameFileOpen, InspectCommitOpen};

const SLICE_SIZE: usize = 1200;

#[derive(Clone, Debug)]
pub struct FileRevOpen {
	pub file_path: String,
	pub selection: Option<usize>,
}

impl FileRevOpen {
	pub const fn new(file_path: String) -> Self {
		Self {
			file_path,
			selection: None,
		}
	}
}

///
pub struct FileRevlogPopup {
	git_log: Option<AsyncLog>,
	git_diff: AsyncDiff,
	theme: SharedTheme,
	queue: Queue,
	sender: Sender<AsyncGitNotification>,
	diff: DiffComponent,
	visible: bool,
	repo_path: RepoPathRef,
	open_request: Option<FileRevOpen>,
	table_state: std::cell::Cell<TableState>,
	items: ItemBatch,
	count_total: usize,
	key_config: SharedKeyConfig,
	options: SharedOptions,
	current_width: std::cell::Cell<usize>,
	current_height: std::cell::Cell<usize>,
}

impl FileRevlogPopup {
	///
	pub fn new(env: &Environment) -> Self {
		Self {
			theme: env.theme.clone(),
			queue: env.queue.clone(),
			sender: env.sender_git.clone(),
			diff: DiffComponent::new(env, true),
			git_log: None,
			git_diff: AsyncDiff::new(
				env.repo.borrow().clone(),
				&env.sender_git,
			),
			visible: false,
			repo_path: env.repo.clone(),
			open_request: None,
			table_state: std::cell::Cell::new(TableState::default()),
			items: ItemBatch::default(),
			count_total: 0,
			key_config: env.key_config.clone(),
			current_width: std::cell::Cell::new(0),
			current_height: std::cell::Cell::new(0),
			options: env.options.clone(),
		}
	}

	fn components_mut(&mut self) -> Vec<&mut dyn Component> {
		vec![&mut self.diff]
	}

	///
	pub fn open(&mut self, open_request: FileRevOpen) -> Result<()> {
		self.open_request = Some(open_request.clone());

		let filter = diff_contains_file(open_request.file_path);
		self.git_log = Some(AsyncLog::new(
			self.repo_path.borrow().clone(),
			&self.sender,
			Some(filter),
		));

		self.items.clear();
		self.set_selection(open_request.selection.unwrap_or(0));

		self.show()?;

		self.diff.focus(false);
		self.diff.clear(false);

		self.update()?;

		Ok(())
	}

	///
	pub fn any_work_pending(&self) -> bool {
		self.git_diff.is_pending()
			|| self.git_log.as_ref().is_some_and(AsyncLog::is_pending)
	}

	///
	pub fn update(&mut self) -> Result<()> {
		if let Some(ref mut git_log) = self.git_log {
			git_log.fetch()?;

			self.fetch_commits_if_needed()?;
			self.update_diff()?;
		}

		Ok(())
	}

	///
	pub fn update_git(
		&mut self,
		event: AsyncGitNotification,
	) -> Result<()> {
		if self.visible {
			match event {
				AsyncGitNotification::CommitFiles
				| AsyncGitNotification::Log => self.update()?,
				AsyncGitNotification::Diff => self.update_diff()?,
				_ => (),
			}
		}

		Ok(())
	}

	pub fn update_diff(&mut self) -> Result<()> {
		if self.is_visible() {
			if let Some(commit_id) = self.selected_commit() {
				if let Some(open_request) = &self.open_request {
					let diff_params = DiffParams {
						path: open_request.file_path.clone(),
						diff_type: DiffType::Commit(commit_id),
						options: self.options.borrow().diff_options(),
					};

					if let Some((params, last)) =
						self.git_diff.last()?
					{
						if params == diff_params {
							self.diff.update(
								open_request.file_path.to_string(),
								false,
								last,
							);

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

	fn fetch_commits(
		&mut self,
		new_offset: usize,
		new_max_offset: usize,
	) -> Result<()> {
		if let Some(git_log) = &mut self.git_log {
			let amount = new_max_offset
				.saturating_sub(new_offset)
				.max(SLICE_SIZE);

			let commits = get_commits_info(
				&self.repo_path.borrow(),
				&git_log.get_slice(new_offset, amount)?,
				self.current_width.get(),
			);

			if let Ok(commits) = commits {
				self.items.set_items(new_offset, commits, None);
			}

			self.count_total = git_log.count()?;
		}

		Ok(())
	}

	fn selected_commit(&self) -> Option<CommitId> {
		let table_state = self.table_state.take();

		let commit_id = table_state.selected().and_then(|selected| {
			self.items
				.iter()
				.nth(
					selected
						.saturating_sub(self.items.index_offset()),
				)
				.as_ref()
				.map(|entry| entry.id)
		});

		self.table_state.set(table_state);

		commit_id
	}

	fn can_focus_diff(&self) -> bool {
		self.selected_commit().is_some()
	}

	fn get_title(&self) -> String {
		let selected = {
			let table = self.table_state.take();
			let res = table.selected().unwrap_or_default();
			self.table_state.set(table);
			res
		};
		let revisions = self.get_max_selection();

		self.open_request.as_ref().map_or(
			"<no history available>".into(),
			|open_request| {
				strings::file_log_title(
					&open_request.file_path,
					selected,
					revisions,
				)
			},
		)
	}

	fn get_rows(&self, now: DateTime<Local>) -> Vec<Row> {
		self.items
			.iter()
			.map(|entry| {
				let spans = Line::from(vec![
					Span::styled(
						entry.hash_short.to_string(),
						self.theme.commit_hash(false),
					),
					Span::raw(" "),
					Span::styled(
						entry.time_to_string(now),
						self.theme.commit_time(false),
					),
					Span::raw(" "),
					Span::styled(
						entry.author.to_string(),
						self.theme.commit_author(false),
					),
				]);

				let mut text = Text::from(spans);
				text.extend(Text::raw(entry.msg.to_string()));

				let cells = vec![Cell::from(""), Cell::from(text)];

				Row::new(cells).height(2)
			})
			.collect()
	}

	fn get_max_selection(&self) -> usize {
		self.git_log.as_ref().map_or(0, |log| {
			log.count().unwrap_or(0).saturating_sub(1)
		})
	}

	fn move_selection(
		&mut self,
		scroll_type: ScrollType,
	) -> Result<()> {
		let old_selection =
			self.table_state.get_mut().selected().unwrap_or(0);
		let max_selection = self.get_max_selection();
		let height_in_items = self.current_height.get() / 2;

		let new_selection = match scroll_type {
			ScrollType::Up => old_selection.saturating_sub(1),
			ScrollType::Down => {
				old_selection.saturating_add(1).min(max_selection)
			}
			ScrollType::Home => 0,
			ScrollType::End => max_selection,
			ScrollType::PageUp => old_selection
				.saturating_sub(height_in_items.saturating_sub(2)),
			ScrollType::PageDown => old_selection
				.saturating_add(height_in_items.saturating_sub(2))
				.min(max_selection),
		};

		let needs_update = new_selection != old_selection;

		if needs_update {
			self.queue.push(InternalEvent::Update(NeedsUpdate::DIFF));
		}

		self.set_selection(new_selection);
		self.fetch_commits_if_needed()?;

		Ok(())
	}

	fn set_selection(&mut self, selection: usize) {
		let height_in_items =
			(self.current_height.get().saturating_sub(2)) / 2;

		let offset = *self.table_state.get_mut().offset_mut();
		let min_offset = selection
			.saturating_sub(height_in_items.saturating_sub(1));

		let new_offset = offset.clamp(min_offset, selection);

		*self.table_state.get_mut().offset_mut() = new_offset;
		self.table_state.get_mut().select(Some(selection));
	}

	fn fetch_commits_if_needed(&mut self) -> Result<()> {
		let selection =
			self.table_state.get_mut().selected().unwrap_or(0);
		let offset = *self.table_state.get_mut().offset_mut();
		let height_in_items =
			(self.current_height.get().saturating_sub(2)) / 2;
		let new_max_offset =
			selection.saturating_add(height_in_items);

		if self.items.needs_data(offset, new_max_offset) {
			self.fetch_commits(offset, new_max_offset)?;
		}

		Ok(())
	}

	fn get_selection(&self) -> Option<usize> {
		let table_state = self.table_state.take();
		let selection = table_state.selected();
		self.table_state.set(table_state);

		selection
	}

	fn draw_revlog(&self, f: &mut Frame, area: Rect) {
		let constraints = [
			// type of change: (A)dded, (M)odified, (D)eleted
			Constraint::Length(1),
			// commit details
			Constraint::Percentage(100),
		];

		let now = Local::now();

		let title = self.get_title();
		let rows = self.get_rows(now);

		let table = Table::new(rows, constraints)
			.column_spacing(1)
			.row_highlight_style(self.theme.text(true, true))
			.block(
				Block::default()
					.borders(Borders::ALL)
					.title(Span::styled(
						title,
						self.theme.title(true),
					))
					.border_style(self.theme.block(true)),
			);

		let table_state = self.table_state.take();
		// We have to adjust the table state for drawing to account for the fact
		// that `self.items` not necessarily starts at index 0.
		//
		// When a user scrolls down, items outside of the current view are removed
		// when new data is fetched. Let’s have a look at an example: if the item at
		// index 50 is the first item in the current view and `self.items` has been
		// freshly fetched, the current offset is 50 and `self.items[0]` is the item
		// at index 50. Subtracting the current offset from the selected index
		// yields the correct index in `self.items`, in this case 0.
		let mut adjusted_table_state = TableState::default()
			.with_selected(table_state.selected().map(|selected| {
				selected.saturating_sub(self.items.index_offset())
			}))
			.with_offset(
				table_state
					.offset()
					.saturating_sub(self.items.index_offset()),
			);

		f.render_widget(Clear, area);
		f.render_stateful_widget(
			table,
			area,
			&mut adjusted_table_state,
		);

		draw_scrollbar(
			f,
			area,
			&self.theme,
			self.count_total,
			table_state.selected().unwrap_or(0),
			Orientation::Vertical,
		);

		self.table_state.set(table_state);
		self.current_width.set(area.width.into());
		self.current_height.set(area.height.into());
	}

	fn hide_stacked(&mut self, stack: bool) {
		self.hide();

		if stack {
			if let Some(open_request) = self.open_request.clone() {
				self.queue.push(InternalEvent::PopupStackPush(
					StackablePopupOpen::FileRevlog(FileRevOpen {
						file_path: open_request.file_path,
						selection: self.get_selection(),
					}),
				));
			}
		} else {
			self.queue.push(InternalEvent::PopupStackPop);
		}
	}
}

impl DrawableComponent for FileRevlogPopup {
	fn draw(&self, f: &mut Frame, area: Rect) -> Result<()> {
		if self.visible {
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
				.split(area);

			f.render_widget(Clear, area);

			self.draw_revlog(f, chunks[0]);
			self.diff.draw(f, chunks[1])?;
		}

		Ok(())
	}
}

impl Component for FileRevlogPopup {
	fn event(&mut self, event: &Event) -> Result<EventState> {
		if self.is_visible() {
			if event_pump(
				event,
				self.components_mut().as_mut_slice(),
			)?
			.is_consumed()
			{
				return Ok(EventState::Consumed);
			}

			if let Event::Key(key) = event {
				if key_match(key, self.key_config.keys.exit_popup) {
					if self.diff.focused() {
						self.diff.focus(false);
					} else {
						self.hide_stacked(false);
					}
				} else if key_match(
					key,
					self.key_config.keys.move_right,
				) && self.can_focus_diff()
				{
					self.diff.focus(true);
				} else if key_match(key, self.key_config.keys.enter) {
					if let Some(commit_id) = self.selected_commit() {
						self.hide_stacked(true);
						self.queue.push(InternalEvent::OpenPopup(
							StackablePopupOpen::InspectCommit(
								InspectCommitOpen::new(commit_id),
							),
						));
					}
				} else if key_match(key, self.key_config.keys.blame) {
					if let Some(open_request) =
						self.open_request.clone()
					{
						self.hide_stacked(true);
						self.queue.push(InternalEvent::OpenPopup(
							StackablePopupOpen::BlameFile(
								BlameFileOpen {
									file_path: open_request.file_path,
									commit_id: self.selected_commit(),
									selection: None,
								},
							),
						));
					}
				} else if key_match(key, self.key_config.keys.move_up)
				{
					self.move_selection(ScrollType::Up)?;
				} else if key_match(
					key,
					self.key_config.keys.move_down,
				) {
					self.move_selection(ScrollType::Down)?;
				} else if key_match(
					key,
					self.key_config.keys.shift_up,
				) || key_match(
					key,
					self.key_config.keys.home,
				) {
					self.move_selection(ScrollType::Home)?;
				} else if key_match(
					key,
					self.key_config.keys.shift_down,
				) || key_match(
					key,
					self.key_config.keys.end,
				) {
					self.move_selection(ScrollType::End)?;
				} else if key_match(key, self.key_config.keys.page_up)
				{
					self.move_selection(ScrollType::PageUp)?;
				} else if key_match(
					key,
					self.key_config.keys.page_down,
				) {
					self.move_selection(ScrollType::PageDown)?;
				}
			}

			return Ok(EventState::Consumed);
		}

		Ok(EventState::NotConsumed)
	}

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
			out.push(
				CommandInfo::new(
					strings::commands::log_details_toggle(
						&self.key_config,
					),
					true,
					self.selected_commit().is_some(),
				)
				.order(1),
			);
			out.push(
				CommandInfo::new(
					strings::commands::blame_file(&self.key_config),
					true,
					self.selected_commit().is_some(),
				)
				.order(1),
			);

			out.push(CommandInfo::new(
				strings::commands::diff_focus_right(&self.key_config),
				self.can_focus_diff(),
				!self.diff.focused(),
			));
			out.push(CommandInfo::new(
				strings::commands::diff_focus_left(&self.key_config),
				true,
				self.diff.focused(),
			));
		}

		visibility_blocking(self)
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
