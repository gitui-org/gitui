//! simple macro to insert a scope based runtime measure that logs the result

#![forbid(unsafe_code)]
#![deny(unused_imports)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::perf)]

use std::time::Instant;

pub struct ScopeTimeLog<'a> {
	title: &'a str,
	mod_path: &'a str,
	file: &'a str,
	line: u32,
	time: Instant,
}

impl<'a> ScopeTimeLog<'a> {
	pub fn new(
		mod_path: &'a str,
		title: &'a str,
		file: &'a str,
		line: u32,
	) -> Self {
		Self {
			title,
			mod_path,
			file,
			line,
			time: Instant::now(),
		}
	}
}

impl Drop for ScopeTimeLog<'_> {
	fn drop(&mut self) {
		log::trace!(
			"scopetime: {:?} ms [{}::{}] @{}:{}",
			self.time.elapsed().as_millis(),
			self.mod_path,
			self.title,
			self.file,
			self.line,
		);
	}
}

/// measures runtime of scope and prints it into log
#[cfg(feature = "enabled")]
#[macro_export]
macro_rules! scope_time {
	($target:literal) => {
		#[allow(unused_variables)]
		let time = $crate::ScopeTimeLog::new(
			module_path!(),
			$target,
			file!(),
			line!(),
		);
	};
}

#[doc(hidden)]
#[cfg(not(feature = "enabled"))]
#[macro_export]
macro_rules! scope_time {
	($target:literal) => {};
}
