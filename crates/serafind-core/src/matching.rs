//! Which entries a query keeps, and in which order.
//!
//! Strict matching first (as in PowerHive): every word must be contained in the name (a word with
//! `*` or `?` is a pattern for the whole name), no excluded word may be, the extension must be one
//! of `ext:`, and the kind must fit the mode. When no entry matches strictly, the words are matched
//! fuzzily instead (`fctr` finds `facture`); the other criteria still apply.
//!
//! Ranking, best first: in All mode apps come before folders and files; then exact name, name
//! starting with the first word, every word starting a word of the name, other matches; then
//! shorter paths, shorter names, and alphabetical order.

use std::cmp::{Ordering, Reverse};

use nucleo_matcher::pattern::{Atom, AtomKind, CaseMatching as FuzzyCase, Normalization};
use nucleo_matcher::{Config, Utf32Str};

use crate::text::{self, CaseMatching, Needle};
use crate::{Query, SearchMode};

/// What an entry is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntryKind {
    /// An installed application.
    App,
    /// A folder.
    Folder,
    /// A file.
    File,
}

/// An entry offered to the matcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Candidate<'a> {
    /// The name shown and matched: file or folder name, or the app name.
    pub name: &'a str,
    /// The full path (or app identifier), used for ranking only.
    pub path: &'a str,
    /// What the entry is.
    pub kind: EntryKind,
}

/// How well a candidate matches; smaller is better.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Score {
    kind_rank: u8,
    match_rank: MatchRank,
    fuzzy: Reverse<u32>,
    depth: usize,
    name_length: usize,
}

impl Score {
    /// How the words were found in the name.
    #[must_use]
    pub fn match_rank(&self) -> MatchRank {
        self.match_rank
    }
}

/// How the words were found in the name, best first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MatchRank {
    /// The name (or the name without its extension) is the words.
    Exact,
    /// The name starts with the first word.
    NamePrefix,
    /// Every word starts a word of the name (`rap` in `mon-rapport`, `Rep` in `myReport`).
    WordPrefix,
    /// The words are somewhere in the name, or there are no words.
    Contains,
    /// Found by the fuzzy fallback only.
    Fuzzy,
}

/// A query ready to be compared with many candidates.
pub struct Matcher {
    mode: SearchMode,
    words: Vec<Word>,
    excluded_words: Vec<Word>,
    extensions: Vec<String>,
    excluded_extensions: Vec<String>,
    whole: Option<Needle>,
    fuzzy_atoms: Vec<Atom>,
    fuzzy: nucleo_matcher::Matcher,
    buffer: Vec<char>,
}

enum Word {
    Text(Needle),
    Pattern(Glob),
}

impl Matcher {
    /// Prepares `query` for the mode the search runs in.
    #[must_use]
    pub fn new(query: &Query, mode: SearchMode) -> Self {
        let words: Vec<Word> = query.words.iter().map(|word| Word::new(word)).collect();
        let fuzzy_atoms = query
            .words
            .iter()
            .map(|word| word.replace(['*', '?'], ""))
            .filter(|word| !word.is_empty())
            .map(|word| {
                let needle = Needle::new(&word);
                let case = match needle.case() {
                    CaseMatching::Ignore => FuzzyCase::Ignore,
                    CaseMatching::Respect => FuzzyCase::Respect,
                };
                Atom::new(
                    needle.folded(),
                    case,
                    Normalization::Smart,
                    AtomKind::Fuzzy,
                    false,
                )
            })
            .collect();
        let whole = (!query.words.is_empty() && words.iter().all(|word| matches!(word, Word::Text(_))))
            .then(|| Needle::new(&query.words.join(" ")));
        let extension = |extension: &String| text::fold(extension, CaseMatching::Ignore);
        Self {
            mode,
            words,
            excluded_words: query.excluded_words.iter().map(|word| Word::new(word)).collect(),
            extensions: query.extensions.iter().map(extension).collect(),
            excluded_extensions: query.excluded_extensions.iter().map(extension).collect(),
            whole,
            fuzzy_atoms,
            fuzzy: nucleo_matcher::Matcher::new(Config::DEFAULT),
            buffer: Vec::new(),
        }
    }

    /// The score of a candidate that matches strictly; `None` otherwise.
    pub fn strict(&mut self, candidate: &Candidate<'_>) -> Option<Score> {
        if !self.passes_filters(candidate) || !self.words.iter().all(|word| word.is_in(candidate.name)) {
            return None;
        }
        Some(self.score(candidate, self.strict_rank(candidate.name), 0))
    }

    /// The score of a candidate found by fuzzy matching of the words; `None` otherwise.
    pub fn fuzzy(&mut self, candidate: &Candidate<'_>) -> Option<Score> {
        if self.fuzzy_atoms.is_empty() || !self.passes_filters(candidate) {
            return None;
        }
        let haystack = Utf32Str::new(candidate.name, &mut self.buffer);
        let mut total = 0;
        for atom in &self.fuzzy_atoms {
            total += u32::from(atom.score(haystack, &mut self.fuzzy)?);
        }
        Some(self.score(candidate, MatchRank::Fuzzy, total))
    }

    /// The best `limit` candidates: strict matches, or fuzzy ones when nothing matches strictly.
    pub fn best<'a>(
        &mut self,
        candidates: impl IntoIterator<Item = Candidate<'a>>,
        limit: usize,
    ) -> Ranked<'a> {
        let mut strict = Vec::new();
        let mut fuzzy = Vec::new();
        for candidate in candidates {
            if let Some(score) = self.strict(&candidate) {
                strict.push((score, candidate));
            } else if strict.is_empty()
                && let Some(score) = self.fuzzy(&candidate)
            {
                fuzzy.push((score, candidate));
            }
        }
        let is_fuzzy = strict.is_empty() && !fuzzy.is_empty();
        let mut found = if is_fuzzy { fuzzy } else { strict };
        let total = found.len();
        keep_best(&mut found, limit);
        Ranked {
            entries: found,
            total,
            is_fuzzy,
        }
    }

    fn passes_filters(&self, candidate: &Candidate<'_>) -> bool {
        let kind_fits = match self.mode {
            SearchMode::All => true,
            SearchMode::Apps => candidate.kind == EntryKind::App,
            SearchMode::Folders => candidate.kind == EntryKind::Folder,
            SearchMode::Files | SearchMode::Content => candidate.kind == EntryKind::File,
        };
        if !kind_fits || self.excluded_words.iter().any(|word| word.is_in(candidate.name)) {
            return false;
        }
        if self.extensions.is_empty() && self.excluded_extensions.is_empty() {
            return true;
        }
        let extension = extension_of(candidate);
        let has = |list: &[String]| {
            extension
                .as_ref()
                .is_some_and(|extension| list.contains(extension))
        };
        (self.extensions.is_empty() || has(&self.extensions)) && !has(&self.excluded_extensions)
    }

    fn strict_rank(&self, name: &str) -> MatchRank {
        if let Some(whole) = &self.whole {
            let folded = text::fold(name, whole.case());
            let stem = folded.rsplit_once('.').map_or(folded.as_str(), |(stem, _)| stem);
            if folded == whole.folded() || stem == whole.folded() {
                return MatchRank::Exact;
            }
        }
        let mut texts = self.words.iter().filter_map(|word| match word {
            Word::Text(needle) => Some(needle),
            Word::Pattern(_) => None,
        });
        let Some(first) = texts.next() else {
            return MatchRank::Contains;
        };
        if starts_with_needle(name, first) {
            MatchRank::NamePrefix
        } else if starts_a_word(name, first) && texts.all(|needle| starts_a_word(name, needle)) {
            MatchRank::WordPrefix
        } else {
            MatchRank::Contains
        }
    }

    fn score(&self, candidate: &Candidate<'_>, match_rank: MatchRank, fuzzy: u32) -> Score {
        let kind_rank = match (self.mode, candidate.kind) {
            (SearchMode::All, EntryKind::App) => 0,
            _ => 1,
        };
        Score {
            kind_rank,
            match_rank,
            fuzzy: Reverse(fuzzy),
            depth: candidate.path.matches(['/', '\\']).count(),
            name_length: candidate.name.chars().count(),
        }
    }
}

/// The result of [`Matcher::best`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ranked<'a> {
    /// The best candidates, best first.
    pub entries: Vec<(Score, Candidate<'a>)>,
    /// How many candidates matched in all (more than `entries` when the limit was reached).
    pub total: usize,
    /// True when the entries come from the fuzzy fallback.
    pub is_fuzzy: bool,
}

fn keep_best(found: &mut Vec<(Score, Candidate<'_>)>, limit: usize) {
    let order = |a: &(Score, Candidate<'_>), b: &(Score, Candidate<'_>)| -> Ordering {
        a.0.cmp(&b.0)
            .then_with(|| a.1.name.cmp(b.1.name))
            .then_with(|| a.1.path.cmp(b.1.path))
    };
    if limit == 0 {
        found.clear();
        return;
    }
    if found.len() > limit {
        found.select_nth_unstable_by(limit - 1, order);
        found.truncate(limit);
    }
    found.sort_unstable_by(order);
}

fn extension_of(candidate: &Candidate<'_>) -> Option<String> {
    if candidate.kind == EntryKind::App {
        return None;
    }
    // ".bashrc" has no extension, "archive.tar.gz" has "gz".
    let (stem, extension) = candidate.name.rsplit_once('.')?;
    (!stem.is_empty() && !extension.is_empty()).then(|| text::fold(extension, CaseMatching::Ignore))
}

fn starts_with_needle(name: &str, needle: &Needle) -> bool {
    text::fold(name, needle.case()).starts_with(needle.folded())
}

/// True when the needle is found at the start of a word of the name: after the start, a
/// non-alphanumeric character, a lower-to-upper case change, or a letter-digit change.
fn starts_a_word(name: &str, needle: &Needle) -> bool {
    let chars: Vec<char> = name.chars().collect();
    let folded: Vec<char> = chars.iter().map(|&c| text::fold_char(c, needle.case())).collect();
    let target: Vec<char> = needle.folded().chars().collect();
    if target.is_empty() || target.len() > folded.len() {
        return target.is_empty();
    }
    (0..=folded.len() - target.len())
        .filter(|&index| is_word_start(&chars, index))
        .any(|index| folded[index..index + target.len()] == target[..])
}

fn is_word_start(chars: &[char], index: usize) -> bool {
    let Some(previous) = index.checked_sub(1).map(|before| chars[before]) else {
        return true;
    };
    let current = chars[index];
    !previous.is_alphanumeric()
        || (previous.is_lowercase() && current.is_uppercase())
        || (previous.is_alphabetic() && current.is_numeric())
        || (previous.is_numeric() && current.is_alphabetic())
}

impl Word {
    fn new(word: &str) -> Self {
        if word.contains(['*', '?']) {
            Self::Pattern(Glob::new(word))
        } else {
            Self::Text(Needle::new(word))
        }
    }

    fn is_in(&self, name: &str) -> bool {
        match self {
            Self::Text(needle) => needle.is_in(name),
            Self::Pattern(glob) => glob.matches(name),
        }
    }
}

/// A `*` / `?` pattern matched against the whole name, with the same case and accent rules as words.
struct Glob {
    pattern: Vec<char>,
    case: CaseMatching,
}

impl Glob {
    fn new(pattern: &str) -> Self {
        let case = CaseMatching::smart(pattern);
        Self {
            pattern: text::fold(pattern, case).chars().collect(),
            case,
        }
    }

    /// Classic wildcard matching with backtracking on the last `*`: linear in practice.
    fn matches(&self, name: &str) -> bool {
        let name: Vec<char> = text::fold(name, self.case).chars().collect();
        let (mut p, mut n) = (0, 0);
        let mut star: Option<(usize, usize)> = None;
        while n < name.len() {
            match self.pattern.get(p) {
                Some('*') => {
                    star = Some((p, n));
                    p += 1;
                }
                Some(&c) if c == '?' || c == name[n] => {
                    p += 1;
                    n += 1;
                }
                _ => match star {
                    Some((star_p, star_n)) => {
                        p = star_p + 1;
                        n = star_n + 1;
                        star = Some((star_p, star_n + 1));
                    }
                    None => return false,
                },
            }
        }
        self.pattern[p..].iter().all(|&c| c == '*')
    }
}

#[cfg(test)]
mod tests;
