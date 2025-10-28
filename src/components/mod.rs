/*!
Components are the visible building blocks in gitui.

They have a state, handle events, and render to the terminal:

* Some are full screen. That would be all the [`tabs`](super::tabs).
* Some look like panels, eg [`CommitDetailsComponent`]
* Some overlap others. They are collected in module [`popups`](super::popups)
* Some are decorations, eg [`HorizontalScroll`](utils::scroll_horizontal::HorizontalScroll).

Components can be reused.
For example, [`CommitList`] is used in both tab "revlog" and tab "stashlist".


## Composition

In gitui, composition is driven by code. This means each component must
have code that explicitly forwards component function calls like draw,
commands and event to the components it is composed of.

Other systems use composition by data: They provide a generic data structure
that reflects the visual hierarchy, and uses it at runtime to
determine which code should be executed. This is not how gitui works.

## Traits

There are two traits defined here:
* [`Component`] handles events from the user,
* [`DrawableComponent`] renders to the terminal.

In the current codebase these are always implemented together, and it probably
makes more sense to merge them some time in the future.
It is a little strange that you implement `draw()` on a `DrawableComponent`,
but have function `hide()` from trait Component which does not know how
to `draw()`.
*/

mod changes;
mod command;
mod commit_details;
mod commitlist;
mod cred;
mod diff;
mod revision_files;
mod status_tree;
mod syntax_text;
mod textinput;
mod utils;

pub use self::status_tree::StatusTreeComponent;
pub use changes::ChangesComponent;
pub use command::{CommandInfo, CommandText};
pub use commit_details::CommitDetailsComponent;
pub use commitlist::CommitList;
pub use cred::CredComponent;
pub use diff::DiffComponent;
pub use revision_files::RevisionFilesComponent;
pub use syntax_text::SyntaxTextComponent;
pub use textinput::{InputType, TextInputComponent};
pub use utils::{
	filetree::FileTreeItemKind, logitems::ItemBatch,
	scroll_vertical::VerticalScroll, string_width_align,
	time_to_string,
};

use crate::ui::style::Theme;
use anyhow::Result;
use crossterm::event::Event;
use ratatui::{
	layout::{Alignment, Rect},
	text::{Span, Text},
	widgets::{Block, Borders, Paragraph},
	Frame,
};

/// creates accessors for a list of components
///
/// allows generating code to make sure
/// we always enumerate all components in both getter functions
#[macro_export]
macro_rules! accessors {
    ($self:ident, [$($element:ident),+]) => {
        fn components(& $self) -> Vec<&dyn Component> {
            vec![
                $(&$self.$element,)+
            ]
        }

        fn components_mut(&mut $self) -> Vec<&mut dyn Component> {
            vec![
                $(&mut $self.$element,)+
            ]
        }
    };
}

/// creates a function to determine if any popup is visible
#[macro_export]
macro_rules! any_popup_visible {
    ($self:ident, [$($element:ident),+]) => {
        fn any_popup_visible(& $self) -> bool{
            ($($self.$element.is_visible()) || +)
        }
    };
}

/// creates the draw popup function
#[macro_export]
macro_rules! draw_popups {
    ($self:ident, [$($element:ident),+]) => {
        fn draw_popups(& $self, mut f: &mut Frame) -> Result<()>{
            //TODO: move the layout part out and feed it into `draw_popups`
            let size = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                [
                    Constraint::Min(1),
                    Constraint::Length($self.cmdbar.borrow().height()),
                ]
                .as_ref(),
            )
            .split(f.area())[0];

            ($($self.$element.draw(&mut f, size)?) , +);

            return Ok(());
        }
    };
}

/// simply calls
/// `any_popup_visible`!() and `draw_popups`!() macros
#[macro_export]
macro_rules! setup_popups {
    ($self:ident, [$($element:ident),+]) => {
        $crate::any_popup_visible!($self, [$($element),+]);
        $crate::draw_popups!($self, [ $($element),+ ]);
    };
}

/// returns `true` if event was consumed
pub fn event_pump(
	ev: &Event,
	components: &mut [&mut dyn Component],
) -> Result<EventState> {
	for c in components {
		if c.event(ev)?.is_consumed() {
			return Ok(EventState::Consumed);
		}
	}

	Ok(EventState::NotConsumed)
}

/// helper fn to simplify delegating command
/// gathering down into child components
/// see `event_pump`,`accessors`
pub fn command_pump(
	out: &mut Vec<CommandInfo>,
	force_all: bool,
	components: &[&dyn Component],
) {
	for c in components {
		if c.commands(out, force_all) != CommandBlocking::PassingOn
			&& !force_all
		{
			break;
		}
	}
}

#[derive(Copy, Clone)]
pub enum ScrollType {
	Up,
	Down,
	Home,
	End,
	PageUp,
	PageDown,
}

#[derive(Copy, Clone)]
pub enum HorizontalScrollType {
	Left,
	Right,
}

#[derive(Copy, Clone)]
pub enum Direction {
	Up,
	Down,
}

///
#[derive(PartialEq, Eq)]
pub enum CommandBlocking {
	Blocking,
	PassingOn,
}

///
pub fn visibility_blocking<T: Component>(
	comp: &T,
) -> CommandBlocking {
	if comp.is_visible() {
		CommandBlocking::Blocking
	} else {
		CommandBlocking::PassingOn
	}
}

///
pub trait DrawableComponent {
	///
	fn draw(&self, f: &mut Frame, rect: Rect) -> Result<()>;
}

///
#[derive(PartialEq, Eq)]
pub enum EventState {
	Consumed,
	NotConsumed,
}

#[derive(Copy, Clone)]
pub enum FuzzyFinderTarget {
	Branches,
	Files,
}

impl EventState {
	pub fn is_consumed(&self) -> bool {
		*self == Self::Consumed
	}
}

impl From<bool> for EventState {
	fn from(consumed: bool) -> Self {
		if consumed {
			Self::Consumed
		} else {
			Self::NotConsumed
		}
	}
}

/// base component trait
pub trait Component {
	///
	fn commands(
		&self,
		out: &mut Vec<CommandInfo>,
		force_all: bool,
	) -> CommandBlocking;

	///
	fn event(&mut self, ev: &Event) -> Result<EventState>;

	///
	fn focused(&self) -> bool {
		false
	}
	/// focus/unfocus this component depending on param
	fn focus(&mut self, _focus: bool) {}
	///
	fn is_visible(&self) -> bool {
		true
	}
	///
	fn hide(&mut self) {}
	///
	fn show(&mut self) -> Result<()> {
		Ok(())
	}

	///
	fn toggle_visible(&mut self) -> Result<()> {
		if self.is_visible() {
			self.hide();
			Ok(())
		} else {
			self.show()
		}
	}
}

fn dialog_paragraph<'a>(
	title: &'a str,
	content: Text<'a>,
	theme: &Theme,
	focused: bool,
) -> Paragraph<'a> {
	Paragraph::new(content)
		.block(
			Block::default()
				.title(Span::styled(title, theme.title(focused)))
				.borders(Borders::ALL)
				.border_style(theme.block(focused)),
		)
		.alignment(Alignment::Left)
}
