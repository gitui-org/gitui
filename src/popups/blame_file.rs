use crate::{
	app::Environment,
	components::{
		string_width_align, time_to_string, visibility_blocking,
		CommandBlocking, CommandInfo, Component, DrawableComponent,
		EventState, ScrollType,
	},
	keys::{key_match, SharedKeyConfig},
	popups::{FileRevOpen, InspectCommitOpen},
	queue::{InternalEvent, Queue, StackablePopupOpen},
	string_utils::tabs_to_spaces,
	strings,
	ui::{self, style::SharedTheme, AsyncSyntaxJob, SyntaxText},
	AsyncAppNotification, AsyncNotification, SyntaxHighlightProgress,
};
use anyhow::Result;
use asyncgit::{
	asyncjob::AsyncSingleJob,
	sync::{BlameHunk, CommitId, FileBlame, RepoPathRef},
	AsyncBlame, AsyncGitNotification, BlameParams,
};
use crossbeam_channel::Sender;
use crossterm::event::Event;
use ratatui::{
	layout::{Constraint, Rect},
	symbols::line::VERTICAL,
	text::{Span, Text},
	widgets::{Block, Borders, Cell, Clear, Row, Table, TableState},
	Frame,
};
use std::path::Path;

static NO_COMMIT_ID: &str = "0000000";
static NO_AUTHOR: &str = "<no author>";
static MIN_AUTHOR_WIDTH: usize = 3;
static MAX_AUTHOR_WIDTH: usize = 20;

struct SyntaxFileBlame {
	pub file_blame: FileBlame,
	pub styled_text: Option<SyntaxText>,
}

impl SyntaxFileBlame {
	#[expect(
		clippy::missing_const_for_fn,
		reason = "as of 1.86.0 clippy wants this to be const even though that breaks"
	)]
	fn path(&self) -> &str {
		&self.file_blame.path
	}

	const fn commit_id(&self) -> &CommitId {
		&self.file_blame.commit_id
	}

	const fn lines(&self) -> &Vec<(Option<BlameHunk>, String)> {
		&self.file_blame.lines
	}
}

enum BlameProcess {
	GettingBlame(AsyncBlame),
	SyntaxHighlighting {
		unstyled_file_blame: SyntaxFileBlame,
		job: AsyncSingleJob<AsyncSyntaxJob>,
	},
	Result(SyntaxFileBlame),
}

impl BlameProcess {
	const fn result(&self) -> Option<&SyntaxFileBlame> {
		match self {
			Self::GettingBlame(_) => None,
			Self::SyntaxHighlighting {
				unstyled_file_blame,
				..
			} => Some(unstyled_file_blame),
			Self::Result(ref file_blame) => Some(file_blame),
		}
	}
}

#[derive(Clone, Debug)]
pub struct BlameFileOpen {
	pub file_path: String,
	pub commit_id: Option<CommitId>,
	pub selection: Option<usize>,
}

pub struct BlameFilePopup {
	title: String,
	theme: SharedTheme,
	queue: Queue,
	visible: bool,
	open_request: Option<BlameFileOpen>,
	params: Option<BlameParams>,
	table_state: std::cell::Cell<TableState>,
	key_config: SharedKeyConfig,
	current_height: std::cell::Cell<usize>,
	blame: Option<BlameProcess>,
	app_sender: Sender<AsyncAppNotification>,
	git_sender: Sender<AsyncGitNotification>,
	repo: RepoPathRef,
}

impl DrawableComponent for BlameFilePopup {
	fn draw(&self, f: &mut Frame, area: Rect) -> Result<()> {
		if self.is_visible() {
			let title = self.get_title();

			let rows = self.get_rows(area.width.into());
			let author_width = get_author_width(area.width.into());
			let constraints = [
				// commit id
				Constraint::Length(7),
				// commit date
				Constraint::Length(10),
				// commit author
				Constraint::Length(author_width.try_into()?),
				// line number and vertical bar
				Constraint::Length(
					(self.get_line_number_width().saturating_add(1))
						.try_into()?,
				),
				// the source code line
				Constraint::Percentage(100),
			];

			let number_of_rows: usize = rows.len();
			let syntax_highlight_progress = match self.blame {
				Some(BlameProcess::SyntaxHighlighting {
					ref job,
					..
				}) => job
					.progress()
					.map(|p| format!(" ({}%)", p.progress))
					.unwrap_or_default(),
				_ => String::new(),
			};
			let title_with_highlight_progress =
				format!("{title}{syntax_highlight_progress}");

			let table = Table::new(rows, constraints)
				.column_spacing(1)
				.row_highlight_style(self.theme.text(true, true))
				.block(
					Block::default()
						.borders(Borders::ALL)
						.title(Span::styled(
							title_with_highlight_progress,
							self.theme.title(true),
						))
						.border_style(self.theme.block(true)),
				);

			let mut table_state = self.table_state.take();

			f.render_widget(Clear, area);
			f.render_stateful_widget(table, area, &mut table_state);

			ui::draw_scrollbar(
				f,
				area,
				&self.theme,
				// April 2021: `draw_scrollbar` assumes that the last parameter
				// is `scroll_top`.  Therefore, it subtracts the area’s height
				// before calculating the position of the scrollbar. To account
				// for that, we add the current height.
				number_of_rows + (area.height as usize),
				// April 2021: we don’t have access to `table_state.offset`
				// (it’s private), so we use `table_state.selected()` as a
				// replacement.
				//
				// Other widgets, for example `BranchListComponent`, manage
				// scroll state themselves and use `self.scroll_top` in this
				// situation.
				//
				// There are plans to change `render_stateful_widgets`, so this
				// might be acceptable as an interim solution.
				//
				// https://github.com/fdehau/tui-rs/issues/448
				table_state.selected().unwrap_or(0),
				ui::Orientation::Vertical,
			);

			self.table_state.set(table_state);
			self.current_height.set(area.height.into());
		}

		Ok(())
	}
}

impl Component for BlameFilePopup {
	fn commands(
		&self,
		out: &mut Vec<CommandInfo>,
		force_all: bool,
	) -> CommandBlocking {
		let has_result = self
			.blame
			.as_ref()
			.is_some_and(|blame| blame.result().is_some());
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
					strings::commands::scroll(&self.key_config),
					true,
					has_result,
				)
				.order(1),
			);
			out.push(
				CommandInfo::new(
					strings::commands::commit_details_open(
						&self.key_config,
					),
					true,
					has_result,
				)
				.order(1),
			);
			out.push(
				CommandInfo::new(
					strings::commands::open_file_history(
						&self.key_config,
					),
					true,
					has_result,
				)
				.order(1),
			);
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
				} else if key_match(key, self.key_config.keys.move_up)
				{
					self.move_selection(ScrollType::Up);
				} else if key_match(
					key,
					self.key_config.keys.move_down,
				) {
					self.move_selection(ScrollType::Down);
				} else if key_match(
					key,
					self.key_config.keys.shift_up,
				) || key_match(
					key,
					self.key_config.keys.home,
				) {
					self.move_selection(ScrollType::Home);
				} else if key_match(
					key,
					self.key_config.keys.shift_down,
				) || key_match(
					key,
					self.key_config.keys.end,
				) {
					self.move_selection(ScrollType::End);
				} else if key_match(
					key,
					self.key_config.keys.page_down,
				) {
					self.move_selection(ScrollType::PageDown);
				} else if key_match(key, self.key_config.keys.page_up)
				{
					self.move_selection(ScrollType::PageUp);
				} else if key_match(
					key,
					self.key_config.keys.move_right,
				) {
					if let Some(commit_id) = self.selected_commit() {
						self.hide_stacked(true);
						self.queue.push(InternalEvent::OpenPopup(
							StackablePopupOpen::InspectCommit(
								InspectCommitOpen::new(commit_id),
							),
						));
					}
				} else if key_match(
					key,
					self.key_config.keys.file_history,
				) {
					if let Some(filepath) = self
						.params
						.as_ref()
						.map(|p| p.file_path.clone())
					{
						self.hide_stacked(true);
						self.queue.push(InternalEvent::OpenPopup(
							StackablePopupOpen::FileRevlog(
								FileRevOpen::new(filepath),
							),
						));
					}
				}

				return Ok(EventState::Consumed);
			}
		}

		Ok(EventState::NotConsumed)
	}

	fn is_visible(&self) -> bool {
		self.visible
	}

	fn show(&mut self) -> Result<()> {
		self.visible = true;

		Ok(())
	}
}

impl BlameFilePopup {
	///
	pub fn new(env: &Environment, title: &str) -> Self {
		Self {
			title: String::from(title),
			theme: env.theme.clone(),
			queue: env.queue.clone(),
			visible: false,
			params: None,
			open_request: None,
			table_state: std::cell::Cell::new(TableState::default()),
			key_config: env.key_config.clone(),
			current_height: std::cell::Cell::new(0),
			app_sender: env.sender_app.clone(),
			git_sender: env.sender_git.clone(),
			blame: None,
			repo: env.repo.clone(),
		}
	}

	fn hide_stacked(&mut self, stack: bool) {
		self.visible = false;
		if stack {
			if let Some(request) = self.open_request.clone() {
				self.queue.push(InternalEvent::PopupStackPush(
					StackablePopupOpen::BlameFile(BlameFileOpen {
						file_path: request.file_path,
						commit_id: request.commit_id,
						selection: self.get_selection(),
					}),
				));
			}
		} else {
			self.queue.push(InternalEvent::PopupStackPop);
		}
	}

	///
	pub fn open(&mut self, open: BlameFileOpen) -> Result<()> {
		self.open_request = Some(open.clone());
		self.params = Some(BlameParams {
			file_path: open.file_path,
			commit_id: open.commit_id,
		});
		self.blame =
			Some(BlameProcess::GettingBlame(AsyncBlame::new(
				self.repo.borrow().clone(),
				&self.git_sender,
			)));
		self.table_state.get_mut().select(Some(0));
		self.visible = true;
		self.update()?;

		Ok(())
	}

	///
	pub const fn any_work_pending(&self) -> bool {
		self.blame.is_some()
			&& !matches!(self.blame, Some(BlameProcess::Result(_)))
	}

	pub fn update_async(
		&mut self,
		ev: AsyncNotification,
	) -> Result<()> {
		if let AsyncNotification::Git(ev) = ev {
			return self.update_git(ev);
		}

		self.update_syntax(ev);
		Ok(())
	}

	fn update_git(
		&mut self,
		event: AsyncGitNotification,
	) -> Result<()> {
		if self.is_visible() && event == AsyncGitNotification::Blame {
			self.update()?;
		}

		Ok(())
	}

	fn update(&mut self) -> Result<()> {
		if self.is_visible() {
			if let Some(BlameProcess::GettingBlame(
				ref mut async_blame,
			)) = self.blame
			{
				if let Some(params) = &self.params {
					if let Some((
						previous_blame_params,
						last_file_blame,
					)) = async_blame.last()?
					{
						if previous_blame_params == *params {
							self.blame = Some(
								BlameProcess::SyntaxHighlighting {
									unstyled_file_blame:
										SyntaxFileBlame {
											file_blame:
												last_file_blame,
											styled_text: None,
										},
									job: AsyncSingleJob::new(
										self.app_sender.clone(),
									),
								},
							);
							self.set_open_selection();
							self.highlight_blame_lines();

							return Ok(());
						}
					}

					async_blame.request(params.clone())?;
				}
			}
		}

		Ok(())
	}

	fn update_syntax(&mut self, ev: AsyncNotification) {
		let Some(BlameProcess::SyntaxHighlighting {
			ref unstyled_file_blame,
			ref job,
		}) = self.blame
		else {
			return;
		};

		if let AsyncNotification::App(
			AsyncAppNotification::SyntaxHighlighting(progress),
		) = ev
		{
			match progress {
				SyntaxHighlightProgress::Done => {
					if let Some(job) = job.take_last() {
						if let Some(syntax) = job.result() {
							if syntax.path()
								== Path::new(
									unstyled_file_blame.path(),
								) {
								self.blame =
									Some(BlameProcess::Result(
										SyntaxFileBlame {
											file_blame:
												unstyled_file_blame
													.file_blame
													.clone(),
											styled_text: Some(syntax),
										},
									));
							}
						}
					}
				}
				SyntaxHighlightProgress::Progress => {}
			}
		}
	}

	///
	fn get_title(&self) -> String {
		match (
			self.any_work_pending(),
			self.params.as_ref(),
			self.blame.as_ref().and_then(|blame| blame.result()),
		) {
			(true, Some(params), _) => {
				format!(
					"{} -- {} -- <calculating.. (who is to blame?)>",
					self.title, params.file_path
				)
			}
			(false, Some(params), Some(file_blame)) => {
				format!(
					"{} -- {} -- {}",
					self.title,
					params.file_path,
					file_blame.commit_id().get_short_string()
				)
			}
			(false, Some(params), None) => {
				format!(
					"{} -- {} -- <no blame available>",
					self.title, params.file_path
				)
			}
			_ => format!("{} -- <no blame available>", self.title),
		}
	}

	///
	fn get_rows(&self, width: usize) -> Vec<Row> {
		self.blame
			.as_ref()
			.and_then(|blame| blame.result())
			.map(|file_blame| {
				let styled_text: Option<Text<'_>> = file_blame
					.styled_text
					.as_ref()
					.map(std::convert::Into::into);
				file_blame
					.lines()
					.iter()
					.enumerate()
					.map(|(i, (blame_hunk, line))| {
						self.get_line_blame(
							width,
							i,
							(blame_hunk.as_ref(), line.as_ref()),
							file_blame,
							styled_text.as_ref(),
						)
					})
					.collect()
			})
			.unwrap_or_default()
	}

	fn highlight_blame_lines(&mut self) {
		let Some(BlameProcess::SyntaxHighlighting {
			ref unstyled_file_blame,
			ref mut job,
		}) = self.blame
		else {
			return;
		};

		let Some(params) = &self.params else {
			return;
		};

		let raw_lines = unstyled_file_blame
			.lines()
			.iter()
			.map(|l| l.1.clone())
			.collect::<Vec<_>>();
		let mut text = tabs_to_spaces(raw_lines.join("\n"));
		text.push('\n');

		job.spawn(AsyncSyntaxJob::new(
			text,
			params.file_path.clone(),
			self.theme.get_syntax(),
		));
	}

	fn get_line_blame<'a>(
		&'a self,
		width: usize,
		line_number: usize,
		hunk_and_line: (Option<&BlameHunk>, &str),
		file_blame: &'a SyntaxFileBlame,
		styled_text: Option<&Text<'a>>,
	) -> Row<'a> {
		let (hunk_for_line, line) = hunk_and_line;

		let show_metadata = if line_number == 0 {
			true
		} else {
			let hunk_for_previous_line =
				&file_blame.lines()[line_number - 1];

			match (hunk_for_previous_line, hunk_for_line) {
				((Some(previous), _), Some(current)) => {
					previous.commit_id != current.commit_id
				}
				_ => true,
			}
		};

		let mut cells = if show_metadata {
			self.get_metadata_for_line_blame(width, hunk_for_line)
		} else {
			vec![Cell::from(""), Cell::from(""), Cell::from("")]
		};

		let line_number_width = self.get_line_number_width();

		let text_cell = styled_text.as_ref().map_or_else(
			|| {
				Cell::from(tabs_to_spaces(String::from(line)))
					.style(self.theme.text(true, false))
			},
			|styled_text| {
				let styled_text =
					styled_text.lines[line_number].clone();
				Cell::from(styled_text)
			},
		);

		cells.push(
			Cell::from(format!(
				"{line_number:>line_number_width$}{VERTICAL}",
			))
			.style(self.theme.text(true, false)),
		);
		cells.push(text_cell);

		Row::new(cells)
	}

	fn get_metadata_for_line_blame(
		&self,
		width: usize,
		blame_hunk: Option<&BlameHunk>,
	) -> Vec<Cell> {
		let commit_hash = blame_hunk.map_or_else(
			|| NO_COMMIT_ID.into(),
			|hunk| hunk.commit_id.get_short_string(),
		);
		let author_width = get_author_width(width);
		let truncated_author: String = blame_hunk.map_or_else(
			|| NO_AUTHOR.into(),
			|hunk| string_width_align(&hunk.author, author_width),
		);
		let author = format!("{truncated_author:MAX_AUTHOR_WIDTH$}");
		let time = blame_hunk.map_or_else(String::new, |hunk| {
			time_to_string(hunk.time, true)
		});

		let file_blame =
			self.blame.as_ref().and_then(|blame| blame.result());
		let is_blamed_commit = file_blame
			.and_then(|file_blame| {
				blame_hunk.map(|hunk| {
					file_blame.commit_id() == &hunk.commit_id
				})
			})
			.unwrap_or(false);

		vec![
			Cell::from(commit_hash).style(
				self.theme.commit_hash_in_blame(is_blamed_commit),
			),
			Cell::from(time).style(self.theme.commit_time(false)),
			Cell::from(author).style(self.theme.commit_author(false)),
		]
	}

	fn get_max_line_number(&self) -> usize {
		self.blame
			.as_ref()
			.and_then(|blame| blame.result())
			.map_or(0, |file_blame| file_blame.lines().len() - 1)
	}

	fn get_line_number_width(&self) -> usize {
		let max_line_number = self.get_max_line_number();

		number_of_digits(max_line_number)
	}

	fn move_selection(&self, scroll_type: ScrollType) -> bool {
		let mut table_state = self.table_state.take();

		let old_selection = table_state.selected().unwrap_or(0);
		let max_selection = self.get_max_line_number();

		let new_selection = match scroll_type {
			ScrollType::Up => old_selection.saturating_sub(1),
			ScrollType::Down => {
				old_selection.saturating_add(1).min(max_selection)
			}
			ScrollType::Home => 0,
			ScrollType::End => max_selection,
			ScrollType::PageUp => old_selection.saturating_sub(
				self.current_height.get().saturating_sub(2),
			),
			ScrollType::PageDown => old_selection
				.saturating_add(
					self.current_height.get().saturating_sub(2),
				)
				.min(max_selection),
		};

		let needs_update = new_selection != old_selection;

		table_state.select(Some(new_selection));
		self.table_state.set(table_state);

		needs_update
	}

	fn set_open_selection(&self) {
		if let Some(selection) =
			self.open_request.as_ref().and_then(|req| req.selection)
		{
			let mut table_state = self.table_state.take();
			table_state.select(Some(selection));
			self.table_state.set(table_state);
		}
	}

	fn get_selection(&self) -> Option<usize> {
		self.blame
			.as_ref()
			.and_then(|blame| blame.result())
			.and_then(|_| {
				let table_state = self.table_state.take();

				let selection = table_state.selected();

				self.table_state.set(table_state);

				selection
			})
	}

	fn selected_commit(&self) -> Option<CommitId> {
		self.blame
			.as_ref()
			.and_then(|blame| blame.result())
			.and_then(|file_blame| {
				let table_state = self.table_state.take();

				let commit_id =
					table_state.selected().and_then(|selected| {
						file_blame.lines()[selected]
							.0
							.as_ref()
							.map(|hunk| hunk.commit_id)
					});

				self.table_state.set(table_state);

				commit_id
			})
	}
}

fn get_author_width(width: usize) -> usize {
	(width.saturating_sub(19) / 3)
		.clamp(MIN_AUTHOR_WIDTH, MAX_AUTHOR_WIDTH)
}

const fn number_of_digits(number: usize) -> usize {
	let mut rest = number;
	let mut result = 0;

	while rest > 0 {
		rest /= 10;
		result += 1;
	}

	result
}
