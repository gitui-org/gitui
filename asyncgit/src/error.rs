use std::{
	num::TryFromIntError, path::StripPrefixError,
	string::FromUtf8Error,
};
use thiserror::Error;

///
#[derive(Error, Debug)]
pub enum GixError {
	///
	#[error("gix::discover error: {0}")]
	Discover(#[from] Box<gix::discover::Error>),

	///
	#[error("gix::head::peel::to_commit error: {0}")]
	HeadPeelToCommit(#[from] gix::head::peel::to_commit::Error),

	///
	#[error("gix::object::find::existing::with_conversion::Error error: {0}")]
	ObjectFindExistingWithConversion(
		#[from] gix::object::find::existing::with_conversion::Error,
	),

	///
	#[error("gix::objs::decode::Error error: {0}")]
	ObjsDecode(#[from] gix::objs::decode::Error),

	///
	#[error("gix::pathspec::init::Error error: {0}")]
	PathspecInit(#[from] Box<gix::pathspec::init::Error>),

	///
	#[error("gix::reference::find::existing error: {0}")]
	ReferenceFindExisting(
		#[from] gix::reference::find::existing::Error,
	),

	///
	#[error("gix::reference::head_tree_id::Error error: {0}")]
	ReferenceHeadTreeId(#[from] gix::reference::head_tree_id::Error),

	///
	#[error("gix::reference::iter::Error error: {0}")]
	ReferenceIter(#[from] gix::reference::iter::Error),

	///
	#[error("gix::reference::iter::init::Error error: {0}")]
	ReferenceIterInit(#[from] gix::reference::iter::init::Error),

	///
	#[error("gix::revision::walk error: {0}")]
	RevisionWalk(#[from] gix::revision::walk::Error),

	///
	#[error("gix::status::Error error: {0}")]
	Status(#[from] Box<gix::status::Error>),

	///
	#[error("gix::status::index_worktree::Error error: {0}")]
	StatusIndexWorktree(
		#[from] Box<gix::status::index_worktree::Error>,
	),

	///
	#[error("gix::status::into_iter::Error error: {0}")]
	StatusIntoIter(#[from] Box<gix::status::into_iter::Error>),

	///
	#[error("gix::status::iter::Error error: {0}")]
	StatusIter(#[from] Box<gix::status::iter::Error>),

	///
	#[error("gix::status::tree_index::Error error: {0}")]
	StatusTreeIndex(#[from] Box<gix::status::tree_index::Error>),

	///
	#[error("gix::worktree::open_index::Error error: {0}")]
	WorktreeOpenIndex(#[from] Box<gix::worktree::open_index::Error>),
}

///
#[derive(Error, Debug)]
pub enum Error {
	///
	#[error("`{0}`")]
	Generic(String),

	///
	#[error("git: no head found")]
	NoHead,

	///
	#[error("git: conflict during rebase")]
	RebaseConflict,

	///
	#[error("git: remote url not found")]
	UnknownRemote,

	///
	#[error("git: inconclusive remotes")]
	NoDefaultRemoteFound,

	///
	#[error("git: work dir error")]
	NoWorkDir,

	///
	#[error("git: uncommitted changes")]
	UncommittedChanges,

	///
	#[error("git: can\u{2019}t run blame on a binary file")]
	NoBlameOnBinaryFile,

	///
	#[error("binary file")]
	BinaryFile,

	///
	#[error("io error:{0}")]
	Io(#[from] std::io::Error),

	///
	#[error("git error:{0}")]
	Git(#[from] git2::Error),

	///
	#[error("git config error: {0}")]
	GitConfig(String),

	///
	#[error("strip prefix error: {0}")]
	StripPrefix(#[from] StripPrefixError),

	///
	#[error("utf8 error:{0}")]
	Utf8Conversion(#[from] FromUtf8Error),

	///
	#[error("TryFromInt error:{0}")]
	IntConversion(#[from] TryFromIntError),

	///
	#[error("EasyCast error:{0}")]
	EasyCast(#[from] easy_cast::Error),

	///
	#[error("no parent of commit found")]
	NoParent,

	///
	#[error("not on a branch")]
	NoBranch,

	///
	#[error("rayon error: {0}")]
	ThreadPool(#[from] rayon_core::ThreadPoolBuildError),

	///
	#[error("git hook error: {0}")]
	Hooks(#[from] git2_hooks::HooksError),

	///
	#[error("sign builder error: {0}")]
	SignBuilder(#[from] crate::sync::sign::SignBuilderError),

	///
	#[error("sign error: {0}")]
	Sign(#[from] crate::sync::sign::SignError),

	///
	#[error("gix error:{0}")]
	Gix(#[from] GixError),

	///
	#[error("amend error: config commit.gpgsign=true detected.\ngpg signing is not supported for amending non-last commits")]
	SignAmendNonLastCommit,

	///
	#[error("reword error: config commit.gpgsign=true detected.\ngpg signing is not supported for rewording non-last commits")]
	SignRewordNonLastCommit,

	///
	#[error("reword error: config commit.gpgsign=true detected.\ngpg signing is not supported for rewording commits with staged changes\ntry unstaging or stashing your changes")]
	SignRewordLastCommitStaged,
}

///
pub type Result<T> = std::result::Result<T, Error>;

impl<T> From<std::sync::PoisonError<T>> for Error {
	fn from(error: std::sync::PoisonError<T>) -> Self {
		Self::Generic(format!("poison error: {error}"))
	}
}

impl<T> From<crossbeam_channel::SendError<T>> for Error {
	fn from(error: crossbeam_channel::SendError<T>) -> Self {
		Self::Generic(format!("send error: {error}"))
	}
}

impl From<gix::discover::Error> for GixError {
	fn from(error: gix::discover::Error) -> Self {
		Self::Discover(Box::new(error))
	}
}

impl From<gix::discover::Error> for Error {
	fn from(error: gix::discover::Error) -> Self {
		Self::Gix(GixError::from(error))
	}
}

impl From<gix::head::peel::to_commit::Error> for Error {
	fn from(error: gix::head::peel::to_commit::Error) -> Self {
		Self::Gix(GixError::from(error))
	}
}

impl From<gix::object::find::existing::with_conversion::Error>
	for Error
{
	fn from(
		error: gix::object::find::existing::with_conversion::Error,
	) -> Self {
		Self::Gix(GixError::from(error))
	}
}

impl From<gix::objs::decode::Error> for Error {
	fn from(error: gix::objs::decode::Error) -> Self {
		Self::Gix(GixError::from(error))
	}
}

impl From<gix::pathspec::init::Error> for GixError {
	fn from(error: gix::pathspec::init::Error) -> Self {
		Self::PathspecInit(Box::new(error))
	}
}

impl From<gix::pathspec::init::Error> for Error {
	fn from(error: gix::pathspec::init::Error) -> Self {
		Self::Gix(GixError::from(error))
	}
}

impl From<gix::reference::find::existing::Error> for Error {
	fn from(error: gix::reference::find::existing::Error) -> Self {
		Self::Gix(GixError::from(error))
	}
}

impl From<gix::reference::head_tree_id::Error> for Error {
	fn from(error: gix::reference::head_tree_id::Error) -> Self {
		Self::Gix(GixError::from(error))
	}
}

impl From<gix::reference::iter::Error> for Error {
	fn from(error: gix::reference::iter::Error) -> Self {
		Self::Gix(GixError::from(error))
	}
}

impl From<gix::reference::iter::init::Error> for Error {
	fn from(error: gix::reference::iter::init::Error) -> Self {
		Self::Gix(GixError::from(error))
	}
}

impl From<gix::revision::walk::Error> for Error {
	fn from(error: gix::revision::walk::Error) -> Self {
		Self::Gix(GixError::from(error))
	}
}

impl From<gix::status::Error> for GixError {
	fn from(error: gix::status::Error) -> Self {
		Self::Status(Box::new(error))
	}
}

impl From<gix::status::Error> for Error {
	fn from(error: gix::status::Error) -> Self {
		Self::Gix(GixError::from(error))
	}
}

impl From<gix::status::iter::Error> for GixError {
	fn from(error: gix::status::iter::Error) -> Self {
		Self::StatusIter(Box::new(error))
	}
}

impl From<gix::status::iter::Error> for Error {
	fn from(error: gix::status::iter::Error) -> Self {
		Self::Gix(GixError::from(error))
	}
}

impl From<gix::status::into_iter::Error> for GixError {
	fn from(error: gix::status::into_iter::Error) -> Self {
		Self::StatusIntoIter(Box::new(error))
	}
}

impl From<gix::status::into_iter::Error> for Error {
	fn from(error: gix::status::into_iter::Error) -> Self {
		Self::Gix(GixError::from(error))
	}
}

impl From<gix::status::index_worktree::Error> for GixError {
	fn from(error: gix::status::index_worktree::Error) -> Self {
		Self::StatusIndexWorktree(Box::new(error))
	}
}

impl From<gix::status::index_worktree::Error> for Error {
	fn from(error: gix::status::index_worktree::Error) -> Self {
		Self::Gix(GixError::from(error))
	}
}

impl From<gix::status::tree_index::Error> for GixError {
	fn from(error: gix::status::tree_index::Error) -> Self {
		Self::StatusTreeIndex(Box::new(error))
	}
}

impl From<gix::status::tree_index::Error> for Error {
	fn from(error: gix::status::tree_index::Error) -> Self {
		Self::Gix(GixError::from(error))
	}
}

impl From<gix::worktree::open_index::Error> for GixError {
	fn from(error: gix::worktree::open_index::Error) -> Self {
		Self::WorktreeOpenIndex(Box::new(error))
	}
}

impl From<gix::worktree::open_index::Error> for Error {
	fn from(error: gix::worktree::open_index::Error) -> Self {
		Self::Gix(GixError::from(error))
	}
}
