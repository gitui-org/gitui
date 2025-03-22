use crate::{
	app::Environment,
	components::{
		visibility_blocking, CommandBlocking, CommandInfo, Component,
		CredComponent, DrawableComponent, EventState,
	},
	keys::{key_match, SharedKeyConfig},
	queue::{InternalEvent, Queue},
	strings,
	ui::{self, style::SharedTheme},
};
use anyhow::Result;
use asyncgit::{
	sync::{
		cred::{
			extract_username_password, need_username_password,
			BasicAuthCredential,
		},
		get_default_remote, AsyncProgress, PushTagsProgress,
		RepoPathRef,
	},
	AsyncGitNotification, AsyncPushTags, PushTagsRequest,
};
use crossterm::event::Event;
use ratatui::{
	layout::Rect,
	text::Span,
	widgets::{Block, BorderType, Borders, Clear, Gauge},
	Frame,
};

///
pub struct PushTagsPopup {
	repo: RepoPathRef,
	visible: bool,
	git_push: AsyncPushTags,
	progress: Option<PushTagsProgress>,
	pending: bool,
	queue: Queue,
	theme: SharedTheme,
	key_config: SharedKeyConfig,
	input_cred: CredComponent,
}

impl PushTagsPopup {
	///
	pub fn new(env: &Environment) -> Self {
		Self {
			repo: env.repo.clone(),
			queue: env.queue.clone(),
			pending: false,
			visible: false,
			git_push: AsyncPushTags::new(
				env.repo.borrow().clone(),
				&env.sender_git,
			),
			progress: None,
			input_cred: CredComponent::new(env),
			theme: env.theme.clone(),
			key_config: env.key_config.clone(),
		}
	}

	///
	pub fn push_tags(&mut self) -> Result<()> {
		self.show()?;
		if need_username_password(&self.repo.borrow())? {
			let cred = extract_username_password(&self.repo.borrow())
				.unwrap_or_else(|_| {
					BasicAuthCredential::new(None, None)
				});
			if cred.is_complete() {
				self.push_to_remote(Some(cred))
			} else {
				self.input_cred.set_cred(cred);
				self.input_cred.show()
			}
		} else {
			self.push_to_remote(None)
		}
	}

	fn push_to_remote(
		&mut self,
		cred: Option<BasicAuthCredential>,
	) -> Result<()> {
		self.pending = true;
		self.progress = None;
		self.git_push.request(PushTagsRequest {
			remote: get_default_remote(&self.repo.borrow())?,
			basic_credential: cred,
		})?;
		Ok(())
	}

	///
	pub fn update_git(
		&mut self,
		ev: AsyncGitNotification,
	) -> Result<()> {
		if self.is_visible() && ev == AsyncGitNotification::PushTags {
			self.update()?;
		}

		Ok(())
	}

	///
	fn update(&mut self) -> Result<()> {
		self.pending = self.git_push.is_pending()?;
		self.progress = self.git_push.progress()?;

		if !self.pending {
			if let Some(err) = self.git_push.last_result()? {
				self.queue.push(InternalEvent::ShowErrorMsg(
					format!("push tags failed:\n{err}"),
				));
			}
			self.hide();
		}

		Ok(())
	}

	///
	pub const fn any_work_pending(&self) -> bool {
		self.pending
	}

	///
	pub fn get_progress(
		progress: Option<&PushTagsProgress>,
	) -> (String, u8) {
		progress.as_ref().map_or(
			(strings::PUSH_POPUP_PROGRESS_NONE.into(), 0),
			|progress| {
				(
					Self::progress_state_name(progress),
					progress.progress().progress,
				)
			},
		)
	}

	fn progress_state_name(progress: &PushTagsProgress) -> String {
		match progress {
			PushTagsProgress::CheckRemote => {
				strings::PUSH_TAGS_STATES_FETCHING
			}
			PushTagsProgress::Push { .. } => {
				strings::PUSH_TAGS_STATES_PUSHING
			}
			PushTagsProgress::Done => strings::PUSH_TAGS_STATES_DONE,
		}
		.to_string()
	}
}

impl DrawableComponent for PushTagsPopup {
	fn draw(&self, f: &mut Frame, rect: Rect) -> Result<()> {
		if self.visible {
			let (state, progress) =
				Self::get_progress(self.progress.as_ref());

			let area = ui::centered_rect_absolute(30, 3, f.area());

			f.render_widget(Clear, area);
			f.render_widget(
				Gauge::default()
					.label(state.as_str())
					.block(
						Block::default()
							.title(Span::styled(
								strings::PUSH_TAGS_POPUP_MSG,
								self.theme.title(true),
							))
							.borders(Borders::ALL)
							.border_type(BorderType::Thick)
							.border_style(self.theme.block(true)),
					)
					.gauge_style(self.theme.push_gauge())
					.percent(u16::from(progress)),
				area,
			);
			self.input_cred.draw(f, rect)?;
		}

		Ok(())
	}
}

impl Component for PushTagsPopup {
	fn commands(
		&self,
		out: &mut Vec<CommandInfo>,
		force_all: bool,
	) -> CommandBlocking {
		if self.is_visible() || force_all {
			if !force_all {
				out.clear();
			}

			if self.input_cred.is_visible() {
				return self.input_cred.commands(out, force_all);
			}

			out.push(CommandInfo::new(
				strings::commands::close_msg(&self.key_config),
				!self.pending,
				self.visible,
			));
		}
		visibility_blocking(self)
	}

	fn event(&mut self, ev: &Event) -> Result<EventState> {
		if self.visible {
			if let Event::Key(e) = ev {
				if self.input_cred.is_visible() {
					self.input_cred.event(ev)?;

					if self.input_cred.get_cred().is_complete()
						|| !self.input_cred.is_visible()
					{
						self.push_to_remote(Some(
							self.input_cred.get_cred().clone(),
						))?;
						self.input_cred.hide();
					}
				} else if key_match(
					e,
					self.key_config.keys.exit_popup,
				) && !self.pending
				{
					self.hide();
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
