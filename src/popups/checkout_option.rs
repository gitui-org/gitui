use crate::components::{
	visibility_blocking, CommandBlocking, CommandInfo, Component,
	DrawableComponent, EventState,
};
use crate::queue::{InternalEvent, NeedsUpdate};
use crate::strings::CheckoutOptions;
use crate::try_or_popup;
use crate::{
	app::Environment,
	keys::{key_match, SharedKeyConfig},
	queue::Queue,
	strings,
	ui::{self, style::SharedTheme},
};
use anyhow::{Ok, Result};
use asyncgit::sync::branch::checkout_remote_branch;
use asyncgit::sync::status::discard_status;
use asyncgit::sync::{checkout_branch, BranchInfo, RepoPath};
use crossterm::event::Event;
use ratatui::{
	layout::{Alignment, Rect},
	text::{Line, Span},
	widgets::{Block, Borders, Clear, Paragraph},
	Frame,
};

pub struct CheckoutOptionPopup {
	queue: Queue,
	repo: RepoPath,
	branch: Option<BranchInfo>,
	option: CheckoutOptions,
	visible: bool,
	key_config: SharedKeyConfig,
	theme: SharedTheme,
}

impl CheckoutOptionPopup {
	///
	pub fn new(env: &Environment) -> Self {
		Self {
			queue: env.queue.clone(),
			repo: env.repo.borrow().clone(),
			branch: None,
			option: CheckoutOptions::KeepLocalChanges,
			visible: false,
			key_config: env.key_config.clone(),
			theme: env.theme.clone(),
		}
	}

	fn get_text(&self, _width: u16) -> Vec<Line<'_>> {
		let mut txt: Vec<Line> = Vec::with_capacity(10);

		txt.push(Line::from(vec![
			Span::styled(
				String::from("Switch to: "),
				self.theme.text(true, false),
			),
			Span::styled(
				self.branch.as_ref().expect("No branch").name.clone(),
				self.theme.commit_hash(false),
			),
		]));

		let (kind_name, kind_desc) = self.option.to_string_pair();

		txt.push(Line::from(vec![
			Span::styled(
				String::from("How: "),
				self.theme.text(true, false),
			),
			Span::styled(kind_name, self.theme.text(true, true)),
			Span::styled(kind_desc, self.theme.text(true, false)),
		]));

		txt
	}

	///
	pub fn open(&mut self, branch: BranchInfo) -> Result<()> {
		self.show()?;

		self.branch = Some(branch);

		Ok(())
	}

	fn checkout(&self) -> Result<()> {
		if let Some(branch) = &self.branch {
			if branch.is_local() {
				checkout_branch(&self.repo, &branch.name)?;
			} else {
				checkout_remote_branch(&self.repo, branch)?;
			}
		}

		Ok(())
	}

	fn handle_event(&mut self) -> Result<()> {
		match self.option {
			CheckoutOptions::KeepLocalChanges => {
				self.checkout()?;
			}
			CheckoutOptions::DiscardAllLocalChagnes => {
				discard_status(&self.repo)?;
				self.checkout()?;
			}
		}

		self.queue.push(InternalEvent::Update(NeedsUpdate::ALL));
		self.queue.push(InternalEvent::SelectBranch);
		self.hide();

		Ok(())
	}

	fn change_kind(&mut self, incr: bool) {
		self.option = if incr {
			self.option.next()
		} else {
			self.option.previous()
		};
	}
}

impl DrawableComponent for CheckoutOptionPopup {
	fn draw(&self, f: &mut Frame, area: Rect) -> Result<()> {
		if self.is_visible() {
			const SIZE: (u16, u16) = (55, 4);
			let area =
				ui::centered_rect_absolute(SIZE.0, SIZE.1, area);

			let width = area.width;

			f.render_widget(Clear, area);
			f.render_widget(
				Paragraph::new(self.get_text(width))
					.block(
						Block::default()
							.borders(Borders::ALL)
							.title(Span::styled(
								"Checkout options",
								self.theme.title(true),
							))
							.border_style(self.theme.block(true)),
					)
					.alignment(Alignment::Left),
				area,
			);
		}

		Ok(())
	}
}

impl Component for CheckoutOptionPopup {
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
					strings::commands::reset_commit(&self.key_config),
					true,
					true,
				)
				.order(1),
			);

			out.push(
				CommandInfo::new(
					strings::commands::reset_type(&self.key_config),
					true,
					true,
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
			if let Event::Key(key) = &event {
				if key_match(key, self.key_config.keys.exit_popup) {
					self.hide();
				} else if key_match(
					key,
					self.key_config.keys.move_down,
				) {
					self.change_kind(true);
				} else if key_match(key, self.key_config.keys.move_up)
				{
					self.change_kind(false);
				} else if key_match(key, self.key_config.keys.enter) {
					try_or_popup!(
						self,
						"checkout error:",
						self.handle_event()
					);
				}
			}

			return Ok(EventState::Consumed);
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
