use std::{num::TryFromIntError, path::PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
	#[error("InvalidPath: `{0}`")]
	InvalidPath(PathBuf),

	#[error("TryFromInt error:{0}")]
	IntConversion(#[from] TryFromIntError),
}

pub type Result<T> = std::result::Result<T, Error>;
