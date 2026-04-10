use crate::strings::order;

///
#[derive(Clone, PartialEq, PartialOrd, Ord, Eq)]
pub struct CommandText {
	///
	pub name: String,
	///
	pub desc: &'static str,
	///
	pub group: &'static str,
	///
	pub hide_help: bool,
}

impl CommandText {
	///
	pub const fn new(
		name: String,
		desc: &'static str,
		group: &'static str,
	) -> Self {
		Self {
			name,
			desc,
			group,
			hide_help: false,
		}
	}
	///
	pub const fn hide_help(self) -> Self {
		let mut tmp = self;
		tmp.hide_help = true;
		tmp
	}
}

///
pub struct CommandInfo {
	///
	pub text: CommandText,
	/// available but not active in the context
	pub enabled: bool,
	/// will show up in the quick bar
	pub quick_bar: bool,

	/// available in current app state
	pub available: bool,
	/// used to order commands in quickbar
	pub order: i8,
}

impl CommandInfo {
	///
	pub const fn new(
		text: CommandText,
		enabled: bool,
		available: bool,
	) -> Self {
		Self {
			text,
			enabled,
			quick_bar: true,
			available,
			order: order::AVERAGE,
		}
	}

	///
	pub const fn order(self, order: i8) -> Self {
		let mut res = self;
		res.order = order;
		res
	}

	///
	pub const fn hidden(self) -> Self {
		let mut res = self;
		res.quick_bar = false;
		res
	}

	///
	pub const fn show_in_quickbar(&self) -> bool {
		self.quick_bar && self.available
	}
}
