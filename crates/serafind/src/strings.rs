//! Every text shown to the user, in one place.
//!
//! The bar is in English for now. To translate it later, replace [`Strings::ENGLISH`] by a table
//! chosen at start (built in or read from a file): the rest of the code only reads [`Strings`].

use serafind_core::{PartError, SearchMode};

/// The texts of the user interface.
#[derive(Debug, Clone, Copy)]
pub struct Strings {
    /// Placeholder of the empty input.
    pub placeholder: &'static str,
    /// Hint line under the empty input.
    pub filters_hint: &'static str,
    /// Footer key hints.
    pub key_hints: &'static str,
    /// Footer while the index is being built.
    pub indexing: &'static str,
    /// No result.
    pub no_result: &'static str,
    /// Results found by the fuzzy fallback.
    pub fuzzy_note: &'static str,
    /// `in:` names a folder outside the index.
    pub outside_index: &'static str,
    /// The resident process does not answer.
    pub unreachable: &'static str,
    /// The path was copied.
    pub path_copied: &'static str,
    /// The name was copied.
    pub name_copied: &'static str,
    /// Prefix of a settings error.
    pub settings_error: &'static str,
    /// Prefix of a global shortcut problem.
    pub no_shortcut: &'static str,
    /// Footer counter for one result.
    pub one_result: &'static str,
    /// Footer counter, `{total}` replaced.
    pub results: &'static str,
    /// Footer counter when not all are shown, `{shown}` and `{total}` replaced.
    pub results_of: &'static str,
    /// Mode names, in tab order (All, Apps, Folders, Files, Content).
    pub modes: [&'static str; 5],
    /// Label of an extension chip.
    pub chip_extension: &'static str,
    /// Label of a folder chip.
    pub chip_folder: &'static str,
    /// Label of a content chip.
    pub chip_content: &'static str,
    /// Label of an excluded part.
    pub chip_not: &'static str,
    /// Accessibility label of the input.
    pub input_label: &'static str,
    /// Accessibility label of the mode selector.
    pub mode_label: &'static str,
    /// Notification area: tooltip of the icon.
    pub tray_tooltip: &'static str,
    /// Notification area menu: open the bar.
    pub tray_open: &'static str,
    /// Notification area menu: index again.
    pub tray_reindex: &'static str,
    /// Notification area menu: open the settings folder.
    pub tray_settings: &'static str,
    /// Notification area menu: stop Serafind.
    pub tray_quit: &'static str,
}

impl Strings {
    /// The English texts.
    pub const ENGLISH: Self = Self {
        placeholder: "Search…",
        filters_hint: "a: apps · d: folders · f: files · c: content · ext:pdf · in:~/folder · -word",
        key_hints: "↵ open   Ctrl+↵ show in folder   Ctrl+C copy path",
        indexing: "Indexing…",
        no_result: "No result",
        fuzzy_note: "approximate",
        outside_index: "This folder is not indexed",
        unreachable: "Serafind is not running",
        path_copied: "Path copied",
        name_copied: "Name copied",
        settings_error: "Settings not applied",
        no_shortcut: "No global shortcut",
        one_result: "1 result",
        results: "{total} results",
        results_of: "{shown} of {total}",
        modes: ["All", "Apps", "Folders", "Files", "Content"],
        chip_extension: "Ext",
        chip_folder: "In",
        chip_content: "Text",
        chip_not: "Not",
        input_label: "Search",
        mode_label: "Mode",
        tray_tooltip: "Serafind: click to search",
        tray_open: "Open Serafind",
        tray_reindex: "Index again",
        tray_settings: "Settings",
        tray_quit: "Quit Serafind",
    };

    /// The name of a mode.
    #[must_use]
    pub fn mode(&self, mode: SearchMode) -> &'static str {
        let index = SearchMode::ALL.iter().position(|&each| each == mode).unwrap_or(0);
        self.modes[index]
    }

    /// The footer counter: `3 results`, `50 of 1,234`.
    #[must_use]
    pub fn count(&self, shown: usize, total: usize) -> String {
        match (shown, total) {
            (_, 1) => self.one_result.to_owned(),
            (shown, total) if shown < total => self
                .results_of
                .replace("{shown}", &group(shown))
                .replace("{total}", &group(total)),
            (_, total) => self.results.replace("{total}", &group(total)),
        }
    }

    /// Why a part of the search is not used, for the chip tooltip.
    #[must_use]
    #[expect(clippy::unused_self, reason = "a translated table will map each error itself")]
    pub fn part_error(&self, error: &PartError) -> String {
        // The core describes its errors in English already; a translation would map each case.
        error.to_string()
    }
}

/// `1234567` → `1,234,567`.
fn group(number: usize) -> String {
    let digits = number.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case(0, 0, "0 results")]
    #[case(1, 1, "1 result")]
    #[case(3, 3, "3 results")]
    #[case(50, 1234, "50 of 1,234")]
    #[case(50, 1_000_000, "50 of 1,000,000")]
    fn counts(#[case] shown: usize, #[case] total: usize, #[case] text: &str) {
        assert_eq!(Strings::ENGLISH.count(shown, total), text);
    }

    #[test]
    fn every_mode_has_a_name() {
        for mode in SearchMode::ALL {
            assert!(!Strings::ENGLISH.mode(mode).is_empty());
        }
        assert_eq!(Strings::ENGLISH.mode(SearchMode::Content), "Content");
    }
}
