//! The Content mode: finds text inside the files of the index, like ripgrep, in the background.
//! A search can be cancelled, and shows what it found so far while it runs.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use grep_matcher::Matcher as _;
use grep_regex::{RegexMatcher, RegexMatcherBuilder};
use grep_searcher::{SearcherBuilder, sinks::Lossy};
use serafind_core::Query;

/// Bytes read at the start of a file to tell a binary file (it has a NUL byte).
const BINARY_PROBE: usize = 8 * 1024;
/// Matching lines counted per file at most.
const MAX_COUNT: u32 = 999;
/// Characters of a matching line kept around the match.
const LINE_LENGTH: usize = 160;

/// What to find inside the files.
#[derive(Debug, Clone)]
pub struct ContentQuery {
    /// The first is shown (line and highlights); every one must match.
    wanted: Vec<RegexMatcher>,
    /// None may match.
    unwanted: Vec<RegexMatcher>,
}

impl ContentQuery {
    /// From the search line: its words as one phrase, `c:` texts, `re:` expressions. Texts are
    /// literal; case is ignored unless they have an upper-case letter. `Ok(None)` when there is
    /// nothing to find.
    ///
    /// # Errors
    /// A `re:` expression that is not valid, with the reason.
    pub fn new(query: &Query) -> Result<Option<Self>, String> {
        let (texts, unwanted_texts) = query.content_texts();
        let build = |pattern: &str, literal: bool| {
            RegexMatcherBuilder::new()
                .case_smart(true)
                .fixed_strings(literal)
                .build(pattern)
                .map_err(|error| format!("re:{pattern:?} is not a valid expression: {error}"))
        };
        let mut wanted = Vec::new();
        for text in &texts {
            wanted.push(build(text, true)?);
        }
        for pattern in &query.content_patterns {
            wanted.push(build(pattern, false)?);
        }
        let mut unwanted = Vec::new();
        for text in &unwanted_texts {
            unwanted.push(build(text, true)?);
        }
        for pattern in &query.excluded_content_patterns {
            unwanted.push(build(pattern, false)?);
        }
        Ok((!wanted.is_empty()).then_some(Self { wanted, unwanted }))
    }

    /// Looks into `bytes`: `None` when the file does not match (or is binary).
    #[must_use]
    pub fn find(&self, bytes: &[u8]) -> Option<Found> {
        if bytes[..bytes.len().min(BINARY_PROBE)].contains(&0) {
            return None;
        }
        let (first, others) = self.wanted.split_first()?;
        if !others
            .iter()
            .all(|matcher| matcher.is_match(bytes).unwrap_or(false))
            || self
                .unwanted
                .iter()
                .any(|matcher| matcher.is_match(bytes).unwrap_or(false))
        {
            return None;
        }
        let mut found: Option<Found> = None;
        let mut count = 0u32;
        let mut searcher = SearcherBuilder::new().line_number(true).build();
        let result = searcher.search_slice(
            first,
            bytes,
            Lossy(|line_number, line: &str| {
                count += 1;
                if found.is_none() {
                    found = Some(Found::of_line(first, line_number, line));
                }
                Ok(count < MAX_COUNT)
            }),
        );
        let mut found = found.filter(|_| result.is_ok())?;
        found.count = count;
        Some(found)
    }
}

/// A file that matches: its first matching line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// Line number, from 1.
    pub line: u32,
    /// The line, trimmed and shortened around the match.
    pub text: String,
    /// Characters of `text` that match.
    pub highlights: Vec<u32>,
    /// Matching lines in the file (counting stops at 999).
    pub count: u32,
}

impl Found {
    fn of_line(matcher: &RegexMatcher, line_number: u64, line: &str) -> Self {
        let line = line.trim_end_matches(['\n', '\r']);
        let mut ranges = Vec::new();
        let _ = matcher.find_iter(line.as_bytes(), |found| {
            ranges.push(found.start()..found.end());
            true
        });
        // Keep a window of the line around the first match, without leading spaces.
        let first = ranges.first().map_or(0, |range| range.start);
        let window = line[..first]
            .char_indices()
            .rev()
            .nth(LINE_LENGTH / 4)
            .map_or(0, |(index, _)| index);
        let cut = window > 0;
        let start = window + line[window..].len() - line[window..].trim_start().len();
        let end = line[start..]
            .char_indices()
            .nth(LINE_LENGTH)
            .map_or(line.len(), |(index, _)| start + index);
        let text = &line[start..end];
        let highlights: Vec<u32> = ranges
            .iter()
            .filter(|range| range.start >= start && range.end <= end)
            .flat_map(|range| {
                let from = line[start..range.start].chars().count();
                let length = line[range.clone()].chars().count();
                from..from + length
            })
            .filter_map(|index| u32::try_from(index).ok())
            .collect();
        let prefix = if cut { "…" } else { "" };
        Self {
            line: u32::try_from(line_number).unwrap_or(u32::MAX),
            text: format!("{prefix}{text}"),
            highlights: if cut {
                // The "…" shifts the characters by one.
                highlights.into_iter().map(|index: u32| index + 1).collect()
            } else {
                highlights
            },
            count: 0,
        }
    }
}

/// What a search found so far.
#[derive(Debug, Clone, Default)]
pub struct Progress {
    /// Matching files, in the order found (shallow files are read first).
    pub hits: Vec<(PathBuf, Found)>,
    /// Matching files in all (more than `hits` when the limit was reached).
    pub total: usize,
    /// Files read.
    pub read: usize,
    /// Files to read.
    pub files: usize,
    /// All files were read.
    pub done: bool,
}

/// A search running in the background; cancelled when dropped.
pub struct ContentSearch {
    cancel: Arc<AtomicBool>,
    progress: Arc<Mutex<Progress>>,
}

impl ContentSearch {
    /// Starts reading `files` (those larger than `max_size` bytes are skipped); `limit` matching
    /// files are kept, all are counted.
    #[must_use]
    pub fn start(files: Vec<PathBuf>, query: ContentQuery, max_size: u64, limit: usize) -> Self {
        let cancel = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(Mutex::new(Progress {
            files: files.len(),
            ..Progress::default()
        }));
        let search = Self {
            cancel: Arc::clone(&cancel),
            progress: Arc::clone(&progress),
        };
        std::thread::spawn(move || {
            let next = AtomicUsize::new(0);
            let threads = std::thread::available_parallelism().map_or(2, |count| count.get().min(8));
            std::thread::scope(|scope| {
                for _ in 0..threads {
                    scope.spawn(|| {
                        while !cancel.load(Ordering::Relaxed) {
                            let position = next.fetch_add(1, Ordering::Relaxed);
                            let Some(file) = files.get(position) else {
                                break;
                            };
                            let found = read(file, max_size).and_then(|bytes| query.find(&bytes));
                            let Ok(mut progress) = progress.lock() else {
                                break;
                            };
                            progress.read += 1;
                            if let Some(found) = found {
                                progress.total += 1;
                                if progress.hits.len() < limit {
                                    progress.hits.push((file.clone(), found));
                                }
                            }
                        }
                    });
                }
            });
            if let Ok(mut progress) = progress.lock() {
                progress.done = !cancel.load(Ordering::Relaxed);
            }
        });
        search
    }

    /// What was found so far.
    #[must_use]
    pub fn progress(&self) -> Progress {
        self.progress
            .lock()
            .map(|progress| progress.clone())
            .unwrap_or_default()
    }

    /// Waits until the search is done or `time` has passed.
    pub fn wait(&self, time: std::time::Duration) {
        let deadline = std::time::Instant::now() + time;
        while std::time::Instant::now() < deadline && !self.progress().done {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
}

impl Drop for ContentSearch {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// The bytes of a file, unless it is larger than `max_size` or cannot be read.
fn read(file: &Path, max_size: u64) -> Option<Vec<u8>> {
    let metadata = std::fs::metadata(file).ok()?;
    if !metadata.is_file() || metadata.len() > max_size {
        return None;
    }
    std::fs::read(file).ok()
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::Duration;

    use super::*;

    fn content_query(line: &str) -> ContentQuery {
        let home = Path::new("/home/me");
        let query = crate::search::understand(line, home);
        ContentQuery::new(&query).unwrap().unwrap()
    }

    #[test]
    fn finds_the_first_line_and_counts() {
        let text = b"fn main() {\n    // TODO: fix the parser\n    let todo = 1; // todo again\n}\n";
        let found = content_query("todo").find(text).unwrap();
        assert_eq!(found.line, 2);
        assert_eq!(found.text, "// TODO: fix the parser");
        assert_eq!(found.highlights, [3, 4, 5, 6]);
        assert_eq!(found.count, 2);
    }

    #[test]
    fn case_is_ignored_unless_there_is_an_upper_case_letter() {
        let text = b"a todo\nA TODO\n";
        assert_eq!(content_query("TODO").find(text).unwrap().line, 2);
        assert_eq!(content_query("todo").find(text).unwrap().count, 2);
    }

    #[test]
    fn every_text_must_be_there_and_excluded_ones_absent() {
        let text = b"invoice for march\ntotal 120\n";
        assert!(content_query("invoice c:total").find(text).is_some());
        assert!(content_query("invoice c:paid").find(text).is_none());
        assert!(content_query("invoice -march").find(text).is_none());
        assert!(content_query(r#"re:"total \d+""#).find(text).is_some());
        assert!(content_query(r#"invoice -re:"\d{3}""#).find(text).is_none());
    }

    #[test]
    fn words_are_one_literal_phrase() {
        assert!(content_query("fix the").find(b"fix the parser").is_some());
        assert!(content_query("the fix").find(b"fix the parser").is_none());
        assert!(
            content_query("a.c").find(b"abc").is_none(),
            "literal, not a regex"
        );
    }

    #[test]
    fn binary_files_are_skipped() {
        assert!(content_query("todo").find(b"todo\0\x01\x02").is_none());
    }

    #[test]
    fn long_lines_are_cut_around_the_match() {
        let line = format!("{}needle{}", "x".repeat(300), "y".repeat(300));
        let found = content_query("needle").find(line.as_bytes()).unwrap();
        assert!(found.text.starts_with('…'));
        assert!(found.text.chars().count() <= LINE_LENGTH + 1);
        let shown: String = found
            .highlights
            .iter()
            .map(|&index| found.text.chars().nth(index as usize).unwrap())
            .collect();
        assert_eq!(shown, "needle");
    }

    #[test]
    fn bad_expressions_are_errors_and_nothing_to_find_is_none() {
        let home = Path::new("/home/me");
        let error = ContentQuery::new(&crate::search::understand("re:\"(\"", home)).unwrap_err();
        assert!(error.contains("not a valid expression"), "{error}");
        assert!(
            ContentQuery::new(&crate::search::understand("ext:rs", home))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn searches_files_in_the_background() {
        let folder = assert_fs::TempDir::new().unwrap();
        let mut files = Vec::new();
        for (name, text) in [
            ("a.txt", "nothing here"),
            ("b.txt", "a needle"),
            ("c.txt", "needle again"),
            ("big.txt", "needle in a big file"),
        ] {
            let path = folder.path().join(name);
            std::fs::write(&path, text).unwrap();
            files.push(path);
        }
        // big.txt is over the size limit.
        let search = ContentSearch::start(files, content_query("needle"), 15, 1);
        search.wait(Duration::from_secs(5));
        let progress = search.progress();
        assert!(progress.done);
        assert_eq!((progress.read, progress.files, progress.total), (4, 4, 2));
        assert_eq!(progress.hits.len(), 1, "limited to 1");
    }
}
