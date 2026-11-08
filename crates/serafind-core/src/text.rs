//! Text comparison rules shared by every search: accents are ignored, case is "smart".
//!
//! Smart case (as in ripgrep): a needle written in lower case matches any case; a needle with at
//! least one upper-case letter matches the case exactly. Accents are always ignored, both ways:
//! `ete` finds `Été` and `été` finds `ete`.

use nucleo_matcher::chars;

/// How a needle compares with the text it is searched in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseMatching {
    /// Upper and lower case are the same letter.
    Ignore,
    /// Upper and lower case differ.
    Respect,
}

impl CaseMatching {
    /// Smart case: respect case only when the needle has an upper-case letter.
    #[must_use]
    pub fn smart(needle: &str) -> Self {
        if needle.chars().any(chars::is_upper_case) {
            Self::Respect
        } else {
            Self::Ignore
        }
    }
}

/// The text as it is compared: accents removed, and lower case when case is ignored.
#[must_use]
pub fn fold(text: &str, case: CaseMatching) -> String {
    text.chars().map(|c| fold_char(c, case)).collect()
}

/// One character as it is compared (see [`fold`]).
#[must_use]
pub fn fold_char(c: char, case: CaseMatching) -> char {
    let plain = chars::normalize(c);
    match case {
        CaseMatching::Ignore => chars::to_lower_case(plain),
        CaseMatching::Respect => plain,
    }
}

/// A text searched for: folded once, then compared with many haystacks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Needle {
    folded: String,
    case: CaseMatching,
}

impl Needle {
    /// Prepares `text` with smart case.
    #[must_use]
    pub fn new(text: &str) -> Self {
        let case = CaseMatching::smart(text);
        Self {
            folded: fold(text, case),
            case,
        }
    }

    /// The needle as compared (accents removed, lower case when case is ignored).
    #[must_use]
    pub fn folded(&self) -> &str {
        &self.folded
    }

    /// How case is compared for this needle.
    #[must_use]
    pub fn case(&self) -> CaseMatching {
        self.case
    }

    /// True when `haystack` contains the needle.
    #[must_use]
    pub fn is_in(&self, haystack: &str) -> bool {
        fold(haystack, self.case).contains(&self.folded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rstest::rstest;

    #[rstest]
    #[case("ete", "Été 2026", true)]
    #[case("été", "ete", true)]
    #[case("facture", "Facturé-EDF.pdf", true)]
    #[case("garcon", "Garçon.jpg", true)]
    #[case("Rapport", "rapport.odt", false)]
    #[case("Rapport", "Rapport.odt", true)]
    #[case("Éte", "Ete", true)]
    #[case("rapport", "RAPPORT.ODT", true)]
    #[case("xyz", "rapport.odt", false)]
    fn needles_use_smart_case_and_ignore_accents(
        #[case] needle: &str,
        #[case] haystack: &str,
        #[case] expected: bool,
    ) {
        assert_eq!(Needle::new(needle).is_in(haystack), expected);
    }

    #[test]
    fn upper_case_needle_respects_case() {
        assert_eq!(CaseMatching::smart("Report"), CaseMatching::Respect);
        assert_eq!(CaseMatching::smart("report"), CaseMatching::Ignore);
        assert_eq!(CaseMatching::smart("123"), CaseMatching::Ignore);
    }

    proptest! {
        #[test]
        fn a_text_always_contains_itself(text in "\\PC{0,30}") {
            prop_assert!(Needle::new(&text).is_in(&text));
        }

        #[test]
        fn lower_case_needles_find_any_case(text in "[a-zA-Zàéèçù]{1,20}") {
            let needle = Needle::new(&text.to_lowercase());
            prop_assert!(needle.is_in(&text.to_uppercase()));
        }
    }
}
