use thiserror::Error;

/// crate specific error type
#[derive(Error, Debug)]
pub enum HooksError {
	#[error("git error:{0}")]
	Git(#[from] git2::Error),

	#[error("io error:{0}")]
	Io(#[from] std::io::Error),

	#[error("path string conversion error")]
	PathToString,

	#[error("shellexpand error:{0}")]
	ShellExpand(#[from] shellexpand::LookupError<std::env::VarError>),

	#[error("hook process terminated by signal without exit code")]
	NoExitCode,
}

/// crate specific `Result` type
pub type Result<T> = std::result::Result<T, HooksError>;
