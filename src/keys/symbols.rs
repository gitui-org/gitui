use std::{fs::File, io::Read, path::PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct KeySymbols {
	pub enter: String,
	pub left: String,
	pub right: String,
	pub up: String,
	pub down: String,
	pub backspace: String,
	pub home: String,
	pub end: String,
	pub page_up: String,
	pub page_down: String,
	pub tab: String,
	pub back_tab: String,
	pub delete: String,
	pub insert: String,
	pub esc: String,
	pub control: String,
	pub shift: String,
	pub alt: String,
}

#[rustfmt::skip]
impl Default for KeySymbols {
	fn default() -> Self {
		Self {
			enter: "\u{23ce}".into(),     //⏎
			left: "\u{2190}".into(),      //←
			right: "\u{2192}".into(),     //→
			up: "\u{2191}".into(),        //↑
			down: "\u{2193}".into(),      //↓
			backspace: "\u{232b}".into(), //⌫
			home: "\u{2912}".into(),      //⤒
			end: "\u{2913}".into(),       //⤓
			page_up: "\u{21de}".into(),   //⇞
			page_down: "\u{21df}".into(), //⇟
			tab: "\u{21e5}".into(),       //⇥
			back_tab: "\u{21e4}".into(),  //⇤
			delete: "\u{2326}".into(),    //⌦
			insert: "\u{2380}".into(),    //⎀
			esc: "\u{238b}".into(),       //⎋
			control: "^".into(),
			shift: "\u{21e7}".into(),     //⇧
			alt: "\u{2325}".into(),       //⌥
		}
	}
}

impl KeySymbols {
	pub fn init(file: PathBuf) -> Self {
		if file.exists() {
			let file =
				KeySymbolsFile::read_file(file).unwrap_or_default();
			file.get_symbols()
		} else {
			Self::default()
		}
	}
}

//TODO: this could auto generated in a proc macro
#[derive(Serialize, Deserialize, Default)]
pub struct KeySymbolsFile {
	pub enter: Option<String>,
	pub left: Option<String>,
	pub right: Option<String>,
	pub up: Option<String>,
	pub down: Option<String>,
	pub backspace: Option<String>,
	pub home: Option<String>,
	pub end: Option<String>,
	pub page_up: Option<String>,
	pub page_down: Option<String>,
	pub tab: Option<String>,
	pub back_tab: Option<String>,
	pub delete: Option<String>,
	pub insert: Option<String>,
	pub esc: Option<String>,
	pub control: Option<String>,
	pub shift: Option<String>,
	pub alt: Option<String>,
}

impl KeySymbolsFile {
	fn read_file(config_file: PathBuf) -> Result<Self> {
		let mut f = File::open(config_file)?;
		let mut buffer = Vec::new();
		f.read_to_end(&mut buffer)?;
		Ok(ron::de::from_bytes(&buffer)?)
	}

	pub fn get_symbols(self) -> KeySymbols {
		let default = KeySymbols::default();

		KeySymbols {
			enter: self.enter.unwrap_or(default.enter),
			left: self.left.unwrap_or(default.left),
			right: self.right.unwrap_or(default.right),
			up: self.up.unwrap_or(default.up),
			down: self.down.unwrap_or(default.down),
			backspace: self.backspace.unwrap_or(default.backspace),
			home: self.home.unwrap_or(default.home),
			end: self.end.unwrap_or(default.end),
			page_up: self.page_up.unwrap_or(default.page_up),
			page_down: self.page_down.unwrap_or(default.page_down),
			tab: self.tab.unwrap_or(default.tab),
			back_tab: self.back_tab.unwrap_or(default.back_tab),
			delete: self.delete.unwrap_or(default.delete),
			insert: self.insert.unwrap_or(default.insert),
			esc: self.esc.unwrap_or(default.esc),
			control: self.control.unwrap_or(default.control),
			shift: self.shift.unwrap_or(default.shift),
			alt: self.alt.unwrap_or(default.alt),
		}
	}
}
