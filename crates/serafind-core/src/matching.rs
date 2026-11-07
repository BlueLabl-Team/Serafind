//! Which entries a query keeps, and in which order.
//!
//! Strict matching first (as in PowerHive): every word must be contained in the name (a word with
//! `*` or `?` is a pattern found anywhere in the name; `^` anchors it at the start of the name and
//! `$` at the end), no excluded word may be, the extension must be one
//! of `ext:`, and the kind must fit the mode. When no entry matches strictly, the words are matched
//! fuzzily instead (`fctr` finds `facture`); the other criteria still apply.
//!
//! Ranking, best first: in All mode apps come before folders and files; then exact name, name
//! starting with the first word, every word starting a word of the name, other matches; then
//! shorter paths, shorter names, alphabetical order and the caller's id.

use std::borrow::Cow;
use std::cmp::{Ordering, Reverse};
use std::collections::BinaryHeap;

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
    /// The name already folded with case ignored ([`text::fold`]), when the caller keeps it ready;
    /// saves folding it again for every word.
    pub folded_name: Option<&'a str>,
    /// How deep the entry is (number of parent folders): shorter paths rank first.
    pub depth: u16,
    /// What the entry is.
    pub kind: EntryKind,
    /// The caller's identifier for the entry (an index entry, an app...), given back with the result.
    pub id: u32,
}

impl Candidate<'_> {
    /// The name folded for `case`: the ready one when it fits, else folded now.
    fn folded(&self, case: CaseMatching) -> Cow<'_, str> {
        match (case, self.folded_name) {
            (CaseMatching::Ignore, Some(folded)) => Cow::Borrowed(folded),
            _ => Cow::Owned(text::fold(self.name, case)),
        }
    }
}

/// How well a candidate matches; smaller is better.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Score {
    kind_rank: u8,
    match_rank: MatchRank,
    fuzzy: Reverse<u32>,
    depth: u16,
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
            .map(|word| {
                let word = if is_pattern(word) {
                    word.trim_start_matches('^').trim_end_matches('$')
                } else {
                    word
                };
                word.replace(['*', '?'], "")
            })
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
        if !self.passes_filters(candidate) || !self.words.iter().all(|word| word.is_in(candidate)) {
            return None;
        }
        Some(self.score(candidate, self.strict_rank(candidate), 0))
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
        self.scan(candidates, limit).finish()
    }

    /// Matches a part of the candidates. Parts scanned separately (in parallel, each with its own
    /// matcher) are put together with [`Partial::merge`], then [`Partial::finish`] gives the result.
    pub fn scan<'a>(
        &mut self,
        candidates: impl IntoIterator<Item = Candidate<'a>>,
        limit: usize,
    ) -> Partial<'a> {
        let mut partial = Partial::new(limit);
        for candidate in candidates {
            if let Some(score) = self.strict(&candidate) {
                partial.strict.push(score, candidate);
            } else if partial.strict.total == 0
                && let Some(score) = self.fuzzy(&candidate)
            {
                // Fuzzy matches only matter while nothing matches strictly.
                partial.fuzzy.push(score, candidate);
            }
        }
        partial
    }

    /// Which characters of the name to highlight (indices of `char`s, sorted, without repeats):
    /// where each word was found, or the letters the fuzzy matching used. Meant for the few
    /// results shown, not for every candidate.
    pub fn highlights(&mut self, candidate: &Candidate<'_>, fuzzy: bool) -> Vec<usize> {
        let mut chars = Vec::new();
        if fuzzy {
            let haystack = Utf32Str::new(candidate.name, &mut self.buffer);
            let mut indices = Vec::new();
            for atom in &self.fuzzy_atoms {
                atom.indices(haystack, &mut self.fuzzy, &mut indices);
            }
            chars.extend(indices.into_iter().map(|index| index as usize));
        } else {
            for word in &self.words {
                if let Word::Text(needle) = word {
                    let folded = candidate.folded(needle.case());
                    if let Some(at) = folded.find(needle.folded()) {
                        // Folding keeps one char per char: count chars to find the position.
                        let start = folded[..at].chars().count();
                        chars.extend(start..start + needle.folded().chars().count());
                    }
                }
            }
        }
        chars.sort_unstable();
        chars.dedup();
        chars
    }

    fn passes_filters(&self, candidate: &Candidate<'_>) -> bool {
        let kind_fits = match self.mode {
            SearchMode::All => true,
            SearchMode::Apps => candidate.kind == EntryKind::App,
            SearchMode::Folders => candidate.kind == EntryKind::Folder,
            SearchMode::Files | SearchMode::Content => candidate.kind == EntryKind::File,
        };
        if !kind_fits || self.excluded_words.iter().any(|word| word.is_in(candidate)) {
            return false;
        }
        if self.extensions.is_empty() && self.excluded_extensions.is_empty() {
            return true;
        }
        let folded = candidate.folded(CaseMatching::Ignore);
        let extension = extension_of(candidate, &folded);
        let has =
            |list: &[String]| extension.is_some_and(|extension| list.iter().any(|item| item == extension));
        (self.extensions.is_empty() || has(&self.extensions)) && !has(&self.excluded_extensions)
    }

    fn strict_rank(&self, candidate: &Candidate<'_>) -> MatchRank {
        if let Some(whole) = &self.whole {
            let folded = candidate.folded(whole.case());
            let stem = folded.rsplit_once('.').map_or(&*folded, |(stem, _)| stem);
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
        if candidate.folded(first.case()).starts_with(first.folded()) {
            MatchRank::NamePrefix
        } else if starts_a_word(candidate, first) && texts.all(|needle| starts_a_word(candidate, needle)) {
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
            depth: candidate.depth,
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

/// Matches of a part of the candidates (see [`Matcher::scan`]).
#[derive(Debug, Clone)]
pub struct Partial<'a> {
    strict: TopK<'a>,
    fuzzy: TopK<'a>,
}

impl<'a> Partial<'a> {
    fn new(limit: usize) -> Self {
        Self {
            strict: TopK::new(limit),
            fuzzy: TopK::new(limit),
        }
    }

    /// Adds the matches of another part.
    pub fn merge(&mut self, other: Self) {
        self.strict.merge(other.strict);
        self.fuzzy.merge(other.fuzzy);
    }

    /// The best matches: strict ones, or fuzzy ones when nothing matched strictly.
    #[must_use]
    pub fn finish(self) -> Ranked<'a> {
        let is_fuzzy = self.strict.total == 0 && self.fuzzy.total > 0;
        let found = if is_fuzzy { self.fuzzy } else { self.strict };
        let total = found.total;
        let mut entries: Vec<Ordered<'a>> = found.heap.into_vec();
        entries.sort_unstable();
        Ranked {
            entries: entries
                .into_iter()
                .map(|Ordered(score, candidate)| (score, candidate))
                .collect(),
            total,
            is_fuzzy,
        }
    }
}

/// The `limit` best matches seen so far, and how many were seen.
#[derive(Debug, Clone)]
struct TopK<'a> {
    limit: usize,
    /// A max-heap: the worst kept match is on top, ready to be replaced.
    heap: BinaryHeap<Ordered<'a>>,
    total: usize,
}

impl<'a> TopK<'a> {
    fn new(limit: usize) -> Self {
        Self {
            limit,
            heap: BinaryHeap::with_capacity(limit.min(1024) + 1),
            total: 0,
        }
    }

    fn push(&mut self, score: Score, candidate: Candidate<'a>) {
        self.total += 1;
        self.offer(Ordered(score, candidate));
    }

    fn offer(&mut self, item: Ordered<'a>) {
        if self.heap.len() < self.limit {
            self.heap.push(item);
        } else if let Some(mut worst) = self.heap.peek_mut()
            && item < *worst
        {
            *worst = item;
        }
    }

    fn merge(&mut self, other: Self) {
        self.total += other.total;
        for item in other.heap {
            self.offer(item);
        }
    }
}

/// A match in ranking order: score, then name, then the caller's id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Ordered<'a>(Score, Candidate<'a>);

impl Ord for Ordered<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0
            .cmp(&other.0)
            .then_with(|| self.1.name.cmp(other.1.name))
            .then_with(|| self.1.id.cmp(&other.1.id))
    }
}

impl PartialOrd for Ordered<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// The extension of a folded name: ".bashrc" has none, "archive.tar.gz" has "gz"; apps have none.
fn extension_of<'n>(candidate: &Candidate<'_>, folded: &'n str) -> Option<&'n str> {
    if candidate.kind == EntryKind::App {
        return None;
    }
    let (stem, extension) = folded.rsplit_once('.')?;
    (!stem.is_empty() && !extension.is_empty()).then_some(extension)
}

/// True when the needle is found at the start of a word of the name: after the start, a
/// non-alphanumeric character, a lower-to-upper case change, or a letter-digit change.
///
/// Folding maps each character to exactly one character, so the original name (for the word
/// boundaries) and the folded one (for the comparison) are walked side by side.
fn starts_a_word(candidate: &Candidate<'_>, needle: &Needle) -> bool {
    let folded = candidate.folded(needle.case());
    let target = needle.folded();
    if target.is_empty() {
        return true;
    }
    let mut previous: Option<char> = None;
    for ((at, _), current) in folded.char_indices().zip(candidate.name.chars()) {
        if is_word_start(previous, current) && folded[at..].starts_with(target) {
            return true;
        }
        previous = Some(current);
    }
    false
}

fn is_word_start(previous: Option<char>, current: char) -> bool {
    let Some(previous) = previous else {
        return true;
    };
    !previous.is_alphanumeric()
        || (previous.is_lowercase() && current.is_uppercase())
        || (previous.is_alphabetic() && current.is_numeric())
        || (previous.is_numeric() && current.is_alphabetic())
}

impl Word {
    fn new(word: &str) -> Self {
        if is_pattern(word) {
            Self::Pattern(Glob::new(word))
        } else {
            Self::Text(Needle::new(word))
        }
    }

    fn is_in(&self, candidate: &Candidate<'_>) -> bool {
        match self {
            Self::Text(needle) => candidate.folded(needle.case()).contains(needle.folded()),
            Self::Pattern(glob) => glob.matches_folded(&candidate.folded(glob.case)),
        }
    }
}

/// A `*` / `?` pattern matched against the whole name, with the same case and accent rules as words.
struct Glob {
    pattern: Vec<char>,
    case: CaseMatching,
}

/// True when a word is a pattern: it has `*` or `?`, starts with `^` or ends with `$`.
fn is_pattern(word: &str) -> bool {
    word.contains(['*', '?']) || (word.len() > 1 && (word.starts_with('^') || word.ends_with('$')))
}

impl Glob {
    /// A pattern found anywhere in the name; `^` anchors it at the start, `$` at the end.
    fn new(pattern: &str) -> Self {
        let case = CaseMatching::smart(pattern);
        let (anchored_start, rest) = pattern
            .strip_prefix('^')
            .map_or((false, pattern), |rest| (true, rest));
        let (anchored_end, body) = rest.strip_suffix('$').map_or((false, rest), |body| (true, body));
        let mut chars = Vec::with_capacity(body.len() + 2);
        if !anchored_start {
            chars.push('*');
        }
        chars.extend(text::fold(body, case).chars());
        if !anchored_end {
            chars.push('*');
        }
        Self { pattern: chars, case }
    }

    /// Folds `name`, then matches it (for tests).
    #[cfg(test)]
    fn matches(&self, name: &str) -> bool {
        self.matches_folded(&text::fold(name, self.case))
    }

    /// Matches a name already folded with this pattern's case. Classic wildcard matching with
    /// backtracking on the last `*` (linear in practice), walking the name in place.
    fn matches_folded(&self, name: &str) -> bool {
        let (mut p, mut n) = (0, 0);
        let mut star: Option<(usize, usize)> = None;
        while let Some(c) = name[n..].chars().next() {
            match self.pattern.get(p) {
                Some('*') => {
                    star = Some((p, n));
                    p += 1;
                }
                Some(&wanted) if wanted == '?' || wanted == c => {
                    p += 1;
                    n += c.len_utf8();
                }
                _ => match star {
                    Some((star_p, star_n)) => {
                        // Let the last '*' take one more character and try again.
                        let skipped = name[star_n..].chars().next().map_or(1, char::len_utf8);
                        p = star_p + 1;
                        n = star_n + skipped;
                        star = Some((star_p, n));
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
