//! Core of Serafind: what a search line means and how entries are matched.
//!
//! This crate has no access to the system (no disk, no window): everything here is pure
//! and can be tested in isolation.

mod mode;

pub use mode::SearchMode;
