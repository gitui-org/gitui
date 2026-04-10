use anyhow::Result;
use crossterm::event::Event;
use ratatui::{layout::Rect, Frame};

use asyncgit::sync::cred::BasicAuthCredential;

use crate::app::Environment;
use crate::components::{EventState, InputType, TextInputComponent};
use crate::keys::key_match;
use crate::{
	components::{
		visibility_blocking, CommandBlocking, CommandInfo, Component,
		DrawableComponent,
	},
	keys::SharedKeyConfig,
	strings,
};

///
pub struct CredComponent {
	visible: bool,
	key_config: SharedKeyConfig,
	input_username: TextInputComponent,
	input_password: TextInputComponent,
	cred: BasicAuthCredential,
}

impl CredComponent {
	///
	pub fn new(env: &Environment) -> Self {
		let key_config = env.key_config.clone();
		Self {
			visible: false,
			input_username: TextInputComponent::new(
				env,
				&strings::username_popup_title(&key_config),
				&strings::username_popup_msg(&key_config),
				false,
			)
			.with_input_type(InputType::Singleline),
			input_password: TextInputComponent::new(
				env,
				&strings::password_popup_title(&key_config),
				&strings::password_popup_msg(&key_config),
				false,
			)
			.with_input_type(InputType::Password),
			key_config,
			cred: BasicAuthCredential::new(None, None),
		}
	}

	pub fn set_cred(&mut self, cred: BasicAuthCredential) {
		self.cred = cred;
	}

	pub const fn get_cred(&self) -> &BasicAuthCredential {
		&self.cred
	}
}

impl DrawableComponent for CredComponent {
	fn draw(&self, f: &mut Frame, rect: Rect) -> Result<()> {
		if self.visible {
			self.input_username.draw(f, rect)?;
			self.input_password.draw(f, rect)?;
		}
		Ok(())
	}
}

impl Component for CredComponent {
	fn commands(
		&self,
		out: &mut Vec<CommandInfo>,
		force_all: bool,
	) -> CommandBlocking {
		if self.is_visible() || force_all {
			if !force_all {
				out.clear();
			}

			out.push(CommandInfo::new(
				strings::commands::validate_msg(&self.key_config),
				true,
				true,
			));
			out.push(CommandInfo::new(
				strings::commands::close_popup(&self.key_config),
				true,
				true,
			));
		}

		visibility_blocking(self)
	}

	fn event(&mut self, ev: &Event) -> Result<EventState> {
		if self.visible {
			if let Event::Key(e) = ev {
				if key_match(e, self.key_config.keys.exit_popup) {
					self.hide();
					return Ok(EventState::Consumed);
				}
				if self.input_username.event(ev)?.is_consumed()
					|| self.input_password.event(ev)?.is_consumed()
				{
					return Ok(EventState::Consumed);
				} else if key_match(e, self.key_config.keys.enter) {
					if self.input_username.is_visible() {
						self.cred = BasicAuthCredential::new(
							Some(
								self.input_username
									.get_text()
									.to_string(),
							),
							None,
						);
						self.input_username.hide();
						self.input_password.show()?;
					} else if self.input_password.is_visible() {
						self.cred = BasicAuthCredential::new(
							self.cred.username.clone(),
							Some(
								self.input_password
									.get_text()
									.to_string(),
							),
						);
						self.input_password.hide();
						self.input_password.clear();
						return Ok(EventState::NotConsumed);
					} else {
						self.hide();
					}
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
		self.cred = BasicAuthCredential::new(None, None);
		self.visible = false;
	}

	fn show(&mut self) -> Result<()> {
		self.visible = true;
		if self.cred.username.is_none() {
			self.input_username.show()
		} else if self.cred.password.is_none() {
			self.input_password.show()
		} else {
			Ok(())
		}
	}
}
