use crate::{
	accessors,
	app::Environment,
	components::{
		command_pump, event_pump, visibility_blocking,
		CommandBlocking, CommandInfo, Component, DrawableComponent,
		EventState, StatusTreeComponent,
	},
	keys::{key_match, SharedKeyConfig},
	queue::{InternalEvent, Queue},
	strings,
	ui::style::SharedTheme,
};
use anyhow::Result;
use asyncgit::{
	sync::{self, status::StatusType, RepoPathRef},
	AsyncGitNotification, AsyncStatus, StatusParams,
};
use crossterm::event::Event;
use ratatui::{
	layout::{Alignment, Constraint, Direction, Layout},
	text::{Line, Span},
	widgets::{Block, Borders, Paragraph},
};
use std::borrow::Cow;

#[derive(Default, Clone, Copy, Debug)]
pub struct StashingOptions {
	pub stash_untracked: bool,
	pub keep_index: bool,
}

pub struct Stashing {
	repo: RepoPathRef,
	index: StatusTreeComponent,
	visible: bool,
	options: StashingOptions,
	theme: SharedTheme,
	git_status: AsyncStatus,
	queue: Queue,
	key_config: SharedKeyConfig,
}

impl Stashing {
	accessors!(self, [index]);

	///
	pub fn new(env: &Environment) -> Self {
		Self {
			repo: env.repo.clone(),
			index: StatusTreeComponent::new(
				env,
				&strings::stashing_files_title(&env.key_config),
				true,
			),
			visible: false,
			options: StashingOptions {
				keep_index: false,
				stash_untracked: true,
			},
			theme: env.theme.clone(),
			git_status: AsyncStatus::new(
				env.repo.borrow().clone(),
				env.sender_git.clone(),
			),
			queue: env.queue.clone(),
			key_config: env.key_config.clone(),
		}
	}

	///
	pub fn update(&self) -> Result<()> {
		if self.is_visible() {
			self.git_status
				//TODO: support options
				.fetch(&StatusParams::new(StatusType::Both, None))?;
		}

		Ok(())
	}

	///
	pub fn anything_pending(&self) -> bool {
		self.git_status.is_pending()
	}

	///
	pub fn update_git(
		&mut self,
		ev: AsyncGitNotification,
	) -> Result<()> {
		if self.is_visible() && ev == AsyncGitNotification::Status {
			let status = self.git_status.last()?;
			self.index.show()?;
			self.index.update(&status.items)?;
		}

		Ok(())
	}

	fn get_option_text(&self) -> Vec<Line> {
		let bracket_open = Span::raw(Cow::from("["));
		let bracket_close = Span::raw(Cow::from("]"));
		let option_on =
			Span::styled(Cow::from("x"), self.theme.option(true));

		let option_off =
			Span::styled(Cow::from("_"), self.theme.option(false));

		vec![
			Line::from(vec![
				bracket_open.clone(),
				if self.options.stash_untracked {
					option_on.clone()
				} else {
					option_off.clone()
				},
				bracket_close.clone(),
				Span::raw(Cow::from(" stash untracked")),
			]),
			Line::from(vec![
				bracket_open,
				if self.options.keep_index {
					option_on
				} else {
					option_off
				},
				bracket_close,
				Span::raw(Cow::from(" keep index")),
			]),
		]
	}
}

impl DrawableComponent for Stashing {
	fn draw(
		&self,
		f: &mut ratatui::Frame,
		rect: ratatui::layout::Rect,
	) -> Result<()> {
		let chunks = Layout::default()
			.direction(Direction::Horizontal)
			.constraints(
				[Constraint::Min(1), Constraint::Length(22)].as_ref(),
			)
			.split(rect);

		let right_chunks = Layout::default()
			.direction(Direction::Vertical)
			.constraints(
				[Constraint::Length(4), Constraint::Min(1)].as_ref(),
			)
			.split(chunks[1]);

		f.render_widget(
			Paragraph::new(self.get_option_text())
				.block(Block::default().borders(Borders::ALL).title(
					strings::stashing_options_title(&self.key_config),
				))
				.alignment(Alignment::Left),
			right_chunks[0],
		);

		self.index.draw(f, chunks[0])?;

		Ok(())
	}
}

impl Component for Stashing {
	fn commands(
		&self,
		out: &mut Vec<CommandInfo>,
		force_all: bool,
	) -> CommandBlocking {
		if self.visible || force_all {
			command_pump(
				out,
				force_all,
				self.components().as_slice(),
			);

			out.push(CommandInfo::new(
				strings::commands::stashing_save(&self.key_config),
				self.visible && !self.index.is_empty(),
				self.visible || force_all,
			));
			out.push(CommandInfo::new(
				strings::commands::stashing_toggle_indexed(
					&self.key_config,
				),
				self.visible,
				self.visible || force_all,
			));
			out.push(CommandInfo::new(
				strings::commands::stashing_toggle_untracked(
					&self.key_config,
				),
				self.visible,
				self.visible || force_all,
			));
		}

		visibility_blocking(self)
	}

	fn event(
		&mut self,
		ev: &crossterm::event::Event,
	) -> Result<EventState> {
		if self.visible {
			if event_pump(ev, self.components_mut().as_mut_slice())?
				.is_consumed()
			{
				return Ok(EventState::Consumed);
			}

			if let Event::Key(k) = ev {
				return if key_match(
					k,
					self.key_config.keys.stashing_save,
				) && !self.index.is_empty()
				{
					self.queue.push(InternalEvent::PopupStashing(
						self.options,
					));

					Ok(EventState::Consumed)
				} else if key_match(
					k,
					self.key_config.keys.stashing_toggle_index,
				) {
					self.options.keep_index =
						!self.options.keep_index;
					self.update()?;
					Ok(EventState::Consumed)
				} else if key_match(
					k,
					self.key_config.keys.stashing_toggle_untracked,
				) {
					self.options.stash_untracked =
						!self.options.stash_untracked;
					self.update()?;
					Ok(EventState::Consumed)
				} else {
					Ok(EventState::NotConsumed)
				};
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
		let config_untracked_files =
			sync::untracked_files_config(&self.repo.borrow())?;

		self.options.stash_untracked =
			!config_untracked_files.include_none();

		self.index.show()?;
		self.visible = true;
		self.update()?;
		Ok(())
	}
}
