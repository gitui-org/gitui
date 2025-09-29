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
			extract_username_password_for_push,
			need_username_password_for_push, BasicAuthCredential,
		},
		get_branch_remote,
		remotes::get_default_remote_for_push,
		RepoPathRef,
	},
	AsyncGitNotification, AsyncPush, PushRequest, PushType,
	RemoteProgress, RemoteProgressState,
};
use crossterm::event::Event;
use ratatui::{
	layout::Rect,
	text::Span,
	widgets::{Block, BorderType, Borders, Clear, Gauge},
	Frame,
};

///
#[derive(PartialEq, Eq)]
enum PushComponentModifier {
	None,
	Force,
	Delete,
	ForceDelete,
}

impl PushComponentModifier {
	pub(crate) fn force(&self) -> bool {
		self == &Self::Force || self == &Self::ForceDelete
	}
	pub(crate) fn delete(&self) -> bool {
		self == &Self::Delete || self == &Self::ForceDelete
	}
}

///
pub struct PushPopup {
	repo: RepoPathRef,
	modifier: PushComponentModifier,
	visible: bool,
	git_push: AsyncPush,
	progress: Option<RemoteProgress>,
	pending: bool,
	branch: String,
	push_type: PushType,
	queue: Queue,
	theme: SharedTheme,
	key_config: SharedKeyConfig,
	input_cred: CredComponent,
}

impl PushPopup {
	///
	pub fn new(env: &Environment) -> Self {
		Self {
			repo: env.repo.clone(),
			queue: env.queue.clone(),
			modifier: PushComponentModifier::None,
			pending: false,
			visible: false,
			branch: String::new(),
			push_type: PushType::Branch,
			git_push: AsyncPush::new(
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
	pub fn push(
		&mut self,
		branch: String,
		push_type: PushType,
		force: bool,
		delete: bool,
	) -> Result<()> {
		self.branch = branch;
		self.push_type = push_type;
		self.modifier = match (force, delete) {
			(true, true) => PushComponentModifier::ForceDelete,
			(false, true) => PushComponentModifier::Delete,
			(true, false) => PushComponentModifier::Force,
			(false, false) => PushComponentModifier::None,
		};

		self.show()?;

		if need_username_password_for_push(&self.repo.borrow())? {
			let cred = extract_username_password_for_push(
				&self.repo.borrow(),
			)
			.unwrap_or_else(|_| BasicAuthCredential::new(None, None));
			if cred.is_complete() {
				self.push_to_remote(Some(cred), force)
			} else {
				self.input_cred.set_cred(cred);
				self.input_cred.show()
			}
		} else {
			self.push_to_remote(None, force)
		}
	}

	fn push_to_remote(
		&mut self,
		cred: Option<BasicAuthCredential>,
		force: bool,
	) -> Result<()> {
		let remote = if let Ok(Some(remote)) =
			get_branch_remote(&self.repo.borrow(), &self.branch)
		{
			log::info!("push: branch '{}' has upstream for remote '{}' - using that",self.branch,remote);
			remote
		} else {
			log::info!("push: branch '{}' has no upstream - looking up default remote",self.branch);
			let remote =
				get_default_remote_for_push(&self.repo.borrow())?;
			log::info!(
				"push: branch '{}' to remote '{}'",
				self.branch,
				remote
			);
			remote
		};

		self.pending = true;
		self.progress = None;
		self.git_push.request(PushRequest {
			remote,
			branch: self.branch.clone(),
			push_type: self.push_type,
			force,
			delete: self.modifier.delete(),
			basic_credential: cred,
		})?;
		Ok(())
	}

	///
	pub fn update_git(
		&mut self,
		ev: AsyncGitNotification,
	) -> Result<()> {
		if self.is_visible() && ev == AsyncGitNotification::Push {
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
					format!("push failed:\n{err}"),
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
		progress: Option<&RemoteProgress>,
	) -> (String, u8) {
		progress.as_ref().map_or_else(
			|| (strings::PUSH_POPUP_PROGRESS_NONE.into(), 0),
			|progress| {
				(
					Self::progress_state_name(&progress.state),
					progress.get_progress_percent(),
				)
			},
		)
	}

	fn progress_state_name(state: &RemoteProgressState) -> String {
		match state {
			RemoteProgressState::PackingAddingObject => {
				strings::PUSH_POPUP_STATES_ADDING
			}
			RemoteProgressState::PackingDeltafiction => {
				strings::PUSH_POPUP_STATES_DELTAS
			}
			RemoteProgressState::Pushing => {
				strings::PUSH_POPUP_STATES_PUSHING
			}
			RemoteProgressState::Transfer => {
				strings::PUSH_POPUP_STATES_TRANSFER
			}
			RemoteProgressState::Done => {
				strings::PUSH_POPUP_STATES_DONE
			}
		}
		.into()
	}
}

impl DrawableComponent for PushPopup {
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
								if self.modifier.force() {
									strings::FORCE_PUSH_POPUP_MSG
								} else {
									strings::PUSH_POPUP_MSG
								},
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

impl Component for PushPopup {
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
						self.push_to_remote(
							Some(self.input_cred.get_cred().clone()),
							self.modifier.force(),
						)?;
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
