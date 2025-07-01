use std::{
	num::TryFromIntError, path::StripPrefixError,
	string::FromUtf8Error,
};
use thiserror::Error;

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
	#[error("gix::discover error: {0}")]
	GixDiscover(#[from] Box<gix::discover::Error>),

	///
	#[error("gix::reference::find::existing error: {0}")]
	GixReferenceFindExisting(
		#[from] gix::reference::find::existing::Error,
	),

	///
	#[error("gix::head::peel::to_commit error: {0}")]
	GixHeadPeelToCommit(#[from] gix::head::peel::to_commit::Error),

	///
	#[error("gix::revision::walk error: {0}")]
	GixRevisionWalk(#[from] gix::revision::walk::Error),

	///
	#[error("gix::objs::decode::Error error: {0}")]
	GixObjsDecode(#[from] gix::objs::decode::Error),

	///
	#[error("gix::object::find::existing::with_conversion::Error error: {0}")]
	GixObjectFindExistingWithConversionError(
		#[from] gix::object::find::existing::with_conversion::Error,
	),

	///
	#[error("gix::pathspec::init::Error error: {0}")]
	GixPathspecInit(#[from] Box<gix::pathspec::init::Error>),

	///
	#[error("gix::reference::head_tree_id::Error error: {0}")]
	GixReferenceHeadTreeId(
		#[from] gix::reference::head_tree_id::Error,
	),

	///
	#[error("gix::status::Error error: {0}")]
	GixStatus(#[from] Box<gix::status::Error>),

	///
	#[error("gix::status::iter::Error error: {0}")]
	GixStatusIter(#[from] Box<gix::status::iter::Error>),

	///
	#[error("gix::status::into_iter::Error error: {0}")]
	GixStatusIntoIter(#[from] Box<gix::status::into_iter::Error>),

	///
	#[error("gix::status::index_worktree::Error error: {0}")]
	GixStatusIndexWorktree(
		#[from] Box<gix::status::index_worktree::Error>,
	),

	///
	#[error("gix::status::tree_index::Error error: {0}")]
	GixStatusTreeIndex(#[from] Box<gix::status::tree_index::Error>),

	///
	#[error("gix::worktree::open_index::Error error: {0}")]
	GixWorktreeOpenIndex(
		#[from] Box<gix::worktree::open_index::Error>,
	),

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

impl From<gix::discover::Error> for Error {
	fn from(error: gix::discover::Error) -> Self {
		Self::GixDiscover(Box::new(error))
	}
}

impl From<gix::pathspec::init::Error> for Error {
	fn from(error: gix::pathspec::init::Error) -> Self {
		Self::GixPathspecInit(Box::new(error))
	}
}

impl From<gix::status::Error> for Error {
	fn from(error: gix::status::Error) -> Self {
		Self::GixStatus(Box::new(error))
	}
}

impl From<gix::status::iter::Error> for Error {
	fn from(error: gix::status::iter::Error) -> Self {
		Self::GixStatusIter(Box::new(error))
	}
}

impl From<gix::status::into_iter::Error> for Error {
	fn from(error: gix::status::into_iter::Error) -> Self {
		Self::GixStatusIntoIter(Box::new(error))
	}
}

impl From<gix::status::index_worktree::Error> for Error {
	fn from(error: gix::status::index_worktree::Error) -> Self {
		Self::GixStatusIndexWorktree(Box::new(error))
	}
}

impl From<gix::status::tree_index::Error> for Error {
	fn from(error: gix::status::tree_index::Error) -> Self {
		Self::GixStatusTreeIndex(Box::new(error))
	}
}

impl From<gix::worktree::open_index::Error> for Error {
	fn from(error: gix::worktree::open_index::Error) -> Self {
		Self::GixWorktreeOpenIndex(Box::new(error))
	}
}
