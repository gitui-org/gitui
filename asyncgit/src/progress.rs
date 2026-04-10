//!

use easy_cast::{Conv, ConvFloat};
use std::cmp;

///
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct ProgressPercent {
	/// percent 0..100
	pub progress: u8,
}

impl ProgressPercent {
	///
	pub fn new(current: usize, total: usize) -> Self {
		let total = f64::conv(cmp::max(current, total));
		let progress = f64::conv(current) / total * 100.0;
		let progress = u8::try_conv_nearest(progress).unwrap_or(100);
		Self { progress }
	}
	///
	pub const fn empty() -> Self {
		Self { progress: 0 }
	}
	///
	pub const fn full() -> Self {
		Self { progress: 100 }
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_progress_zero_total() {
		let prog = ProgressPercent::new(1, 0);

		assert_eq!(prog.progress, 100);
	}

	#[test]
	fn test_progress_zero_all() {
		let prog = ProgressPercent::new(0, 0);
		assert_eq!(prog.progress, 100);
	}

	#[test]
	fn test_progress_rounding() {
		let prog = ProgressPercent::new(2, 10);

		assert_eq!(prog.progress, 20);
	}
}
