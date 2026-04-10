/*!
The tabs module contains a struct for each of the tabs visible in the
ui:

- [`Status`]: Stage changes, push, pull
- [`Revlog`]: Revision log (think git log)
- [`FilesTab`]: See content of any file at HEAD. Blame
- [`Stashing`]: Managing one stash
- [`StashList`]: Managing all stashes

Many of the tabs can expand to show more details. This is done via
Enter or right-arrow. To close again, press ESC.
*/

mod files;
mod revlog;
mod stashing;
mod stashlist;
mod status;

pub use files::FilesTab;
pub use revlog::Revlog;
pub use stashing::{Stashing, StashingOptions};
pub use stashlist::StashList;
pub use status::Status;
