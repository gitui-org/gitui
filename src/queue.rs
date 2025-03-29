use crate::{
	components::FuzzyFinderTarget,
	popups::{
		AppOption, BlameFileOpen, FileRevOpen, FileTreeOpen,
		InspectCommitOpen,
	},
	tabs::StashingOptions,
};
use asyncgit::{
	sync::{
		diff::DiffLinePosition, CommitId, LogFilterSearchOptions,
	},
	PushType,
};
use bitflags::bitflags;
use std::{
	cell::RefCell, collections::VecDeque, path::PathBuf, rc::Rc,
};

bitflags! {
	/// flags defining what part of the app need to update
	pub struct NeedsUpdate: u32 {
		/// app::update
		const ALL = 0b001;
		/// diff may have changed (app::update_diff)
		const DIFF = 0b010;
		/// commands might need updating (app::update_commands)
		const COMMANDS = 0b100;
		/// branches have changed
		const BRANCHES = 0b1000;
		/// Remotes have changed
		const REMOTES = 0b1001;
	}
}

/// data of item that is supposed to be reset
pub struct ResetItem {
	/// path to the item (folder/file)
	pub path: String,
}

///
pub enum Action {
	Reset(ResetItem),
	ResetHunk(String, u64),
	ResetLines(String, Vec<DiffLinePosition>),
	StashDrop(Vec<CommitId>),
	StashPop(CommitId),
	DeleteLocalBranch(String),
	DeleteRemoteBranch(String),
	DeleteTag(String),
	DeleteRemoteTag(String, String),
	DeleteRemote(String),
	ForcePush(String, bool),
	PullMerge { incoming: usize, rebase: bool },
	AbortMerge,
	AbortRebase,
	AbortRevert,
	UndoCommit,
}

#[derive(Debug)]
pub enum StackablePopupOpen {
	///
	BlameFile(BlameFileOpen),
	///
	FileRevlog(FileRevOpen),
	///
	FileTree(FileTreeOpen),
	///
	InspectCommit(InspectCommitOpen),
	///
	CompareCommits(InspectCommitOpen),
}

pub enum AppTabs {
	Status,
	Log,
	Files,
	Stashing,
	Stashlist,
}

///
pub enum InternalEvent {
	///
	ConfirmAction(Action),
	///
	ConfirmedAction(Action),
	///
	ShowErrorMsg(String),
	///
	ShowInfoMsg(String),
	///
	Update(NeedsUpdate),
	///
	StatusLastFileMoved,
	/// open commit msg input
	OpenCommit,
	///
	PopupStashing(StashingOptions),
	///
	TabSwitchStatus,
	///
	TabSwitch(AppTabs),
	///
	SelectCommitInRevlog(CommitId),
	///
	TagCommit(CommitId),
	///
	Tags,
	///
	CreateBranch,
	///
	RenameRemote(String),
	///
	UpdateRemoteUrl(String, String),
	///
	RenameBranch(String, String),
	///
	SelectBranch,
	///
	OpenExternalEditor(Option<String>),
	///
	Push(String, PushType, bool, bool),
	///
	Pull(String),
	///
	PushTags,
	///
	OptionSwitched(AppOption),
	///
	OpenFuzzyFinder(Vec<String>, FuzzyFinderTarget),
	///
	OpenLogSearchPopup,
	///
	FuzzyFinderChanged(usize, String, FuzzyFinderTarget),
	///
	FetchRemotes,
	///
	OpenPopup(StackablePopupOpen),
	///
	PopupStackPop,
	///
	PopupStackPush(StackablePopupOpen),
	///
	ViewSubmodules,
	///
	ViewRemotes,
	///
	CreateRemote,
	///
	OpenRepo { path: PathBuf },
	///
	OpenResetPopup(CommitId),
	///
	RewordCommit(CommitId),
	///
	CommitSearch(LogFilterSearchOptions),
}

/// single threaded simple queue for components to communicate with each other
#[derive(Clone, Default)]
pub struct Queue {
	data: Rc<RefCell<VecDeque<InternalEvent>>>,
}

impl Queue {
	pub fn new() -> Self {
		Self {
			data: Rc::new(RefCell::new(VecDeque::new())),
		}
	}

	pub fn push(&self, ev: InternalEvent) {
		self.data.borrow_mut().push_back(ev);
	}

	pub fn pop(&self) -> Option<InternalEvent> {
		self.data.borrow_mut().pop_front()
	}

	pub fn clear(&self) {
		self.data.borrow_mut().clear();
	}
}
