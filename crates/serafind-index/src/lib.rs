//! In-memory file index of Serafind.
//!
//! Holds every folder and file under the indexed roots (the home directory by default, minus the
//! excluded folders), built by a parallel walk ([`build`]) and kept up to date by watching the
//! filesystem ([`LiveIndex`]).

mod exclusions;
mod index;
mod walk;
mod watch;

pub use exclusions::{DEFAULT_NAMES, Exclusions};
pub use index::{EntryId, Hit, Index, Kind, SearchResults};
pub use walk::{WalkReport, add_subtree, add_tree, build};
pub use watch::{DEFAULT_DELAY, LiveIndex, WatchStatus};
