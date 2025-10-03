use crate::components::{
	visibility_blocking, CommandBlocking, CommandInfo, Component,
	DrawableComponent, EventState, InputType, TextInputComponent,
};
use crate::{
	app::Environment,
	keys::{key_match, SharedKeyConfig},
	queue::{InternalEvent, Queue},
	strings::{self, POPUP_COMMIT_SHA_INVALID},
	ui::{self, style::SharedTheme},
};
use anyhow::Result;
use asyncgit::sync::{
	CommitId, LogFilterSearchOptions, RepoPathRef, SearchFields,
	SearchOptions,
};
use crossterm::event::Event;
use easy_cast::Cast;
use ratatui::{
	layout::{
		Alignment, Constraint, Direction, Layout, Margin, Rect,
	},
	text::{Line, Span},
	widgets::{Block, Borders, Clear, Paragraph},
	Frame,
};

enum Selection {
	EnterText,
	FuzzyOption,
	CaseOption,
	SummarySearch,
	MessageBodySearch,
	FilenameSearch,
	AuthorsSearch,
}

enum PopupMode {
	Search,
	JumpCommitSha,
}

pub struct LogSearchPopupPopup {
	repo: RepoPathRef,
	queue: Queue,
	visible: bool,
	mode: PopupMode,
	selection: Selection,
	key_config: SharedKeyConfig,
	find_text: TextInputComponent,
	options: (SearchFields, SearchOptions),
	theme: SharedTheme,
	jump_commit_id: Option<CommitId>,
}

impl LogSearchPopupPopup {
	///
	pub fn new(env: &Environment) -> Self {
		let mut find_text =
			TextInputComponent::new(env, "", "search text", false)
				.with_input_type(InputType::Singleline);
		find_text.embed();
		find_text.enabled(true);

		Self {
			repo: env.repo.clone(),
			queue: env.queue.clone(),
			visible: false,
			mode: PopupMode::Search,
			key_config: env.key_config.clone(),
			options: (
				SearchFields::default(),
				SearchOptions::default(),
			),
			theme: env.theme.clone(),
			find_text,
			selection: Selection::EnterText,
			jump_commit_id: None,
		}
	}

	pub fn open(&mut self) -> Result<()> {
		self.show()?;
		self.selection = Selection::EnterText;
		self.find_text.show()?;
		self.find_text.set_text(String::new());
		self.find_text.enabled(true);

		self.set_mode(&PopupMode::Search);

		Ok(())
	}

	fn set_mode(&mut self, mode: &PopupMode) {
		self.find_text.set_text(String::new());

		match mode {
			PopupMode::Search => {
				self.mode = PopupMode::Search;
				self.find_text.set_default_msg("search text".into());
				self.find_text.enabled(matches!(
					self.selection,
					Selection::EnterText
				));
			}
			PopupMode::JumpCommitSha => {
				self.mode = PopupMode::JumpCommitSha;
				self.jump_commit_id = None;
				self.find_text.set_default_msg("commit sha".into());
				self.find_text.enabled(false);
				self.selection = Selection::EnterText;
			}
		}
	}

	fn execute_confirm(&mut self) {
		self.hide();

		if !self.is_valid() {
			return;
		}

		match self.mode {
			PopupMode::Search => {
				self.queue.push(InternalEvent::CommitSearch(
					LogFilterSearchOptions {
						fields: self.options.0,
						options: self.options.1,
						search_pattern: self
							.find_text
							.get_text()
							.to_string(),
					},
				));
			}
			PopupMode::JumpCommitSha => {
				let commit_id = self.jump_commit_id
                    .expect("Commit id must have value here because it's already validated");
				self.queue.push(InternalEvent::SelectCommitInRevlog(
					commit_id,
				));
			}
		}
	}

	fn is_valid(&self) -> bool {
		match self.mode {
			PopupMode::Search => {
				!self.find_text.get_text().trim().is_empty()
			}
			PopupMode::JumpCommitSha => self.jump_commit_id.is_some(),
		}
	}

	fn validate_commit_sha(&mut self) {
		let path = self.repo.borrow();
		if let Ok(commit_id) = CommitId::from_revision(
			&path,
			self.find_text.get_text().trim(),
		) {
			self.jump_commit_id = Some(commit_id);
		} else {
			self.jump_commit_id = None;
		}
	}

	fn get_text_options(&self) -> Vec<Line<'_>> {
		let x_summary =
			if self.options.0.contains(SearchFields::MESSAGE_SUMMARY)
			{
				"X"
			} else {
				" "
			};

		let x_body =
			if self.options.0.contains(SearchFields::MESSAGE_BODY) {
				"X"
			} else {
				" "
			};

		let x_files =
			if self.options.0.contains(SearchFields::FILENAMES) {
				"X"
			} else {
				" "
			};

		let x_authors =
			if self.options.0.contains(SearchFields::AUTHORS) {
				"X"
			} else {
				" "
			};

		let x_opt_fuzzy =
			if self.options.1.contains(SearchOptions::FUZZY_SEARCH) {
				"X"
			} else {
				" "
			};

		let x_opt_casesensitive =
			if self.options.1.contains(SearchOptions::CASE_SENSITIVE)
			{
				"X"
			} else {
				" "
			};

		vec![
			Line::from(vec![Span::styled(
				format!("[{x_opt_fuzzy}] fuzzy search"),
				self.theme.text(
					matches!(self.selection, Selection::FuzzyOption),
					false,
				),
			)]),
			Line::from(vec![Span::styled(
				format!("[{x_opt_casesensitive}] case sensitive"),
				self.theme.text(
					matches!(self.selection, Selection::CaseOption),
					false,
				),
			)]),
			Line::from(vec![Span::styled(
				format!("[{x_summary}] summary",),
				self.theme.text(
					matches!(
						self.selection,
						Selection::SummarySearch
					),
					false,
				),
			)]),
			Line::from(vec![Span::styled(
				format!("[{x_body}] message body",),
				self.theme.text(
					matches!(
						self.selection,
						Selection::MessageBodySearch
					),
					false,
				),
			)]),
			Line::from(vec![Span::styled(
				format!("[{x_files}] committed files",),
				self.theme.text(
					matches!(
						self.selection,
						Selection::FilenameSearch
					),
					false,
				),
			)]),
			Line::from(vec![Span::styled(
				format!("[{x_authors}] authors",),
				self.theme.text(
					matches!(
						self.selection,
						Selection::AuthorsSearch
					),
					false,
				),
			)]),
		]
	}

	const fn option_selected(&self) -> bool {
		!matches!(self.selection, Selection::EnterText)
	}

	fn toggle_option(&mut self) {
		match self.selection {
			Selection::EnterText => (),
			Selection::FuzzyOption => {
				self.options.1.toggle(SearchOptions::FUZZY_SEARCH);
			}
			Selection::CaseOption => {
				self.options.1.toggle(SearchOptions::CASE_SENSITIVE);
			}
			Selection::SummarySearch => {
				self.options.0.toggle(SearchFields::MESSAGE_SUMMARY);

				if self.options.0.is_empty() {
					self.options
						.0
						.set(SearchFields::MESSAGE_BODY, true);
				}
			}
			Selection::MessageBodySearch => {
				self.options.0.toggle(SearchFields::MESSAGE_BODY);

				if self.options.0.is_empty() {
					self.options.0.set(SearchFields::FILENAMES, true);
				}
			}
			Selection::FilenameSearch => {
				self.options.0.toggle(SearchFields::FILENAMES);

				if self.options.0.is_empty() {
					self.options.0.set(SearchFields::AUTHORS, true);
				}
			}
			Selection::AuthorsSearch => {
				self.options.0.toggle(SearchFields::AUTHORS);

				if self.options.0.is_empty() {
					self.options
						.0
						.set(SearchFields::MESSAGE_SUMMARY, true);
				}
			}
		}
	}

	fn move_selection(&mut self, arg: bool) {
		if arg {
			//up
			self.selection = match self.selection {
				Selection::EnterText => Selection::AuthorsSearch,
				Selection::FuzzyOption => Selection::EnterText,
				Selection::CaseOption => Selection::FuzzyOption,
				Selection::SummarySearch => Selection::CaseOption,
				Selection::MessageBodySearch => {
					Selection::SummarySearch
				}
				Selection::FilenameSearch => {
					Selection::MessageBodySearch
				}
				Selection::AuthorsSearch => Selection::FilenameSearch,
			};
		} else {
			self.selection = match self.selection {
				Selection::EnterText => Selection::FuzzyOption,
				Selection::FuzzyOption => Selection::CaseOption,
				Selection::CaseOption => Selection::SummarySearch,
				Selection::SummarySearch => {
					Selection::MessageBodySearch
				}
				Selection::MessageBodySearch => {
					Selection::FilenameSearch
				}
				Selection::FilenameSearch => Selection::AuthorsSearch,
				Selection::AuthorsSearch => Selection::EnterText,
			};
		}

		self.find_text
			.enabled(matches!(self.selection, Selection::EnterText));
	}

	fn draw_search_mode(
		&self,
		f: &mut Frame,
		area: Rect,
	) -> Result<()> {
		const SIZE: (u16, u16) = (60, 10);
		let area = ui::centered_rect_absolute(SIZE.0, SIZE.1, area);

		f.render_widget(Clear, area);
		f.render_widget(
			Block::default()
				.borders(Borders::all())
				.style(self.theme.title(true))
				.title(Span::styled(
					strings::POPUP_TITLE_LOG_SEARCH,
					self.theme.title(true),
				)),
			area,
		);

		let chunks = Layout::default()
			.direction(Direction::Vertical)
			.constraints(
				[Constraint::Length(1), Constraint::Percentage(100)]
					.as_ref(),
			)
			.split(area.inner(Margin {
				horizontal: 1,
				vertical: 1,
			}));

		self.find_text.draw(f, chunks[0])?;

		f.render_widget(
			Paragraph::new(self.get_text_options())
				.block(
					Block::default()
						.borders(Borders::TOP)
						.border_style(self.theme.block(true)),
				)
				.alignment(Alignment::Left),
			chunks[1],
		);

		Ok(())
	}

	fn draw_commit_sha_mode(
		&self,
		f: &mut Frame,
		area: Rect,
	) -> Result<()> {
		const SIZE: (u16, u16) = (60, 3);
		let area = ui::centered_rect_absolute(SIZE.0, SIZE.1, area);

		let mut block_style = self.theme.title(true);

		let show_invalid = !self.is_valid()
			&& !self.find_text.get_text().trim().is_empty();

		if show_invalid {
			block_style = block_style.patch(self.theme.text_danger());
		}

		f.render_widget(Clear, area);
		f.render_widget(
			Block::default()
				.borders(Borders::all())
				.style(block_style)
				.title(Span::styled(
					strings::POPUP_TITLE_LOG_SEARCH,
					self.theme.title(true),
				)),
			area,
		);

		let chunks = Layout::default()
			.direction(Direction::Vertical)
			.constraints([Constraint::Length(1)].as_ref())
			.split(area.inner(Margin {
				horizontal: 1,
				vertical: 1,
			}));

		self.find_text.draw(f, chunks[0])?;

		if show_invalid {
			self.draw_invalid_sha(f);
		}

		Ok(())
	}

	fn draw_invalid_sha(&self, f: &mut Frame) {
		let msg_length: u16 = POPUP_COMMIT_SHA_INVALID.len().cast();
		let w = Paragraph::new(POPUP_COMMIT_SHA_INVALID)
			.style(self.theme.text_danger());

		let rect = {
			let mut rect = self.find_text.get_area();
			rect.y += rect.height;
			rect.height = 1;
			let offset = rect.width.saturating_sub(msg_length);
			rect.width = rect.width.saturating_sub(offset);
			rect.x += offset;

			rect
		};

		f.render_widget(w, rect);
	}

	#[inline]
	fn event_search_mode(
		&mut self,
		event: &crossterm::event::Event,
	) -> Result<EventState> {
		if let Event::Key(key) = &event {
			if key_match(key, self.key_config.keys.exit_popup) {
				self.hide();
			} else if key_match(key, self.key_config.keys.enter)
				&& self.is_valid()
			{
				self.execute_confirm();
			} else if key_match(key, self.key_config.keys.popup_up) {
				self.move_selection(true);
			} else if key_match(
				key,
				self.key_config.keys.find_commit_sha,
			) {
				self.set_mode(&PopupMode::JumpCommitSha);
			} else if key_match(key, self.key_config.keys.popup_down)
			{
				self.move_selection(false);
			} else if key_match(
				key,
				self.key_config.keys.log_mark_commit,
			) && self.option_selected()
			{
				self.toggle_option();
			} else if !self.option_selected() {
				self.find_text.event(event)?;
			}
		}

		Ok(EventState::Consumed)
	}

	#[inline]
	fn event_commit_sha_mode(
		&mut self,
		event: &crossterm::event::Event,
	) -> Result<EventState> {
		if let Event::Key(key) = &event {
			if key_match(key, self.key_config.keys.exit_popup) {
				self.set_mode(&PopupMode::Search);
			} else if key_match(key, self.key_config.keys.enter)
				&& self.is_valid()
			{
				self.execute_confirm();
			} else if self.find_text.event(event)?.is_consumed() {
				self.validate_commit_sha();
				self.find_text.enabled(
					!self.find_text.get_text().trim().is_empty(),
				);
			}
		}

		Ok(EventState::Consumed)
	}
}

impl DrawableComponent for LogSearchPopupPopup {
	fn draw(&self, f: &mut Frame, area: Rect) -> Result<()> {
		if self.is_visible() {
			match self.mode {
				PopupMode::Search => {
					self.draw_search_mode(f, area)?;
				}
				PopupMode::JumpCommitSha => {
					self.draw_commit_sha_mode(f, area)?;
				}
			}
		}

		Ok(())
	}
}

impl Component for LogSearchPopupPopup {
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

			if matches!(self.mode, PopupMode::Search) {
				out.push(
					CommandInfo::new(
						strings::commands::scroll_popup(
							&self.key_config,
						),
						true,
						true,
					)
					.order(1),
				);
				out.push(
					CommandInfo::new(
						strings::commands::toggle_option(
							&self.key_config,
						),
						self.option_selected(),
						true,
					)
					.order(1),
				);
				out.push(
					CommandInfo::new(
						strings::commands::find_commit_sha(
							&self.key_config,
						),
						true,
						true,
					)
					.order(1),
				);
			}

			out.push(CommandInfo::new(
				strings::commands::confirm_action(&self.key_config),
				self.is_valid(),
				self.visible,
			));
		}

		visibility_blocking(self)
	}

	fn event(
		&mut self,
		event: &crossterm::event::Event,
	) -> Result<EventState> {
		if !self.is_visible() {
			return Ok(EventState::NotConsumed);
		}

		match self.mode {
			PopupMode::Search => self.event_search_mode(event),
			PopupMode::JumpCommitSha => {
				self.event_commit_sha_mode(event)
			}
		}
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
