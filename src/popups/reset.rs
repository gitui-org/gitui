use crate::components::{
	visibility_blocking, CommandBlocking, CommandInfo, Component,
	DrawableComponent, EventState,
};
use crate::{
	app::Environment,
	keys::{key_match, SharedKeyConfig},
	queue::Queue,
	strings, try_or_popup,
	ui::{self, style::SharedTheme},
};
use anyhow::Result;
use asyncgit::{
	cached,
	sync::{CommitId, RepoPath, ResetType},
};
use crossterm::event::Event;
use ratatui::{
	layout::{Alignment, Rect},
	text::{Line, Span},
	widgets::{Block, Borders, Clear, Paragraph},
	Frame,
};

const fn type_to_string(
	kind: ResetType,
) -> (&'static str, &'static str) {
	const RESET_TYPE_DESC_SOFT: &str =
		"  🟢 Keep all changes. Stage differences";
	const RESET_TYPE_DESC_MIXED: &str =
		" 🟡 Keep all changes. Unstage differences";
	const RESET_TYPE_DESC_HARD: &str =
		"  🔴 Discard all local changes";

	match kind {
		ResetType::Soft => ("Soft", RESET_TYPE_DESC_SOFT),
		ResetType::Mixed => ("Mixed", RESET_TYPE_DESC_MIXED),
		ResetType::Hard => ("Hard", RESET_TYPE_DESC_HARD),
	}
}

pub struct ResetPopup {
	queue: Queue,
	repo: RepoPath,
	commit: Option<CommitId>,
	kind: ResetType,
	git_branch_name: cached::BranchName,
	visible: bool,
	key_config: SharedKeyConfig,
	theme: SharedTheme,
}

impl ResetPopup {
	///
	pub fn new(env: &Environment) -> Self {
		Self {
			queue: env.queue.clone(),
			repo: env.repo.borrow().clone(),
			commit: None,
			kind: ResetType::Soft,
			git_branch_name: cached::BranchName::new(
				env.repo.clone(),
			),
			visible: false,
			key_config: env.key_config.clone(),
			theme: env.theme.clone(),
		}
	}

	fn get_text(&self, _width: u16) -> Vec<Line<'_>> {
		let mut txt: Vec<Line> = Vec::with_capacity(10);

		txt.push(Line::from(vec![
			Span::styled(
				String::from("Branch: "),
				self.theme.text(true, false),
			),
			Span::styled(
				self.git_branch_name.last().unwrap_or_default(),
				self.theme.branch(false, true),
			),
		]));

		txt.push(Line::from(vec![
			Span::styled(
				String::from("Reset to: "),
				self.theme.text(true, false),
			),
			Span::styled(
				self.commit
					.map(|c| c.to_string())
					.unwrap_or_default(),
				self.theme.commit_hash(false),
			),
		]));

		let (kind_name, kind_desc) = type_to_string(self.kind);

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
	pub fn open(&mut self, id: CommitId) -> Result<()> {
		self.show()?;

		self.commit = Some(id);

		Ok(())
	}

	///
	#[allow(clippy::unnecessary_wraps)]
	pub fn update(&mut self) -> Result<()> {
		self.git_branch_name.lookup().ok();

		Ok(())
	}

	fn reset(&mut self) {
		if let Some(id) = self.commit {
			try_or_popup!(
				self,
				"reset:",
				asyncgit::sync::reset_repo(&self.repo, id, self.kind)
			);
		}

		self.hide();
	}

	fn change_kind(&mut self, incr: bool) {
		self.kind = if incr {
			match self.kind {
				ResetType::Soft => ResetType::Mixed,
				ResetType::Mixed => ResetType::Hard,
				ResetType::Hard => ResetType::Soft,
			}
		} else {
			match self.kind {
				ResetType::Soft => ResetType::Hard,
				ResetType::Mixed => ResetType::Soft,
				ResetType::Hard => ResetType::Mixed,
			}
		};
	}
}

impl DrawableComponent for ResetPopup {
	fn draw(&self, f: &mut Frame, area: Rect) -> Result<()> {
		if self.is_visible() {
			const SIZE: (u16, u16) = (55, 5);
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
								"Reset",
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

impl Component for ResetPopup {
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
					self.reset();
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
