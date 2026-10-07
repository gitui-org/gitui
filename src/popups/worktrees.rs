use crate::{
	app::Environment,
	components::{
		visibility_blocking, CommandBlocking, CommandInfo, Component,
		DrawableComponent, EventState, ScrollType, VerticalScroll,
	},
	keys::{key_match, SharedKeyConfig},
	queue::{Action, InternalEvent, Queue},
	strings,
	ui::{self, Size},
};
use anyhow::Result;
use asyncgit::sync::{
	get_worktrees, toggle_worktree_lock, RepoPathRef, WorktreeInfo,
};
use crossterm::event::Event;
use ratatui::{
	layout::{Alignment, Margin, Rect},
	text::{Line, Span, Text},
	widgets::{Block, Borders, Clear, Paragraph},
	Frame,
};
use std::cell::Cell;
use ui::style::SharedTheme;
use unicode_truncate::UnicodeTruncateStr;
use unicode_width::UnicodeWidthStr;

///
pub struct WorktreesPopup {
	repo: RepoPathRef,
	queue: Queue,
	worktrees: Vec<WorktreeInfo>,
	visible: bool,
	current_height: Cell<u16>,
	selection: u16,
	scroll: VerticalScroll,
	theme: SharedTheme,
	key_config: SharedKeyConfig,
}

impl DrawableComponent for WorktreesPopup {
	fn draw(&self, f: &mut Frame, rect: Rect) -> Result<()> {
		if self.is_visible() {
			const PERCENT_SIZE: Size = Size::new(80, 80);
			const MIN_SIZE: Size = Size::new(60, 20);

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
					.title(strings::POPUP_TITLE_WORKTREES)
					.border_type(ratatui::widgets::BorderType::Thick)
					.borders(Borders::ALL),
				area,
			);

			let area = area.inner(Margin {
				vertical: 1,
				horizontal: 1,
			});

			self.draw_list(f, area)?;
		}

		Ok(())
	}
}

impl Component for WorktreesPopup {
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
				strings::commands::open_worktree(&self.key_config),
				self.can_switch_worktree(),
				true,
			));

			out.push(CommandInfo::new(
				strings::commands::create_worktree_confirm_msg(
					&self.key_config,
				),
				true,
				true,
			));

			out.push(CommandInfo::new(
				strings::commands::remove_worktree(&self.key_config),
				self.can_remove_worktree(),
				true,
			));

			out.push(CommandInfo::new(
				strings::commands::lock_worktree(&self.key_config),
				self.can_lock_worktree(),
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
				if let Some(worktree) = self.selected_entry() {
					if !worktree.is_current {
						self.queue.push(InternalEvent::OpenRepo {
							path: worktree.path.clone(),
						});
					}
				}
				self.hide();
			} else if key_match(e, self.key_config.keys.create_branch)
			{
				self.queue.push(InternalEvent::CreateWorktree);
				self.hide();
			} else if key_match(e, self.key_config.keys.delete_branch)
			{
				if let Some(worktree) = self.selected_entry() {
					if !worktree.is_main && !worktree.is_current {
						self.queue.push(
							InternalEvent::ConfirmAction(
								Action::DeleteWorktree(
									worktree.name.clone(),
								),
							),
						);
					}
				}
			} else if key_match(e, self.key_config.keys.lock_worktree)
			{
				let name = self
					.selected_entry()
					.filter(|w| !w.is_main)
					.map(|w| w.name.clone());

				if let Some(name) = name {
					if let Err(err) = toggle_worktree_lock(
						&self.repo.borrow(),
						&name,
					) {
						self.queue.push(InternalEvent::ShowErrorMsg(
							err.to_string(),
						));
					}
					self.update_worktrees()?;
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

impl WorktreesPopup {
	pub fn new(env: &Environment) -> Self {
		Self {
			worktrees: Vec::new(),
			scroll: VerticalScroll::new(),
			queue: env.queue.clone(),
			selection: 0,
			visible: false,
			theme: env.theme.clone(),
			key_config: env.key_config.clone(),
			current_height: Cell::new(0),
			repo: env.repo.clone(),
		}
	}

	///
	pub fn open(&mut self) -> Result<()> {
		self.show()?;
		self.update_worktrees()?;

		Ok(())
	}

	///
	pub fn update_worktrees(&mut self) -> Result<()> {
		if self.is_visible() {
			self.worktrees = get_worktrees(&self.repo.borrow())?;
			self.set_selection(self.selection)?;
		}
		Ok(())
	}

	fn selected_entry(&self) -> Option<&WorktreeInfo> {
		self.worktrees.get(self.selection as usize)
	}

	fn can_switch_worktree(&self) -> bool {
		self.selected_entry().is_some_and(|w| !w.is_current)
	}

	fn can_remove_worktree(&self) -> bool {
		self.selected_entry()
			.is_some_and(|w| !w.is_main && !w.is_current)
	}

	fn can_lock_worktree(&self) -> bool {
		self.selected_entry().is_some_and(|w| !w.is_main)
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
				let count: u16 = self.worktrees.len().try_into()?;
				count.saturating_sub(1)
			}
		};

		self.set_selection(new_selection)?;

		Ok(true)
	}

	fn set_selection(&mut self, selection: u16) -> Result<()> {
		let num_entries: u16 = self.worktrees.len().try_into()?;
		let num_entries = num_entries.saturating_sub(1);

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
	) -> Text<'_> {
		const THREE_DOTS: &str = "...";
		const NAME_WIDTH: usize = 16;
		const BRANCH_WIDTH: usize = 20;

		let mut txt = Vec::with_capacity(self.worktrees.len());

		for (i, worktree) in self
			.worktrees
			.iter()
			.skip(self.scroll.get_top())
			.take(height)
			.enumerate()
		{
			let selected = (self.selection as usize
				- self.scroll.get_top())
				== i;

			let marker = if worktree.is_current { "*" } else { " " };

			let branch = worktree
				.branch
				.clone()
				.unwrap_or_else(|| "(detached)".to_string());

			let mut suffix = String::new();
			if worktree.is_locked {
				suffix.push_str(" 🔒");
			}
			if !worktree.is_valid {
				suffix.push_str(" (invalid)");
			}

			let prefix = format!(
				"{marker} {name:name_w$} {branch:branch_w$} ",
				name = worktree.name,
				name_w = NAME_WIDTH,
				branch_w = BRANCH_WIDTH,
			);

			let used = UnicodeWidthStr::width(prefix.as_str())
				.saturating_add(UnicodeWidthStr::width(
					suffix.as_str(),
				));

			let path_width =
				(width_available as usize).saturating_sub(used);

			let mut path =
				worktree.path.to_string_lossy().to_string();

			if UnicodeWidthStr::width(path.as_str()) > path_width {
				let (trunc, _) = path.unicode_truncate(
					path_width.saturating_sub(THREE_DOTS.len()),
				);
				path = format!("{trunc}{THREE_DOTS}");
			}

			txt.push(Line::from(vec![Span::styled(
				format!("{prefix}{path}{suffix}"),
				theme.text(true, selected),
			)]));
		}

		Text::from(txt)
	}

	fn draw_list(&self, f: &mut Frame, r: Rect) -> Result<()> {
		let height_in_lines = r.height as usize;
		self.current_height.set(height_in_lines.try_into()?);

		self.scroll.update(
			self.selection as usize,
			self.worktrees.len(),
			height_in_lines,
		);

		f.render_widget(
			Paragraph::new(self.get_text(
				&self.theme,
				r.width,
				height_in_lines,
			))
			.alignment(Alignment::Left),
			r,
		);

		let mut r = r;
		r.height += 2;
		r.y = r.y.saturating_sub(1);

		self.scroll.draw(f, r, &self.theme);

		Ok(())
	}
}
