//! Search modes: what kind of result the bar is looking for.

/// What the search bar is looking for. `All` mixes apps first, then folders and files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SearchMode {
    /// Apps, folders and files together (apps ranked first).
    #[default]
    All,
    /// Installed applications only.
    Apps,
    /// Folders only.
    Folders,
    /// Files only.
    Files,
    /// Text inside files.
    Content,
}

impl SearchMode {
    /// Every mode, in the order of the tabs (Ctrl+1 is the first one).
    pub const ALL: [Self; 5] = [Self::All, Self::Apps, Self::Folders, Self::Files, Self::Content];

    /// The mode of the next tab (Tab key), wrapping around after the last one.
    #[must_use]
    pub fn next(self) -> Self {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }

    /// The mode of the previous tab (Shift+Tab), wrapping around before the first one.
    #[must_use]
    pub fn previous(self) -> Self {
        Self::ALL[(self.index() + Self::ALL.len() - 1) % Self::ALL.len()]
    }

    /// The mode of a tab number, from 1 (Ctrl+1) to 5; `None` outside that range.
    #[must_use]
    pub fn from_tab_number(number: usize) -> Option<Self> {
        number
            .checked_sub(1)
            .and_then(|index| Self::ALL.get(index).copied())
    }

    fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|&mode| mode == self)
            .expect("every mode is listed in ALL")
    }
}

#[cfg(test)]
mod tests {
    use super::SearchMode::{self, All, Apps, Content, Files, Folders};
    use proptest::prelude::*;
    use rstest::rstest;

    #[rstest]
    #[case(All, Apps)]
    #[case(Apps, Folders)]
    #[case(Folders, Files)]
    #[case(Files, Content)]
    #[case(Content, All)]
    fn next_goes_to_the_following_tab(#[case] mode: SearchMode, #[case] expected: SearchMode) {
        assert_eq!(mode.next(), expected);
    }

    #[rstest]
    #[case(0, None)]
    #[case(1, Some(All))]
    #[case(5, Some(Content))]
    #[case(6, None)]
    fn tab_numbers_start_at_one(#[case] number: usize, #[case] expected: Option<SearchMode>) {
        assert_eq!(SearchMode::from_tab_number(number), expected);
    }

    #[test]
    fn default_mode_is_all() {
        assert_eq!(SearchMode::default(), All);
    }

    #[test]
    fn tab_order_snapshot() {
        insta::assert_debug_snapshot!(SearchMode::ALL);
    }

    proptest! {
        #[test]
        fn previous_undoes_next(index in 0..SearchMode::ALL.len()) {
            let mode = SearchMode::ALL[index];
            prop_assert_eq!(mode.next().previous(), mode);
        }

        #[test]
        fn pressing_tab_once_per_mode_comes_back(index in 0..SearchMode::ALL.len()) {
            let mode = SearchMode::ALL[index];
            let cycled = (0..SearchMode::ALL.len()).fold(mode, |current, _| current.next());
            prop_assert_eq!(cycled, mode);
        }
    }
}
