use crate::components::{
	visibility_blocking, CommandBlocking, CommandInfo, Component,
	DrawableComponent, EventState, TextInputComponent,
};
use crate::{
	app::Environment,
	commit_helpers::{CommitHelpers, SharedCommitHelpers},
	keys::{key_match, SharedKeyConfig},
	options::SharedOptions,
	queue::{InternalEvent, NeedsUpdate, Queue},
	strings, try_or_popup,
	ui::style::SharedTheme,
};
use anyhow::{bail, Result};
use asyncgit::sync::commit::commit_message_prettify;
use asyncgit::{
	cached,
	sync::{
		self, get_config_string, CommitId, HookResult,
		PrepareCommitMsgSource, RepoPathRef, RepoState,
	},
	StatusItem, StatusItemType,
};
use crossterm::event::Event;
use easy_cast::Cast;
use ratatui::{
	layout::{Alignment, Rect},
	widgets::Paragraph,
	Frame,
};

use std::{
	fmt::Write as _,
	fs::{read_to_string, File},
	io::{Read, Write},
	path::PathBuf,
	str::FromStr,
	sync::mpsc::{self, Receiver},
	thread,
	time::Instant,
};

use super::ExternalEditorPopup;

enum CommitResult {
	CommitDone,
	Aborted,
}

enum Mode {
	Normal,
	Amend(CommitId),
	Merge(Vec<CommitId>),
	Revert,
	Reword(CommitId),
}

#[derive(Clone)]
enum HelperState {
	Idle,
	Selection {
		selected_index: usize,
	},
	Running {
		helper_name: String,
		frame_index: usize,
		start_time: std::time::Instant,
	},
	Success(String), // generated message
	Error(String),   // error message
}

pub struct CommitPopup {
	repo: RepoPathRef,
	input: TextInputComponent,
	mode: Mode,
	queue: Queue,
	key_config: SharedKeyConfig,
	git_branch_name: cached::BranchName,
	commit_template: Option<String>,
	theme: SharedTheme,
	commit_msg_history_idx: usize,
	options: SharedOptions,
	verify: bool,
	commit_helpers: SharedCommitHelpers,
	helper_state: HelperState,
	helper_receiver: Option<Receiver<Result<String, String>>>,
}

const FIRST_LINE_LIMIT: usize = 50;

// Spinner animation frames
const SPINNER_FRAMES: &[&str] =
	&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const SPINNER_INTERVAL_MS: u64 = 80;
const HELPER_TIMEOUT_SECS: u64 = 30;

impl CommitPopup {
	///
	pub fn new(env: &Environment) -> Self {
		Self {
			queue: env.queue.clone(),
			mode: Mode::Normal,
			input: TextInputComponent::new(
				env,
				"",
				&strings::commit_msg(&env.key_config),
				true,
			),
			key_config: env.key_config.clone(),
			git_branch_name: cached::BranchName::new(
				env.repo.clone(),
			),
			commit_template: None,
			theme: env.theme.clone(),
			repo: env.repo.clone(),
			commit_msg_history_idx: 0,
			options: env.options.clone(),
			verify: true,
			commit_helpers: std::sync::Arc::new(
				CommitHelpers::init().unwrap_or_default(),
			),
			helper_state: HelperState::Idle,
			helper_receiver: None,
		}
	}

	///
	pub fn update(&mut self) -> bool {
		self.git_branch_name.lookup().ok();
		self.check_helper_result();
		self.update_helper_animation()
	}

	fn check_helper_result(&mut self) {
		if let Some(receiver) = &self.helper_receiver {
			if let Ok(result) = receiver.try_recv() {
				match result {
					Ok(generated_msg) => {
						let current_msg = self.input.get_text();
						let new_msg =
							if current_msg.is_empty() {
								generated_msg
							} else {
								format!("{current_msg}\n\n{generated_msg}")
							};
						self.input.set_text(new_msg);
						self.helper_state = HelperState::Success(
							"Generated successfully".to_string(),
						);
					}
					Err(error_msg) => {
						self.helper_state =
							HelperState::Error(error_msg);
					}
				}
				self.helper_receiver = None;
			}
		}
	}

	pub fn clear_helper_message(&mut self) {
		if matches!(
			self.helper_state,
			HelperState::Success(_)
				| HelperState::Error(_)
				| HelperState::Selection { .. }
		) {
			self.helper_state = HelperState::Idle;
		}
	}

	pub fn cancel_helper(&mut self) {
		if matches!(self.helper_state, HelperState::Running { .. }) {
			self.helper_state =
				HelperState::Error("Cancelled by user".to_string());
			self.helper_receiver = None;
		}
	}

	pub fn update_helper_animation(&mut self) -> bool {
		if let HelperState::Running {
			frame_index,
			start_time,
			helper_name: _,
		} = &mut self.helper_state
		{
			let elapsed = start_time.elapsed();

			// Check timeout
			if elapsed.as_secs() > HELPER_TIMEOUT_SECS {
				self.helper_state = HelperState::Error(
					"Timeout: Helper took too long".to_string(),
				);
				return true;
			}

			// Update frame
			let new_frame = (elapsed.as_millis()
				/ u128::from(SPINNER_INTERVAL_MS))
				as usize % SPINNER_FRAMES.len();
			if *frame_index != new_frame {
				*frame_index = new_frame;
				return true; // Animation frame changed, needs redraw
			}
		}
		false // No update needed
	}

	fn draw_branch_name(&self, f: &mut Frame) {
		if let Some(name) = self.git_branch_name.last() {
			let w = Paragraph::new(format!("{{{name}}}"))
				.alignment(Alignment::Right);

			let rect = {
				let mut rect = self.input.get_area();
				rect.height = 1;
				rect.width = rect.width.saturating_sub(1);
				rect
			};

			f.render_widget(w, rect);
		}
	}

	fn draw_warnings(&self, f: &mut Frame) {
		let first_line = self
			.input
			.get_text()
			.lines()
			.next()
			.map(str::len)
			.unwrap_or_default();

		if first_line > FIRST_LINE_LIMIT {
			let msg = strings::commit_first_line_warning(first_line);
			let msg_length: u16 = msg.len().cast();
			let w =
				Paragraph::new(msg).style(self.theme.text_danger());

			let rect = {
				let mut rect = self.input.get_area();
				rect.y += rect.height.saturating_sub(1);
				rect.height = 1;
				let offset =
					rect.width.saturating_sub(msg_length + 1);
				rect.width = rect.width.saturating_sub(offset + 1);
				rect.x += offset;

				rect
			};

			f.render_widget(w, rect);
		}
	}

	fn draw_helper_status(&self, f: &mut Frame) {
		use ratatui::style::Style;
		use ratatui::widgets::{Paragraph, Wrap};

		let (msg, style) = match &self.helper_state {
			HelperState::Idle => return,
			HelperState::Selection { selected_index } => {
				let helpers = self.commit_helpers.get_helpers();
				helpers.get(*selected_index).map_or_else(
					|| (String::from("No helpers available"), self.theme.text_danger()),
					|helper| {
						let hotkey_hint = helper.hotkey
							.map(|h| format!(" [{h}]"))
							.unwrap_or_default();
						(format!("Select helper: {} ({}/{}){}. [↑↓] to navigate, [Enter] to run, [ESC] to cancel", 
							helper.name, selected_index + 1, helpers.len(), hotkey_hint),
						 Style::default().fg(ratatui::style::Color::Cyan))
					}
				)
			}
			HelperState::Running {
				helper_name,
				frame_index,
				start_time,
			} => {
				let spinner = SPINNER_FRAMES[*frame_index];
				let elapsed = start_time.elapsed().as_secs();
				(format!("{spinner} Generating with {helper_name}... ({elapsed}s) [ESC to cancel]"), 
				 Style::default().fg(ratatui::style::Color::Yellow))
			}
			HelperState::Success(msg) => (
				format!("✅ {msg}"),
				Style::default().fg(ratatui::style::Color::Green),
			),
			HelperState::Error(err) => {
				(format!("❌ {err}"), self.theme.text_danger())
			}
		};

		let msg_length: u16 =
			msg.chars().count().try_into().unwrap_or(0);
		let paragraph = Paragraph::new(msg)
			.style(style)
			.wrap(Wrap { trim: true });

		let rect = {
			let mut rect = self.input.get_area();
			rect.y = rect.y.saturating_add(rect.height);
			rect.height = 1;
			rect.width = msg_length.min(rect.width);
			rect
		};

		f.render_widget(paragraph, rect);
	}

	const fn item_status_char(
		item_type: StatusItemType,
	) -> &'static str {
		match item_type {
			StatusItemType::Modified => "modified",
			StatusItemType::New => "new file",
			StatusItemType::Deleted => "deleted",
			StatusItemType::Renamed => "renamed",
			StatusItemType::Typechange => " ",
			StatusItemType::Conflicted => "conflicted",
		}
	}

	pub fn show_editor(
		&mut self,
		changes: Vec<StatusItem>,
	) -> Result<()> {
		let file_path = sync::repo_dir(&self.repo.borrow())?
			.join("COMMIT_EDITMSG");

		{
			let mut file = File::create(&file_path)?;
			file.write_fmt(format_args!(
				"{}\n",
				self.input.get_text()
			))?;
			file.write_all(
				strings::commit_editor_msg(&self.key_config)
					.as_bytes(),
			)?;

			file.write_all(b"\n#\n# Changes to be committed:")?;

			for change in changes {
				let status_char =
					Self::item_status_char(change.status);
				let message =
					format!("\n#\t{status_char}: {}", change.path);
				file.write_all(message.as_bytes())?;
			}
		}

		ExternalEditorPopup::open_file_in_editor(
			&self.repo.borrow(),
			&file_path,
		)?;

		let mut message = String::new();

		let mut file = File::open(&file_path)?;
		file.read_to_string(&mut message)?;
		drop(file);
		std::fs::remove_file(&file_path)?;

		message =
			commit_message_prettify(&self.repo.borrow(), message)?;
		self.input.set_text(message);
		self.input.show()?;

		Ok(())
	}

	fn commit(&mut self) -> Result<()> {
		let msg = self.input.get_text().to_string();

		if matches!(
			self.commit_with_msg(msg)?,
			CommitResult::CommitDone
		) {
			self.options
				.borrow_mut()
				.add_commit_msg(self.input.get_text());
			self.commit_msg_history_idx = 0;

			self.hide();
			self.queue.push(InternalEvent::Update(NeedsUpdate::ALL));
			self.queue.push(InternalEvent::StatusLastFileMoved);
			self.input.clear();
		}

		Ok(())
	}

	fn commit_with_msg(
		&mut self,
		msg: String,
	) -> Result<CommitResult> {
		// on exit verify should always be on
		let verify = self.verify;
		self.verify = true;

		if verify {
			// run pre commit hook - can reject commit
			if let HookResult::NotOk(e) =
				sync::hooks_pre_commit(&self.repo.borrow())?
			{
				log::error!("pre-commit hook error: {}", e);
				self.queue.push(InternalEvent::ShowErrorMsg(
					format!("pre-commit hook error:\n{e}"),
				));
				return Ok(CommitResult::Aborted);
			}
		}

		let mut msg =
			commit_message_prettify(&self.repo.borrow(), msg)?;

		if verify {
			// run commit message check hook - can reject commit
			if let HookResult::NotOk(e) =
				sync::hooks_commit_msg(&self.repo.borrow(), &mut msg)?
			{
				log::error!("commit-msg hook error: {}", e);
				self.queue.push(InternalEvent::ShowErrorMsg(
					format!("commit-msg hook error:\n{e}"),
				));
				return Ok(CommitResult::Aborted);
			}
		}
		self.do_commit(&msg)?;

		if let HookResult::NotOk(e) =
			sync::hooks_post_commit(&self.repo.borrow())?
		{
			log::error!("post-commit hook error: {}", e);
			self.queue.push(InternalEvent::ShowErrorMsg(format!(
				"post-commit hook error:\n{e}"
			)));
		}

		Ok(CommitResult::CommitDone)
	}

	fn do_commit(&self, msg: &str) -> Result<()> {
		match &self.mode {
			Mode::Normal => sync::commit(&self.repo.borrow(), msg)?,
			Mode::Amend(amend) => {
				sync::amend(&self.repo.borrow(), *amend, msg)?
			}
			Mode::Merge(ids) => {
				sync::merge_commit(&self.repo.borrow(), msg, ids)?
			}
			Mode::Revert => {
				sync::commit_revert(&self.repo.borrow(), msg)?
			}
			Mode::Reword(id) => {
				let commit =
					sync::reword(&self.repo.borrow(), *id, msg)?;
				self.queue.push(InternalEvent::TabSwitchStatus);

				commit
			}
		};
		Ok(())
	}

	fn can_commit(&self) -> bool {
		!self.is_empty() && self.is_changed()
	}

	fn can_amend(&self) -> bool {
		matches!(self.mode, Mode::Normal)
			&& sync::get_head(&self.repo.borrow()).is_ok()
			&& (self.is_empty() || !self.is_changed())
	}

	fn is_empty(&self) -> bool {
		self.input.get_text().is_empty()
	}

	fn is_changed(&self) -> bool {
		Some(self.input.get_text().trim())
			!= self.commit_template.as_ref().map(|s| s.trim())
	}

	fn amend(&mut self) -> Result<()> {
		if self.can_amend() {
			let id = sync::get_head(&self.repo.borrow())?;
			self.mode = Mode::Amend(id);

			let details =
				sync::get_commit_details(&self.repo.borrow(), id)?;

			self.input.set_title(strings::commit_title_amend());

			if let Some(msg) = details.message {
				self.input.set_text(msg.combine());
			}
		}

		Ok(())
	}
	fn signoff_commit(&mut self) {
		let msg = self.input.get_text();
		let signed_msg = self.add_sign_off(msg);
		if let std::result::Result::Ok(signed_msg) = signed_msg {
			self.input.set_text(signed_msg);
		}
	}
	fn toggle_verify(&mut self) {
		self.verify = !self.verify;
	}

	fn run_commit_helper(&mut self) -> Result<()> {
		self.open_helper_selection()
	}

	fn open_helper_selection(&mut self) -> Result<()> {
		// Check if already running
		if matches!(self.helper_state, HelperState::Running { .. }) {
			return Ok(());
		}

		let helpers = self.commit_helpers.get_helpers();
		if helpers.is_empty() {
			let config_path = if cfg!(target_os = "macos") {
				"~/Library/Application Support/gitui/commit_helpers.ron"
			} else if cfg!(target_os = "windows") {
				"%APPDATA%/gitui/commit_helpers.ron"
			} else {
				"~/.config/gitui/commit_helpers.ron"
			};
			self.helper_state = HelperState::Error(
				format!("No commit helpers configured. Create {config_path} (see .example file)")
			);
			return Ok(());
		}

		if helpers.len() == 1 {
			// Only one helper, run it directly
			self.execute_helper_by_index(0)
		} else {
			// Multiple helpers, show selection UI
			self.helper_state =
				HelperState::Selection { selected_index: 0 };
			Ok(())
		}
	}

	fn execute_helper_by_index(
		&mut self,
		helper_index: usize,
	) -> Result<()> {
		let helpers = self.commit_helpers.get_helpers();
		if helper_index >= helpers.len() {
			self.helper_state = HelperState::Error(
				"Invalid helper index".to_string(),
			);
			return Ok(());
		}

		let helper = helpers[helper_index].clone();

		// Set running state with animation
		self.helper_state = HelperState::Running {
			helper_name: helper.name,
			frame_index: 0,
			start_time: Instant::now(),
		};

		// Create channel for communication
		let (tx, rx) = mpsc::channel();
		self.helper_receiver = Some(rx);

		// Clone helpers for thread
		let commit_helpers = self.commit_helpers.clone();

		// Execute helper in background thread
		thread::spawn(move || {
			let result = commit_helpers
				.execute_helper(helper_index)
				.map_err(|e| format!("Failed: {e}"));

			// Send result back (ignore if receiver is dropped)
			let _ = tx.send(result);
		});

		Ok(())
	}

	pub fn open(&mut self, reword: Option<CommitId>) -> Result<()> {
		//only clear text if it was not a normal commit dlg before, so to preserve old commit msg that was edited
		if !matches!(self.mode, Mode::Normal) {
			self.input.clear();
		}

		self.mode = Mode::Normal;

		let repo_state = sync::repo_state(&self.repo.borrow())?;

		let (mode, msg_source) = if repo_state != RepoState::Clean
			&& reword.is_some()
		{
			bail!("cannot reword while repo is not in a clean state");
		} else if let Some(reword_id) = reword {
			self.input.set_text(
				sync::get_commit_details(
					&self.repo.borrow(),
					reword_id,
				)?
				.message
				.unwrap_or_default()
				.combine(),
			);
			self.input.set_title(strings::commit_reword_title());
			(Mode::Reword(reword_id), PrepareCommitMsgSource::Message)
		} else {
			match repo_state {
				RepoState::Merge => {
					let ids =
						sync::mergehead_ids(&self.repo.borrow())?;
					self.input
						.set_title(strings::commit_title_merge());
					self.input.set_text(sync::merge_msg(
						&self.repo.borrow(),
					)?);
					(Mode::Merge(ids), PrepareCommitMsgSource::Merge)
				}
				RepoState::Revert => {
					self.input
						.set_title(strings::commit_title_revert());
					self.input.set_text(sync::merge_msg(
						&self.repo.borrow(),
					)?);
					(Mode::Revert, PrepareCommitMsgSource::Message)
				}

				_ => {
					self.commit_template = get_config_string(
						&self.repo.borrow(),
						"commit.template",
					)
					.map_err(|e| {
						log::error!("load git-config failed: {}", e);
						e
					})
					.ok()
					.flatten()
					.and_then(|path| {
						shellexpand::full(path.as_str())
							.ok()
							.and_then(|path| {
								PathBuf::from_str(path.as_ref()).ok()
							})
					})
					.and_then(|path| {
						read_to_string(&path)
							.map_err(|e| {
								log::error!("read commit.template failed: {e} (path: '{:?}')",path);
								e
							})
							.ok()
					});

					let msg_source = if self.is_empty() {
						if let Some(s) = &self.commit_template {
							self.input.set_text(s.clone());
							PrepareCommitMsgSource::Template
						} else {
							PrepareCommitMsgSource::Message
						}
					} else {
						PrepareCommitMsgSource::Message
					};
					self.input.set_title(strings::commit_title());

					(Mode::Normal, msg_source)
				}
			}
		};

		self.mode = mode;

		let mut msg = self.input.get_text().to_string();
		if let HookResult::NotOk(e) = sync::hooks_prepare_commit_msg(
			&self.repo.borrow(),
			msg_source,
			&mut msg,
		)? {
			log::error!("prepare-commit-msg hook rejection: {e}",);
		}
		self.input.set_text(msg);

		self.commit_msg_history_idx = 0;
		self.input.show()?;

		Ok(())
	}

	fn add_sign_off(&self, msg: &str) -> Result<String> {
		const CONFIG_KEY_USER_NAME: &str = "user.name";
		const CONFIG_KEY_USER_MAIL: &str = "user.email";

		let user = get_config_string(
			&self.repo.borrow(),
			CONFIG_KEY_USER_NAME,
		)?;

		let mail = get_config_string(
			&self.repo.borrow(),
			CONFIG_KEY_USER_MAIL,
		)?;

		let mut msg = msg.to_owned();
		if let (Some(user), Some(mail)) = (user, mail) {
			let _ = write!(msg, "\n\nSigned-off-by: {user} <{mail}>");
		}

		Ok(msg)
	}
}

impl DrawableComponent for CommitPopup {
	fn draw(&self, f: &mut Frame, rect: Rect) -> Result<()> {
		if self.is_visible() {
			self.input.draw(f, rect)?;
			self.draw_branch_name(f);
			self.draw_warnings(f);
			self.draw_helper_status(f);
		}

		Ok(())
	}
}

impl Component for CommitPopup {
	fn commands(
		&self,
		out: &mut Vec<CommandInfo>,
		force_all: bool,
	) -> CommandBlocking {
		self.input.commands(out, force_all);

		if self.is_visible() || force_all {
			out.push(CommandInfo::new(
				strings::commands::commit_submit(&self.key_config),
				self.can_commit(),
				true,
			));

			out.push(CommandInfo::new(
				strings::commands::toggle_verify(
					&self.key_config,
					self.verify,
				),
				self.can_commit(),
				true,
			));

			out.push(CommandInfo::new(
				strings::commands::commit_amend(&self.key_config),
				self.can_amend(),
				true,
			));

			out.push(CommandInfo::new(
				strings::commands::commit_signoff(&self.key_config),
				true,
				true,
			));

			out.push(CommandInfo::new(
				strings::commands::commit_open_editor(
					&self.key_config,
				),
				true,
				true,
			));

			out.push(CommandInfo::new(
				strings::commands::commit_next_msg_from_history(
					&self.key_config,
				),
				self.options.borrow().has_commit_msg_history(),
				true,
			));

			out.push(CommandInfo::new(
				strings::commands::newline(&self.key_config),
				true,
				true,
			));

			if !self.commit_helpers.get_helpers().is_empty() {
				out.push(CommandInfo::new(
					strings::commands::commit_helper(
						&self.key_config,
					),
					true,
					true,
				));
			}
		}

		visibility_blocking(self)
	}

	fn event(&mut self, ev: &Event) -> Result<EventState> {
		if self.is_visible() {
			// Handle helper selection navigation
			if let Event::Key(key) = ev {
				// Handle helper selection state
				if let HelperState::Selection { selected_index } =
					&self.helper_state
				{
					match key.code {
						crossterm::event::KeyCode::Esc => {
							self.helper_state = HelperState::Idle;
							return Ok(EventState::Consumed);
						}
						crossterm::event::KeyCode::Up => {
							let helpers_len = self
								.commit_helpers
								.get_helpers()
								.len();
							if helpers_len > 0 {
								let new_index =
									if *selected_index == 0 {
										helpers_len - 1
									} else {
										*selected_index - 1
									};
								self.helper_state =
									HelperState::Selection {
										selected_index: new_index,
									};
							}
							return Ok(EventState::Consumed);
						}
						crossterm::event::KeyCode::Down => {
							let helpers_len = self
								.commit_helpers
								.get_helpers()
								.len();
							if helpers_len > 0 {
								let new_index = (*selected_index + 1)
									% helpers_len;
								self.helper_state =
									HelperState::Selection {
										selected_index: new_index,
									};
							}
							return Ok(EventState::Consumed);
						}
						crossterm::event::KeyCode::Enter => {
							let selected = *selected_index;
							try_or_popup!(
								self,
								"helper execution error:",
								self.execute_helper_by_index(
									selected
								)
							);
							return Ok(EventState::Consumed);
						}
						crossterm::event::KeyCode::Char(c) => {
							// Check for hotkey match
							if let Some(index) =
								self.commit_helpers.find_by_hotkey(c)
							{
								try_or_popup!(
									self,
									"helper execution error:",
									self.execute_helper_by_index(
										index
									)
								);
								return Ok(EventState::Consumed);
							}
						}
						_ => {}
					}
				}

				// Handle ESC to cancel running helper
				if key.code == crossterm::event::KeyCode::Esc
					&& matches!(
						self.helper_state,
						HelperState::Running { .. }
					) {
					self.cancel_helper();
					return Ok(EventState::Consumed);
				}

				// Clear success/error messages on key press
				self.clear_helper_message();
			}

			if let Event::Key(e) = ev {
				let input_consumed =
					if key_match(e, self.key_config.keys.commit)
						&& self.can_commit()
					{
						try_or_popup!(
							self,
							"commit error:",
							self.commit()
						);
						true
					} else if key_match(
						e,
						self.key_config.keys.toggle_verify,
					) && self.can_commit()
					{
						self.toggle_verify();
						true
					} else if key_match(
						e,
						self.key_config.keys.commit_amend,
					) && self.can_amend()
					{
						self.amend()?;
						true
					} else if key_match(
						e,
						self.key_config.keys.open_commit_editor,
					) {
						self.queue.push(
							InternalEvent::OpenExternalEditor(None),
						);
						self.hide();
						true
					} else if key_match(
						e,
						self.key_config.keys.commit_history_next,
					) {
						if let Some(msg) = self
							.options
							.borrow()
							.commit_msg(self.commit_msg_history_idx)
						{
							self.input.set_text(msg);
							self.commit_msg_history_idx += 1;
						}
						true
					} else if key_match(
						e,
						self.key_config.keys.toggle_signoff,
					) {
						self.signoff_commit();
						true
					} else if key_match(
						e,
						self.key_config.keys.commit_helper,
					) {
						try_or_popup!(
							self,
							"commit helper error:",
							self.run_commit_helper()
						);
						true
					} else {
						false
					};

				if !input_consumed {
					self.input.event(ev)?;
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
		self.open(None)?;
		Ok(())
	}
}
