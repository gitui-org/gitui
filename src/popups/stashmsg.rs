use crate::components::{
	visibility_blocking, CommandBlocking, CommandInfo, Component,
	DrawableComponent, EventState, InputType, TextInputComponent,
};
use crate::{
	app::Environment,
	keys::{key_match, SharedKeyConfig},
	queue::{AppTabs, InternalEvent, Queue},
	strings,
	tabs::StashingOptions,
};
use anyhow::Result;
use asyncgit::sync::{self, RepoPathRef};
use crossterm::event::Event;
use ratatui::{layout::Rect, Frame};

pub struct StashMsgPopup {
	repo: RepoPathRef,
	options: StashingOptions,
	input: TextInputComponent,
	queue: Queue,
	key_config: SharedKeyConfig,
}

impl DrawableComponent for StashMsgPopup {
	fn draw(&self, f: &mut Frame, rect: Rect) -> Result<()> {
		self.input.draw(f, rect)?;

		Ok(())
	}
}

impl Component for StashMsgPopup {
	fn commands(
		&self,
		out: &mut Vec<CommandInfo>,
		force_all: bool,
	) -> CommandBlocking {
		if self.is_visible() || force_all {
			self.input.commands(out, force_all);

			out.push(CommandInfo::new(
				strings::commands::stashing_confirm_msg(
					&self.key_config,
				),
				true,
				true,
			));
		}

		visibility_blocking(self)
	}

	fn event(&mut self, ev: &Event) -> Result<EventState> {
		if self.is_visible() {
			if self.input.event(ev)?.is_consumed() {
				return Ok(EventState::Consumed);
			}

			if let Event::Key(e) = ev {
				if key_match(e, self.key_config.keys.enter) {
					let result = sync::stash_save(
						&self.repo.borrow(),
						if self.input.get_text().is_empty() {
							None
						} else {
							Some(self.input.get_text())
						},
						self.options.stash_untracked,
						self.options.keep_index,
					);
					match result {
						Ok(_) => {
							self.input.clear();
							self.hide();

							self.queue.push(
								InternalEvent::TabSwitch(
									AppTabs::Stashlist,
								),
							);
						}
						Err(e) => {
							self.hide();
							log::error!(
								"e: {} (options: {:?})",
								e,
								self.options
							);
							self.queue.push(
                                InternalEvent::ShowErrorMsg(format!(
                                    "stash error:\n{}\noptions:\n{:?}",
                                    e, self.options
                                )),
                            );
						}
					}
				}

				// stop key event propagation
				return Ok(EventState::Consumed);
			}
		}
		Ok(EventState::NotConsumed)
	}

	fn is_visible(&self) -> bool {
		self.input.is_visible()
	}

	fn hide(&mut self) {
		self.input.hide();
	}

	fn show(&mut self) -> Result<()> {
		self.input.show()?;

		Ok(())
	}
}

impl StashMsgPopup {
	///
	pub fn new(env: &Environment) -> Self {
		Self {
			options: StashingOptions::default(),
			queue: env.queue.clone(),
			input: TextInputComponent::new(
				env,
				&strings::stash_popup_title(&env.key_config),
				&strings::stash_popup_msg(&env.key_config),
				true,
			)
			.with_input_type(InputType::Singleline),
			key_config: env.key_config.clone(),
			repo: env.repo.clone(),
		}
	}

	///
	pub const fn options(&mut self, options: StashingOptions) {
		self.options = options;
	}
}
