//! Core of Serafind: what a search line means and how entries are matched.
//!
//! This crate has no access to the system (no disk, no window): everything here is pure
//! and can be tested in isolation.

pub mod matching;
mod mode;
pub mod syntax;
pub mod text;

pub use matching::{Candidate, EntryKind, MatchRank, Matcher, Partial, Ranked, Score};
pub use mode::SearchMode;
pub use syntax::{ParseContext, Part, PartError, PartKind, Query, parse};
