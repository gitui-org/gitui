use crate::{
	app::Environment,
	components::{
		visibility_blocking, CommandBlocking, CommandInfo, Component,
		DrawableComponent, EventState, ScrollType, VerticalScroll,
	},
	keys::{key_match, SharedKeyConfig},
	queue::{InternalEvent, NeedsUpdate, Queue},
	strings, try_or_popup,
	ui::{self, Size},
};
use anyhow::Result;
use asyncgit::sync::{
	get_submodules, repo_dir, submodule_parent_info,
	update_submodule, RepoPathRef, SubmoduleInfo,
	SubmoduleParentInfo,
};
use crossterm::event::Event;
use ratatui::{
	layout::{
		Alignment, Constraint, Direction, Layout, Margin, Rect,
	},
	text::{Line, Span, Text},
	widgets::{Block, Borders, Clear, Paragraph},
	Frame,
};
use std::cell::Cell;
use ui::style::SharedTheme;
use unicode_truncate::UnicodeTruncateStr;

///
pub struct SubmodulesListPopup {
	repo: RepoPathRef,
	repo_path: String,
	queue: Queue,
	submodules: Vec<SubmoduleInfo>,
	submodule_parent: Option<SubmoduleParentInfo>,
	visible: bool,
	current_height: Cell<u16>,
	selection: u16,
	scroll: VerticalScroll,
	theme: SharedTheme,
	key_config: SharedKeyConfig,
}

impl DrawableComponent for SubmodulesListPopup {
	fn draw(&self, f: &mut Frame, rect: Rect) -> Result<()> {
		if self.is_visible() {
			const PERCENT_SIZE: Size = Size::new(80, 80);
			const MIN_SIZE: Size = Size::new(60, 30);

			let area = ui::centered_rect(
				PERCENT_SIZE.width,
				PERCENT_SIZE.height,
				rect,
			);
			let area = ui::rect_inside(MIN_SIZE, rect.into(), area);
			let area = area.intersection(rect);

			f.render_widget(Clear, area);

			f.render_widget(
				Block::default()
					.title(strings::POPUP_TITLE_SUBMODULES)
					.border_type(ratatui::widgets::BorderType::Thick)
					.borders(Borders::ALL),
				area,
			);

			let area = area.inner(Margin {
				vertical: 1,
				horizontal: 1,
			});

			let chunks_vertical = Layout::default()
				.direction(Direction::Vertical)
				.constraints(
					[Constraint::Min(1), Constraint::Length(5)]
						.as_ref(),
				)
				.split(area);

			let chunks = Layout::default()
				.direction(Direction::Horizontal)
				.constraints(
					[Constraint::Min(40), Constraint::Length(60)]
						.as_ref(),
				)
				.split(chunks_vertical[0]);

			self.draw_list(f, chunks[0])?;
			self.draw_info(f, chunks[1]);
			self.draw_local_info(f, chunks_vertical[1]);
		}

		Ok(())
	}
}

impl Component for SubmodulesListPopup {
	fn commands(
		&self,
		out: &mut Vec<CommandInfo>,
		force_all: bool,
	) -> CommandBlocking {
		if self.visible || force_all {
			if !force_all {
				out.clear();
			}

			out.push(CommandInfo::new(
				strings::commands::scroll(&self.key_config),
				true,
				true,
			));

			out.push(CommandInfo::new(
				strings::commands::close_popup(&self.key_config),
				true,
				true,
			));

			out.push(CommandInfo::new(
				strings::commands::open_submodule(&self.key_config),
				self.can_open_submodule(),
				true,
			));

			out.push(CommandInfo::new(
				strings::commands::update_submodule(&self.key_config),
				self.is_valid_selection(),
				true,
			));

			out.push(CommandInfo::new(
				strings::commands::open_submodule_parent(
					&self.key_config,
				),
				self.submodule_parent.is_some(),
				true,
			));
		}
		visibility_blocking(self)
	}

	fn event(&mut self, ev: &Event) -> Result<EventState> {
		if !self.visible {
			return Ok(EventState::NotConsumed);
		}

		if let Event::Key(e) = ev {
			if key_match(e, self.key_config.keys.exit_popup) {
				self.hide();
			} else if key_match(e, self.key_config.keys.move_down) {
				return self
					.move_selection(ScrollType::Up)
					.map(Into::into);
			} else if key_match(e, self.key_config.keys.move_up) {
				return self
					.move_selection(ScrollType::Down)
					.map(Into::into);
			} else if key_match(e, self.key_config.keys.page_down) {
				return self
					.move_selection(ScrollType::PageDown)
					.map(Into::into);
			} else if key_match(e, self.key_config.keys.page_up) {
				return self
					.move_selection(ScrollType::PageUp)
					.map(Into::into);
			} else if key_match(e, self.key_config.keys.home) {
				return self
					.move_selection(ScrollType::Home)
					.map(Into::into);
			} else if key_match(e, self.key_config.keys.end) {
				return self
					.move_selection(ScrollType::End)
					.map(Into::into);
			} else if key_match(e, self.key_config.keys.enter) {
				if let Some(submodule) = self.selected_entry() {
					if submodule.status.is_in_wd() {
						self.queue.push(InternalEvent::OpenRepo {
							path: submodule.path.clone(),
						});
					}
				}
			} else if key_match(
				e,
				self.key_config.keys.update_submodule,
			) {
				if let Some(submodule) = self.selected_entry() {
					try_or_popup!(
						self,
						"update submodule:",
						update_submodule(
							&self.repo.borrow(),
							&submodule.name,
						)
					);

					self.update_submodules()?;

					self.queue.push(InternalEvent::Update(
						NeedsUpdate::ALL,
					));
				}
			} else if key_match(
				e,
				self.key_config.keys.view_submodule_parent,
			) {
				if let Some(parent) = &self.submodule_parent {
					self.queue.push(InternalEvent::OpenRepo {
						path: parent.parent_gitpath.clone(),
					});
				}
			} else if key_match(
				e,
				self.key_config.keys.cmd_bar_toggle,
			) {
				//do not consume if its the more key
				return Ok(EventState::NotConsumed);
			}
		}

		Ok(EventState::Consumed)
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

impl SubmodulesListPopup {
	pub fn new(env: &Environment) -> Self {
		Self {
			submodules: Vec::new(),
			submodule_parent: None,
			scroll: VerticalScroll::new(),
			queue: env.queue.clone(),
			selection: 0,
			visible: false,
			theme: env.theme.clone(),
			key_config: env.key_config.clone(),
			current_height: Cell::new(0),
			repo: env.repo.clone(),
			repo_path: String::new(),
		}
	}

	///
	pub fn open(&mut self) -> Result<()> {
		self.show()?;
		self.update_submodules()?;

		Ok(())
	}

	///
	pub fn update_submodules(&mut self) -> Result<()> {
		if self.is_visible() {
			self.submodules = get_submodules(&self.repo.borrow())?;

			self.submodule_parent =
				submodule_parent_info(&self.repo.borrow())?;

			self.repo_path = repo_dir(&self.repo.borrow())
				.map(|e| e.to_string_lossy().to_string())
				.unwrap_or_default();

			self.set_selection(self.selection)?;
		}
		Ok(())
	}

	fn selected_entry(&self) -> Option<&SubmoduleInfo> {
		self.submodules.get(self.selection as usize)
	}

	fn is_valid_selection(&self) -> bool {
		self.selected_entry().is_some()
	}

	fn can_open_submodule(&self) -> bool {
		self.selected_entry().is_some_and(|s| s.status.is_in_wd())
	}

	//TODO: dedup this almost identical with BranchListComponent
	fn move_selection(&mut self, scroll: ScrollType) -> Result<bool> {
		let new_selection = match scroll {
			ScrollType::Up => self.selection.saturating_add(1),
			ScrollType::Down => self.selection.saturating_sub(1),
			ScrollType::PageDown => self
				.selection
				.saturating_add(self.current_height.get()),
			ScrollType::PageUp => self
				.selection
				.saturating_sub(self.current_height.get()),
			ScrollType::Home => 0,
			ScrollType::End => {
				let count: u16 = self.submodules.len().try_into()?;
				count.saturating_sub(1)
			}
		};

		self.set_selection(new_selection)?;

		Ok(true)
	}

	fn set_selection(&mut self, selection: u16) -> Result<()> {
		let num_entriess: u16 = self.submodules.len().try_into()?;
		let num_entries = num_entriess.saturating_sub(1);

		let selection = if selection > num_entries {
			num_entries
		} else {
			selection
		};

		self.selection = selection;

		Ok(())
	}

	fn get_text(
		&self,
		theme: &SharedTheme,
		width_available: u16,
		height: usize,
	) -> Text {
		const THREE_DOTS: &str = "...";
		const THREE_DOTS_LENGTH: usize = THREE_DOTS.len(); // "..."
		const COMMIT_HASH_LENGTH: usize = 8;

		let mut txt = Vec::with_capacity(3);

		let name_length: usize = (width_available as usize)
			.saturating_sub(COMMIT_HASH_LENGTH)
			.saturating_sub(THREE_DOTS_LENGTH);

		for (i, submodule) in self
			.submodules
			.iter()
			.skip(self.scroll.get_top())
			.take(height)
			.enumerate()
		{
			let mut module_path = submodule
				.path
				.as_os_str()
				.to_string_lossy()
				.to_string();

			if module_path.len() > name_length {
				module_path.unicode_truncate(
					name_length.saturating_sub(THREE_DOTS_LENGTH),
				);
				module_path += THREE_DOTS;
			}

			let selected = (self.selection as usize
				- self.scroll.get_top())
				== i;

			let span_hash = Span::styled(
				format!(
					"{} ",
					submodule
						.head_id
						.unwrap_or_default()
						.get_short_string()
				),
				theme.commit_hash(selected),
			);

			let span_name = Span::styled(
				format!("{module_path:name_length$} "),
				theme.text(true, selected),
			);

			txt.push(Line::from(vec![span_name, span_hash]));
		}

		Text::from(txt)
	}

	fn get_info_text(&self, theme: &SharedTheme) -> Text {
		self.selected_entry().map_or_else(
			Text::default,
			|submodule| {
				let span_title_path =
					Span::styled("Path:", theme.text(false, false));
				let span_path = Span::styled(
					submodule.path.to_string_lossy(),
					theme.text(true, false),
				);

				let span_title_commit =
					Span::styled("Commit:", theme.text(false, false));
				let span_commit = Span::styled(
					submodule.id.unwrap_or_default().to_string(),
					theme.commit_hash(false),
				);

				let span_title_url =
					Span::styled("Url:", theme.text(false, false));
				let span_url = Span::styled(
					submodule.url.clone().unwrap_or_default(),
					theme.text(true, false),
				);

				let span_title_status =
					Span::styled("Status:", theme.text(false, false));
				let span_status = Span::styled(
					format!("{:?}", submodule.status),
					theme.text(true, false),
				);

				Text::from(vec![
					Line::from(vec![span_title_path]),
					Line::from(vec![span_path]),
					Line::from(vec![]),
					Line::from(vec![span_title_commit]),
					Line::from(vec![span_commit]),
					Line::from(vec![]),
					Line::from(vec![span_title_url]),
					Line::from(vec![span_url]),
					Line::from(vec![]),
					Line::from(vec![span_title_status]),
					Line::from(vec![span_status]),
				])
			},
		)
	}

	fn get_local_info_text(&self, theme: &SharedTheme) -> Text {
		let mut spans = vec![
			Line::from(vec![Span::styled(
				"Current:",
				theme.text(false, false),
			)]),
			Line::from(vec![Span::styled(
				self.repo_path.clone(),
				theme.text(true, false),
			)]),
			Line::from(vec![Span::styled(
				"Parent:",
				theme.text(false, false),
			)]),
		];

		if let Some(parent_info) = &self.submodule_parent {
			spans.push(Line::from(vec![Span::styled(
				parent_info.parent_gitpath.to_string_lossy(),
				theme.text(true, false),
			)]));
		}

		Text::from(spans)
	}

	fn draw_list(&self, f: &mut Frame, r: Rect) -> Result<()> {
		let height_in_lines = r.height as usize;
		self.current_height.set(height_in_lines.try_into()?);

		self.scroll.update(
			self.selection as usize,
			self.submodules.len(),
			height_in_lines,
		);

		f.render_widget(
			Paragraph::new(self.get_text(
				&self.theme,
				r.width.saturating_add(1),
				height_in_lines,
			))
			.block(Block::default().borders(Borders::RIGHT))
			.alignment(Alignment::Left),
			r,
		);

		let mut r = r;
		r.height += 2;
		r.y = r.y.saturating_sub(1);

		self.scroll.draw(f, r, &self.theme);

		Ok(())
	}

	fn draw_info(&self, f: &mut Frame, r: Rect) {
		f.render_widget(
			Paragraph::new(self.get_info_text(&self.theme))
				.alignment(Alignment::Left),
			r,
		);
	}

	fn draw_local_info(&self, f: &mut Frame, r: Rect) {
		f.render_widget(
			Paragraph::new(self.get_local_info_text(&self.theme))
				.block(Block::default().borders(Borders::TOP))
				.alignment(Alignment::Left),
			r,
		);
	}
}
