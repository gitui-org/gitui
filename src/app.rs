use crate::{
	accessors,
	cmdbar::CommandBar,
	components::{
		command_pump, event_pump, CommandInfo, Component,
		DrawableComponent, FuzzyFinderTarget,
	},
	input::{Input, InputEvent, InputState},
	keys::{key_match, KeyConfig, SharedKeyConfig},
	options::{Options, SharedOptions},
	popup_stack::PopupStack,
	popups::{
		AppOption, BlameFilePopup, BranchListPopup, CommitPopup,
		CompareCommitsPopup, ConfirmPopup, CreateBranchPopup,
		CreateRemotePopup, ExternalEditorPopup, FetchPopup,
		FileRevlogPopup, FuzzyFindPopup, HelpPopup,
		InspectCommitPopup, LogSearchPopupPopup, MsgPopup,
		OptionsPopup, PullPopup, PushPopup, PushTagsPopup,
		RemoteListPopup, RenameBranchPopup, RenameRemotePopup,
		ResetPopup, RevisionFilesPopup, StashMsgPopup,
		SubmodulesListPopup, TagCommitPopup, TagListPopup,
		UpdateRemoteUrlPopup,
	},
	queue::{
		Action, AppTabs, InternalEvent, NeedsUpdate, Queue,
		StackablePopupOpen,
	},
	setup_popups,
	strings::{self, ellipsis_trim_start, order},
	tabs::{FilesTab, Revlog, StashList, Stashing, Status},
	try_or_popup,
	ui::style::{SharedTheme, Theme},
	AsyncAppNotification, AsyncNotification,
};
use anyhow::{bail, Result};
use asyncgit::{
	sync::{
		self,
		utils::{repo_work_dir, undo_last_commit},
		RepoPath, RepoPathRef,
	},
	AsyncGitNotification, PushType,
};
use crossbeam_channel::Sender;
use crossterm::event::{Event, KeyEvent};
use ratatui::{
	layout::{
		Alignment, Constraint, Direction, Layout, Margin, Rect,
	},
	text::{Line, Span},
	widgets::{Block, Borders, Paragraph, Tabs},
	Frame,
};
use std::{
	cell::{Cell, RefCell},
	path::{Path, PathBuf},
	rc::Rc,
};
use unicode_width::UnicodeWidthStr;

#[derive(Clone)]
pub enum QuitState {
	None,
	Close,
	OpenSubmodule(RepoPath),
}

/// the main app type
pub struct App {
	repo: RepoPathRef,
	do_quit: QuitState,
	help_popup: HelpPopup,
	msg_popup: MsgPopup,
	confirm_popup: ConfirmPopup,
	commit_popup: CommitPopup,
	blame_file_popup: BlameFilePopup,
	file_revlog_popup: FileRevlogPopup,
	stashmsg_popup: StashMsgPopup,
	inspect_commit_popup: InspectCommitPopup,
	compare_commits_popup: CompareCommitsPopup,
	external_editor_popup: ExternalEditorPopup,
	revision_files_popup: RevisionFilesPopup,
	fuzzy_find_popup: FuzzyFindPopup,
	log_search_popup: LogSearchPopupPopup,
	push_popup: PushPopup,
	push_tags_popup: PushTagsPopup,
	pull_popup: PullPopup,
	fetch_popup: FetchPopup,
	tag_commit_popup: TagCommitPopup,
	create_branch_popup: CreateBranchPopup,
	create_remote_popup: CreateRemotePopup,
	rename_remote_popup: RenameRemotePopup,
	update_remote_url_popup: UpdateRemoteUrlPopup,
	remotes_popup: RemoteListPopup,
	rename_branch_popup: RenameBranchPopup,
	select_branch_popup: BranchListPopup,
	options_popup: OptionsPopup,
	submodule_popup: SubmodulesListPopup,
	tags_popup: TagListPopup,
	reset_popup: ResetPopup,
	cmdbar: RefCell<CommandBar>,
	tab: usize,
	revlog: Revlog,
	status_tab: Status,
	stashing_tab: Stashing,
	stashlist_tab: StashList,
	files_tab: FilesTab,
	queue: Queue,
	theme: SharedTheme,
	key_config: SharedKeyConfig,
	input: Input,
	popup_stack: PopupStack,
	options: SharedOptions,
	repo_path_text: String,

	// "Flags"
	requires_redraw: Cell<bool>,
	file_to_open: Option<String>,
}

pub struct Environment {
	pub queue: Queue,
	pub theme: SharedTheme,
	pub key_config: SharedKeyConfig,
	pub repo: RepoPathRef,
	pub options: SharedOptions,
	pub sender_git: Sender<AsyncGitNotification>,
	pub sender_app: Sender<AsyncAppNotification>,
}

/// The need to construct a "whatever" environment only arises in testing right now
#[cfg(test)]
impl Environment {
	pub fn test_env() -> Self {
		use crossbeam_channel::unbounded;
		Self {
			queue: Queue::new(),
			theme: Default::default(),
			key_config: Default::default(),
			repo: RefCell::new(RepoPath::Path(Default::default())),
			options: Rc::new(RefCell::new(Options::test_env())),
			sender_git: unbounded().0,
			sender_app: unbounded().0,
		}
	}
}

// public interface
impl App {
	///
	#[allow(clippy::too_many_lines)]
	pub fn new(
		repo: RepoPathRef,
		sender_git: Sender<AsyncGitNotification>,
		sender_app: Sender<AsyncAppNotification>,
		input: Input,
		theme: Theme,
		key_config: KeyConfig,
	) -> Result<Self> {
		log::trace!("open repo at: {:?}", &repo);

		let repo_path_text =
			repo_work_dir(&repo.borrow()).unwrap_or_default();

		let env = Environment {
			queue: Queue::new(),
			theme: Rc::new(theme),
			key_config: Rc::new(key_config),
			options: Options::new(repo.clone()),
			repo,
			sender_git,
			sender_app,
		};

		let tab = env.options.borrow().current_tab();

		let mut app = Self {
			input,
			confirm_popup: ConfirmPopup::new(&env),
			commit_popup: CommitPopup::new(&env),
			blame_file_popup: BlameFilePopup::new(
				&env,
				&strings::blame_title(&env.key_config),
			),
			file_revlog_popup: FileRevlogPopup::new(&env),
			revision_files_popup: RevisionFilesPopup::new(&env),
			stashmsg_popup: StashMsgPopup::new(&env),
			inspect_commit_popup: InspectCommitPopup::new(&env),
			compare_commits_popup: CompareCommitsPopup::new(&env),
			external_editor_popup: ExternalEditorPopup::new(&env),
			push_popup: PushPopup::new(&env),
			push_tags_popup: PushTagsPopup::new(&env),
			reset_popup: ResetPopup::new(&env),
			pull_popup: PullPopup::new(&env),
			fetch_popup: FetchPopup::new(&env),
			tag_commit_popup: TagCommitPopup::new(&env),
			create_branch_popup: CreateBranchPopup::new(&env),
			create_remote_popup: CreateRemotePopup::new(&env),
			rename_remote_popup: RenameRemotePopup::new(&env),
			update_remote_url_popup: UpdateRemoteUrlPopup::new(&env),
			remotes_popup: RemoteListPopup::new(&env),
			rename_branch_popup: RenameBranchPopup::new(&env),
			select_branch_popup: BranchListPopup::new(&env),
			tags_popup: TagListPopup::new(&env),
			options_popup: OptionsPopup::new(&env),
			submodule_popup: SubmodulesListPopup::new(&env),
			log_search_popup: LogSearchPopupPopup::new(&env),
			fuzzy_find_popup: FuzzyFindPopup::new(&env),
			do_quit: QuitState::None,
			cmdbar: RefCell::new(CommandBar::new(
				env.theme.clone(),
				env.key_config.clone(),
			)),
			help_popup: HelpPopup::new(&env),
			msg_popup: MsgPopup::new(&env),
			revlog: Revlog::new(&env),
			status_tab: Status::new(&env),
			stashing_tab: Stashing::new(&env),
			stashlist_tab: StashList::new(&env),
			files_tab: FilesTab::new(&env),
			tab: 0,
			queue: env.queue,
			theme: env.theme,
			options: env.options,
			key_config: env.key_config,
			requires_redraw: Cell::new(false),
			file_to_open: None,
			repo: env.repo,
			repo_path_text,
			popup_stack: PopupStack::default(),
		};

		app.set_tab(tab)?;

		Ok(app)
	}

	///
	pub fn draw(&self, f: &mut Frame) -> Result<()> {
		let fsize = f.area();

		self.cmdbar.borrow_mut().refresh_width(fsize.width);

		let chunks_main = Layout::default()
			.direction(Direction::Vertical)
			.constraints(
				[
					Constraint::Length(2),
					Constraint::Min(2),
					Constraint::Length(self.cmdbar.borrow().height()),
				]
				.as_ref(),
			)
			.split(fsize);

		self.cmdbar.borrow().draw(f, chunks_main[2]);

		self.draw_top_bar(f, chunks_main[0]);

		//TODO: component property + a macro `fullscreen_popup_open!`
		// to make this scale better?
		let fullscreen_popup_open =
			self.revision_files_popup.is_visible()
				|| self.inspect_commit_popup.is_visible()
				|| self.compare_commits_popup.is_visible()
				|| self.blame_file_popup.is_visible()
				|| self.file_revlog_popup.is_visible();

		if !fullscreen_popup_open {
			//TODO: macro because of generic draw call
			match self.tab {
				0 => self.status_tab.draw(f, chunks_main[1])?,
				1 => self.revlog.draw(f, chunks_main[1])?,
				2 => self.files_tab.draw(f, chunks_main[1])?,
				3 => self.stashing_tab.draw(f, chunks_main[1])?,
				4 => self.stashlist_tab.draw(f, chunks_main[1])?,
				_ => bail!("unknown tab"),
			}
		}

		self.draw_popups(f)?;

		Ok(())
	}

	///
	pub fn event(&mut self, ev: InputEvent) -> Result<()> {
		log::trace!("event: {:?}", ev);

		if let InputEvent::Input(ev) = ev {
			if self.check_hard_exit(&ev) || self.check_quit(&ev) {
				return Ok(());
			}

			let mut flags = NeedsUpdate::empty();

			if event_pump(&ev, self.components_mut().as_mut_slice())?
				.is_consumed()
			{
				flags.insert(NeedsUpdate::COMMANDS);
			} else if let Event::Key(k) = &ev {
				let new_flags = if key_match(
					k,
					self.key_config.keys.tab_toggle,
				) {
					self.toggle_tabs(false)?;
					NeedsUpdate::COMMANDS
				} else if key_match(
					k,
					self.key_config.keys.tab_toggle_reverse,
				) {
					self.toggle_tabs(true)?;
					NeedsUpdate::COMMANDS
				} else if key_match(
					k,
					self.key_config.keys.tab_status,
				) || key_match(
					k,
					self.key_config.keys.tab_log,
				) || key_match(
					k,
					self.key_config.keys.tab_files,
				) || key_match(
					k,
					self.key_config.keys.tab_stashing,
				) || key_match(
					k,
					self.key_config.keys.tab_stashes,
				) {
					self.switch_tab(k)?;
					NeedsUpdate::COMMANDS
				} else if key_match(
					k,
					self.key_config.keys.cmd_bar_toggle,
				) {
					self.cmdbar.borrow_mut().toggle_more();
					NeedsUpdate::empty()
				} else if key_match(
					k,
					self.key_config.keys.open_options,
				) {
					self.options_popup.show()?;
					NeedsUpdate::ALL
				} else {
					NeedsUpdate::empty()
				};

				flags.insert(new_flags);
			}

			self.process_queue(flags)?;
		} else if let InputEvent::State(polling_state) = ev {
			self.external_editor_popup.hide();
			if matches!(polling_state, InputState::Paused) {
				let result =
					if let Some(path) = self.file_to_open.take() {
						ExternalEditorPopup::open_file_in_editor(
							&self.repo.borrow(),
							Path::new(&path),
						)
					} else {
						let changes =
							self.status_tab.get_files_changes()?;
						self.commit_popup.show_editor(changes)
					};

				if let Err(e) = result {
					let msg =
						format!("failed to launch editor:\n{e}");
					log::error!("{}", msg.as_str());
					self.msg_popup.show_error(msg.as_str())?;
				}

				self.requires_redraw.set(true);
				self.input.set_polling(true);
			}
		}

		Ok(())
	}

	//TODO: do we need this?
	/// forward ticking to components that require it
	pub fn update(&mut self) -> Result<()> {
		log::trace!("update");

		self.commit_popup.update();
		self.status_tab.update()?;
		self.revlog.update()?;
		self.files_tab.update()?;
		self.stashing_tab.update()?;
		self.stashlist_tab.update()?;
		self.reset_popup.update()?;

		self.update_commands();

		Ok(())
	}

	///
	pub fn update_async(
		&mut self,
		ev: AsyncNotification,
	) -> Result<()> {
		log::trace!("update_async: {:?}", ev);

		if let AsyncNotification::Git(ev) = ev {
			self.status_tab.update_git(ev)?;
			self.stashing_tab.update_git(ev)?;
			self.revlog.update_git(ev)?;
			self.file_revlog_popup.update_git(ev)?;
			self.inspect_commit_popup.update_git(ev)?;
			self.compare_commits_popup.update_git(ev)?;
			self.push_popup.update_git(ev)?;
			self.push_tags_popup.update_git(ev)?;
			self.pull_popup.update_git(ev);
			self.fetch_popup.update_git(ev);
			self.select_branch_popup.update_git(ev)?;
		}

		self.files_tab.update_async(ev)?;
		self.blame_file_popup.update_async(ev)?;
		self.revision_files_popup.update(ev)?;
		self.tags_popup.update(ev);

		//TODO: better system for this
		// can we simply process the queue here and everyone just uses the queue to schedule a cmd update?
		self.process_queue(NeedsUpdate::COMMANDS)?;

		Ok(())
	}

	///
	pub fn is_quit(&self) -> bool {
		!matches!(self.do_quit, QuitState::None)
			|| self.input.is_aborted()
	}

	///
	pub fn quit_state(&self) -> QuitState {
		self.do_quit.clone()
	}

	///
	pub fn any_work_pending(&self) -> bool {
		self.status_tab.anything_pending()
			|| self.revlog.any_work_pending()
			|| self.stashing_tab.anything_pending()
			|| self.files_tab.anything_pending()
			|| self.blame_file_popup.any_work_pending()
			|| self.file_revlog_popup.any_work_pending()
			|| self.inspect_commit_popup.any_work_pending()
			|| self.compare_commits_popup.any_work_pending()
			|| self.input.is_state_changing()
			|| self.push_popup.any_work_pending()
			|| self.push_tags_popup.any_work_pending()
			|| self.pull_popup.any_work_pending()
			|| self.fetch_popup.any_work_pending()
			|| self.revision_files_popup.any_work_pending()
			|| self.tags_popup.any_work_pending()
	}

	///
	pub fn requires_redraw(&self) -> bool {
		if self.requires_redraw.get() {
			self.requires_redraw.set(false);
			true
		} else {
			false
		}
	}
}

// private impls
impl App {
	accessors!(
		self,
		[
			log_search_popup,
			fuzzy_find_popup,
			msg_popup,
			confirm_popup,
			commit_popup,
			blame_file_popup,
			file_revlog_popup,
			stashmsg_popup,
			inspect_commit_popup,
			compare_commits_popup,
			external_editor_popup,
			push_popup,
			push_tags_popup,
			pull_popup,
			fetch_popup,
			tag_commit_popup,
			reset_popup,
			create_branch_popup,
			create_remote_popup,
			rename_remote_popup,
			update_remote_url_popup,
			remotes_popup,
			rename_branch_popup,
			select_branch_popup,
			revision_files_popup,
			submodule_popup,
			tags_popup,
			options_popup,
			help_popup,
			revlog,
			status_tab,
			files_tab,
			stashing_tab,
			stashlist_tab
		]
	);

	setup_popups!(
		self,
		[
			commit_popup,
			stashmsg_popup,
			help_popup,
			inspect_commit_popup,
			compare_commits_popup,
			blame_file_popup,
			file_revlog_popup,
			external_editor_popup,
			tag_commit_popup,
			select_branch_popup,
			remotes_popup,
			create_remote_popup,
			rename_remote_popup,
			update_remote_url_popup,
			submodule_popup,
			tags_popup,
			reset_popup,
			create_branch_popup,
			rename_branch_popup,
			revision_files_popup,
			fuzzy_find_popup,
			log_search_popup,
			push_popup,
			push_tags_popup,
			pull_popup,
			fetch_popup,
			options_popup,
			confirm_popup,
			msg_popup
		]
	);

	fn check_quit(&mut self, ev: &Event) -> bool {
		if self.any_popup_visible() {
			return false;
		}
		if let Event::Key(e) = ev {
			if key_match(e, self.key_config.keys.quit) {
				self.do_quit = QuitState::Close;
				return true;
			}
		}
		false
	}

	fn check_hard_exit(&mut self, ev: &Event) -> bool {
		if let Event::Key(e) = ev {
			if key_match(e, self.key_config.keys.exit) {
				self.do_quit = QuitState::Close;
				return true;
			}
		}
		false
	}

	fn get_tabs(&mut self) -> Vec<&mut dyn Component> {
		vec![
			&mut self.status_tab,
			&mut self.revlog,
			&mut self.files_tab,
			&mut self.stashing_tab,
			&mut self.stashlist_tab,
		]
	}

	fn toggle_tabs(&mut self, reverse: bool) -> Result<()> {
		let tabs_len = self.get_tabs().len();
		let new_tab = if reverse {
			self.tab.wrapping_sub(1).min(tabs_len.saturating_sub(1))
		} else {
			self.tab.saturating_add(1) % tabs_len
		};

		self.set_tab(new_tab)
	}

	fn switch_tab(&mut self, k: &KeyEvent) -> Result<()> {
		if key_match(k, self.key_config.keys.tab_status) {
			self.switch_to_tab(&AppTabs::Status)?;
		} else if key_match(k, self.key_config.keys.tab_log) {
			self.switch_to_tab(&AppTabs::Log)?;
		} else if key_match(k, self.key_config.keys.tab_files) {
			self.switch_to_tab(&AppTabs::Files)?;
		} else if key_match(k, self.key_config.keys.tab_stashing) {
			self.switch_to_tab(&AppTabs::Stashing)?;
		} else if key_match(k, self.key_config.keys.tab_stashes) {
			self.switch_to_tab(&AppTabs::Stashlist)?;
		}

		Ok(())
	}

	fn set_tab(&mut self, tab: usize) -> Result<()> {
		let tabs = self.get_tabs();
		for (i, t) in tabs.into_iter().enumerate() {
			if tab == i {
				t.show()?;
			} else {
				t.hide();
			}
		}

		self.tab = tab;
		self.options.borrow_mut().set_current_tab(tab);

		Ok(())
	}

	fn switch_to_tab(&mut self, tab: &AppTabs) -> Result<()> {
		match tab {
			AppTabs::Status => self.set_tab(0)?,
			AppTabs::Log => self.set_tab(1)?,
			AppTabs::Files => self.set_tab(2)?,
			AppTabs::Stashing => self.set_tab(3)?,
			AppTabs::Stashlist => self.set_tab(4)?,
		}
		Ok(())
	}

	fn update_commands(&mut self) {
		if self.help_popup.is_visible() {
			self.help_popup.set_cmds(self.commands(true));
		}
		self.cmdbar.borrow_mut().set_cmds(self.commands(false));
	}

	fn process_queue(&mut self, flags: NeedsUpdate) -> Result<()> {
		let mut flags = flags;
		let new_flags = self.process_internal_events()?;
		flags.insert(new_flags);

		if flags.contains(NeedsUpdate::ALL) {
			self.update()?;
		}
		//TODO: make this a queue event?
		//NOTE: set when any tree component changed selection
		if flags.contains(NeedsUpdate::DIFF) {
			self.status_tab.update_diff()?;
			self.inspect_commit_popup.update_diff()?;
			self.compare_commits_popup.update_diff()?;
			self.file_revlog_popup.update_diff()?;
		}
		if flags.contains(NeedsUpdate::COMMANDS) {
			self.update_commands();
		}
		if flags.contains(NeedsUpdate::BRANCHES) {
			self.select_branch_popup.update_branches()?;
		}
		if flags.contains(NeedsUpdate::REMOTES) {
			self.remotes_popup.update_remotes()?;
		}

		Ok(())
	}

	fn open_popup(
		&mut self,
		popup: StackablePopupOpen,
	) -> Result<()> {
		match popup {
			StackablePopupOpen::BlameFile(params) => {
				self.blame_file_popup.open(params)?;
			}
			StackablePopupOpen::FileRevlog(param) => {
				self.file_revlog_popup.open(param)?;
			}
			StackablePopupOpen::FileTree(param) => {
				self.revision_files_popup.open(param)?;
			}
			StackablePopupOpen::InspectCommit(param) => {
				self.inspect_commit_popup.open(param)?;
			}
			StackablePopupOpen::CompareCommits(param) => {
				self.compare_commits_popup.open(param)?;
			}
		}

		Ok(())
	}

	fn process_internal_events(&mut self) -> Result<NeedsUpdate> {
		let mut flags = NeedsUpdate::empty();

		loop {
			let front = self.queue.pop();
			if let Some(e) = front {
				flags.insert(self.process_internal_event(e)?);
			} else {
				break;
			}
		}
		self.queue.clear();

		Ok(flags)
	}

	#[allow(clippy::too_many_lines)]
	fn process_internal_event(
		&mut self,
		ev: InternalEvent,
	) -> Result<NeedsUpdate> {
		let mut flags = NeedsUpdate::empty();
		match ev {
			InternalEvent::ConfirmedAction(action) => {
				self.process_confirmed_action(action, &mut flags)?;
			}
			InternalEvent::ConfirmAction(action) => {
				self.confirm_popup.open(action)?;
				flags.insert(NeedsUpdate::COMMANDS);
			}
			InternalEvent::ShowErrorMsg(msg) => {
				self.msg_popup.show_error(msg.as_str())?;
				flags
					.insert(NeedsUpdate::ALL | NeedsUpdate::COMMANDS);
			}
			InternalEvent::ShowInfoMsg(msg) => {
				self.msg_popup.show_info(msg.as_str())?;
				flags
					.insert(NeedsUpdate::ALL | NeedsUpdate::COMMANDS);
			}
			InternalEvent::Update(u) => flags.insert(u),
			InternalEvent::OpenCommit => self.commit_popup.show()?,
			InternalEvent::RewordCommit(id) => {
				self.commit_popup.open(Some(id))?;
			}
			InternalEvent::PopupStashing(opts) => {
				self.stashmsg_popup.options(opts);
				self.stashmsg_popup.show()?;
			}
			InternalEvent::TagCommit(id) => {
				self.tag_commit_popup.open(id)?;
			}
			InternalEvent::CreateRemote => {
				self.create_remote_popup.open()?;
			}
			InternalEvent::RenameRemote(cur_name) => {
				self.rename_remote_popup.open(cur_name)?;
			}
			InternalEvent::UpdateRemoteUrl(remote_name, cur_url) => {
				self.update_remote_url_popup
					.open(remote_name, cur_url)?;
			}
			InternalEvent::ViewRemotes => {
				self.remotes_popup.open()?;
			}
			InternalEvent::CreateBranch => {
				self.create_branch_popup.open()?;
			}
			InternalEvent::RenameBranch(branch_ref, cur_name) => {
				self.rename_branch_popup
					.open(branch_ref, cur_name)?;
			}
			InternalEvent::SelectBranch => {
				self.select_branch_popup.open()?;
			}
			InternalEvent::ViewSubmodules => {
				self.submodule_popup.open()?;
			}
			InternalEvent::Tags => {
				self.tags_popup.open()?;
			}
			InternalEvent::TabSwitchStatus => self.set_tab(0)?,
			InternalEvent::TabSwitch(tab) => {
				self.switch_to_tab(&tab)?;
				flags.insert(NeedsUpdate::ALL);
			}
			InternalEvent::SelectCommitInRevlog(id) => {
				if let Err(error) = self.revlog.select_commit(id) {
					self.queue.push(InternalEvent::ShowErrorMsg(
						error.to_string(),
					));
				} else {
					self.tags_popup.hide();
					flags.insert(NeedsUpdate::ALL);
				}
			}
			InternalEvent::OpenExternalEditor(path) => {
				self.input.set_polling(false);
				self.external_editor_popup.show()?;
				self.file_to_open = path;
				flags.insert(NeedsUpdate::COMMANDS);
			}
			InternalEvent::Push(branch, push_type, force, delete) => {
				self.push_popup
					.push(branch, push_type, force, delete)?;
				flags.insert(NeedsUpdate::ALL);
			}
			InternalEvent::Pull(branch) => {
				if let Err(error) = self.pull_popup.fetch(branch) {
					self.queue.push(InternalEvent::ShowErrorMsg(
						error.to_string(),
					));
				}
				flags.insert(NeedsUpdate::ALL);
			}
			InternalEvent::FetchRemotes => {
				if let Err(error) = self.fetch_popup.fetch() {
					self.queue.push(InternalEvent::ShowErrorMsg(
						error.to_string(),
					));
				}
				flags.insert(NeedsUpdate::ALL);
			}
			InternalEvent::PushTags => {
				self.push_tags_popup.push_tags()?;
				flags.insert(NeedsUpdate::ALL);
			}
			InternalEvent::StatusLastFileMoved => {
				self.status_tab.last_file_moved()?;
			}
			InternalEvent::OpenFuzzyFinder(contents, target) => {
				self.fuzzy_find_popup.open(contents, target)?;
				flags
					.insert(NeedsUpdate::ALL | NeedsUpdate::COMMANDS);
			}
			InternalEvent::OpenLogSearchPopup => {
				self.log_search_popup.open()?;
				flags
					.insert(NeedsUpdate::ALL | NeedsUpdate::COMMANDS);
			}
			InternalEvent::OptionSwitched(o) => {
				match o {
					AppOption::StatusShowUntracked => {
						self.status_tab.update()?;
					}
					AppOption::DiffContextLines
					| AppOption::DiffIgnoreWhitespaces
					| AppOption::DiffInterhunkLines => {
						self.status_tab.update_diff()?;
					}
				}

				flags.insert(NeedsUpdate::ALL);
			}
			InternalEvent::FuzzyFinderChanged(
				idx,
				content,
				target,
			) => {
				match target {
					FuzzyFinderTarget::Branches => self
						.select_branch_popup
						.branch_finder_update(idx)?,
					FuzzyFinderTarget::Files => {
						self.files_tab.file_finder_update(
							&PathBuf::from(content.clone()),
						);
						self.revision_files_popup.file_finder_update(
							&PathBuf::from(content),
						);
					}
				}

				flags
					.insert(NeedsUpdate::ALL | NeedsUpdate::COMMANDS);
			}
			InternalEvent::OpenPopup(popup) => {
				self.open_popup(popup)?;
				flags
					.insert(NeedsUpdate::ALL | NeedsUpdate::COMMANDS);
			}
			InternalEvent::PopupStackPop => {
				if let Some(popup) = self.popup_stack.pop() {
					self.open_popup(popup)?;
					flags.insert(
						NeedsUpdate::ALL | NeedsUpdate::COMMANDS,
					);
				}
			}
			InternalEvent::PopupStackPush(popup) => {
				self.popup_stack.push(popup);
				flags
					.insert(NeedsUpdate::ALL | NeedsUpdate::COMMANDS);
			}
			InternalEvent::OpenRepo { path } => {
				let submodule_repo_path = RepoPath::Path(
					Path::new(&repo_work_dir(&self.repo.borrow())?)
						.join(path),
				);
				//TODO: validate this is a valid repo first, so we can show proper error otherwise
				self.do_quit =
					QuitState::OpenSubmodule(submodule_repo_path);
			}
			InternalEvent::OpenResetPopup(id) => {
				self.reset_popup.open(id)?;
			}
			InternalEvent::CommitSearch(options) => {
				self.revlog.search(options);
			}
		}

		Ok(flags)
	}

	fn process_confirmed_action(
		&mut self,
		action: Action,
		flags: &mut NeedsUpdate,
	) -> Result<()> {
		match action {
			Action::Reset(r) => {
				self.status_tab.reset(&r);
			}
			Action::StashDrop(_) | Action::StashPop(_) => {
				if let Err(e) = self
					.stashlist_tab
					.action_confirmed(&self.repo.borrow(), &action)
				{
					self.queue.push(InternalEvent::ShowErrorMsg(
						e.to_string(),
					));
				}
			}
			Action::ResetHunk(path, hash) => {
				sync::reset_hunk(
					&self.repo.borrow(),
					&path,
					hash,
					Some(self.options.borrow().diff_options()),
				)?;
			}
			Action::ResetLines(path, lines) => {
				sync::discard_lines(
					&self.repo.borrow(),
					&path,
					&lines,
				)?;
			}
			Action::DeleteLocalBranch(branch_ref) => {
				if let Err(e) = sync::delete_branch(
					&self.repo.borrow(),
					&branch_ref,
				) {
					self.queue.push(InternalEvent::ShowErrorMsg(
						e.to_string(),
					));
				}

				self.select_branch_popup.update_branches()?;
			}
			Action::DeleteRemoteBranch(branch_ref) => {
				self.delete_remote_branch(&branch_ref)?;
			}
			Action::DeleteRemote(remote_name) => {
				self.delete_remote(&remote_name);
			}
			Action::DeleteTag(tag_name) => {
				self.delete_tag(tag_name)?;
			}
			Action::DeleteRemoteTag(tag_name, _remote) => {
				self.queue.push(InternalEvent::Push(
					tag_name,
					PushType::Tag,
					false,
					true,
				));
			}
			Action::ForcePush(branch, force) => {
				self.queue.push(InternalEvent::Push(
					branch,
					PushType::Branch,
					force,
					false,
				));
			}
			Action::PullMerge { rebase, .. } => {
				self.pull_popup.try_conflict_free_merge(rebase);
			}
			Action::AbortRevert | Action::AbortMerge => {
				self.status_tab.revert_pending_state();
			}
			Action::AbortRebase => {
				self.status_tab.abort_rebase();
			}
			Action::UndoCommit => {
				try_or_popup!(
					self,
					"undo commit failed:",
					undo_last_commit(&self.repo.borrow())
				);
			}
		}

		flags.insert(NeedsUpdate::ALL);

		Ok(())
	}

	fn delete_tag(&mut self, tag_name: String) -> Result<()> {
		if let Err(error) =
			sync::delete_tag(&self.repo.borrow(), &tag_name)
		{
			self.queue
				.push(InternalEvent::ShowErrorMsg(error.to_string()));
		} else {
			let remote =
				sync::get_default_remote(&self.repo.borrow())?;

			self.queue.push(InternalEvent::ConfirmAction(
				Action::DeleteRemoteTag(tag_name, remote),
			));

			self.tags_popup.update_tags()?;
		}
		Ok(())
	}

	fn delete_remote_branch(
		&mut self,
		branch_ref: &str,
	) -> Result<()> {
		self.queue.push(
			//TODO: check if this is correct based on the fix in `c6abbaf`
			branch_ref.rsplit('/').next().map_or_else(
				|| {
					InternalEvent::ShowErrorMsg(format!(
						    "Failed to find the branch name in {branch_ref}"
					    ))
				},
				|name| {
					InternalEvent::Push(
						name.to_string(),
						PushType::Branch,
						false,
						true,
					)
				},
			),
		);

		self.select_branch_popup.update_branches()?;

		Ok(())
	}

	fn delete_remote(&self, remote_name: &str) {
		let res =
			sync::delete_remote(&self.repo.borrow(), remote_name);
		match res {
			Ok(()) => {
				self.queue.push(InternalEvent::Update(
					NeedsUpdate::ALL | NeedsUpdate::REMOTES,
				));
			}
			Err(e) => {
				log::error!("delete remote: {}", e,);
				self.queue.push(InternalEvent::ShowErrorMsg(
					format!("delete remote error:\n{e}",),
				));
			}
		}
	}

	fn commands(&self, force_all: bool) -> Vec<CommandInfo> {
		let mut res = Vec::new();

		command_pump(&mut res, force_all, &self.components());

		res.push(CommandInfo::new(
			strings::commands::find_file(&self.key_config),
			!self.fuzzy_find_popup.is_visible(),
			(!self.any_popup_visible()
				&& self.files_tab.is_visible())
				|| self.revision_files_popup.is_visible()
				|| force_all,
		));

		res.push(
			CommandInfo::new(
				strings::commands::toggle_tabs(&self.key_config),
				true,
				!self.any_popup_visible(),
			)
			.order(order::NAV),
		);
		res.push(
			CommandInfo::new(
				strings::commands::toggle_tabs_direct(
					&self.key_config,
				),
				true,
				!self.any_popup_visible(),
			)
			.order(order::NAV),
		);
		res.push(
			CommandInfo::new(
				strings::commands::options_popup(&self.key_config),
				true,
				!self.any_popup_visible(),
			)
			.order(order::NAV),
		);

		res.push(
			CommandInfo::new(
				strings::commands::quit(&self.key_config),
				true,
				!self.any_popup_visible(),
			)
			.order(100),
		);

		res
	}

	//TODO: make this dynamic
	fn draw_top_bar(&self, f: &mut Frame, r: Rect) {
		const DIVIDER_PAD_SPACES: usize = 2;
		const SIDE_PADS: usize = 2;
		const MARGIN_LEFT_AND_RIGHT: usize = 2;

		let r = r.inner(Margin {
			vertical: 0,
			horizontal: 1,
		});

		let tab_labels = [
			Span::raw(strings::tab_status(&self.key_config)),
			Span::raw(strings::tab_log(&self.key_config)),
			Span::raw(strings::tab_files(&self.key_config)),
			Span::raw(strings::tab_stashing(&self.key_config)),
			Span::raw(strings::tab_stashes(&self.key_config)),
		];
		let divider = strings::tab_divider(&self.key_config);

		// heuristic, since tui doesn't provide a way to know
		// how much space is needed to draw a `Tabs`
		let tabs_len: usize =
			tab_labels.iter().map(Span::width).sum::<usize>()
				+ tab_labels.len().saturating_sub(1)
					* (divider.width() + DIVIDER_PAD_SPACES)
				+ SIDE_PADS + MARGIN_LEFT_AND_RIGHT;

		let left_right = Layout::default()
			.direction(Direction::Horizontal)
			.constraints(vec![
				Constraint::Length(
					u16::try_from(tabs_len).unwrap_or(r.width),
				),
				Constraint::Min(0),
			])
			.split(r);

		let table_area = r; // use entire area to allow drawing the horizontal separator line
		let text_area = left_right[1];

		let tabs: Vec<Line> =
			tab_labels.into_iter().map(Line::from).collect();

		f.render_widget(
			Tabs::new(tabs)
				.block(
					Block::default()
						.borders(Borders::BOTTOM)
						.border_style(self.theme.block(false)),
				)
				.style(self.theme.tab(false))
				.highlight_style(self.theme.tab(true))
				.divider(divider)
				.select(self.tab),
			table_area,
		);

		f.render_widget(
			Paragraph::new(Line::from(vec![Span::styled(
				ellipsis_trim_start(
					&self.repo_path_text,
					text_area.width as usize,
				),
				self.theme.title(false),
			)]))
			.alignment(Alignment::Right),
			text_area,
		);
	}
}
