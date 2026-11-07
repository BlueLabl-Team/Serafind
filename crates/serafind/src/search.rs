//! Runs a search line against the index and prepares the rows to show.

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use serafind_core::{Candidate, EntryKind, Matcher, ParseContext, Query, SearchMode, parse};
use serafind_index::{Index, Kind};

/// The index the bar searches: shared with the watcher, or not ready yet.
pub type SharedIndex = Arc<RwLock<Option<Arc<RwLock<Index>>>>>;

/// One result, ready to draw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// Name shown.
    pub name: String,
    /// Parent folder, shortened (`~/Documents`).
    pub location: String,
    /// What it is.
    pub kind: EntryKind,
    /// Full path, for the actions.
    pub path: PathBuf,
    /// Characters of the name to highlight.
    pub highlights: Vec<usize>,
}

/// What a search gave.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outcome {
    /// The line as understood.
    pub query: Query,
    /// The mode the search ran in.
    pub mode: SearchMode,
    /// The best results, best first.
    pub rows: Vec<Row>,
    /// How many matched in all.
    pub total: usize,
    /// The rows come from the fuzzy fallback.
    pub is_fuzzy: bool,
    /// `in:` names a folder outside the index.
    pub outside_index: bool,
    /// The index is still being built.
    pub indexing: bool,
}

/// Searches with the home folder as base of `~` and of relative `in:` folders.
pub struct Searcher {
    index: SharedIndex,
    home: PathBuf,
}

impl Searcher {
    /// A searcher over `index` (which may be filled later).
    #[must_use]
    pub fn new(index: SharedIndex, home: PathBuf) -> Self {
        Self { index, home }
    }

    /// The home folder.
    #[must_use]
    pub fn home(&self) -> &Path {
        &self.home
    }

    /// True once the index is ready.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.index.read().is_ok_and(|index| index.is_some())
    }

    /// Understands `line` and searches it in `tab` mode (a mode filter of the line wins).
    #[must_use]
    pub fn run(&self, line: &str, tab: SearchMode, max: usize) -> Outcome {
        let exists = |path: &Path| path.is_dir();
        let context = ParseContext {
            home: &self.home,
            current_folder: &self.home,
            folder_exists: &exists,
        };
        let query = parse(line, &context);
        let mode = query.effective_mode(tab);
        let mut outcome = Outcome {
            mode,
            ..Outcome::default()
        };
        if !query.is_empty() {
            let shared = self.index.read().ok().and_then(|index| index.clone());
            match shared {
                Some(shared) => {
                    if let Ok(index) = shared.read() {
                        self.fill(&mut outcome, &query, &index, max);
                    }
                }
                None => outcome.indexing = true,
            }
        }
        outcome.query = query;
        outcome
    }

    fn fill(&self, outcome: &mut Outcome, query: &Query, index: &Index, max: usize) {
        let results = index.search(query, outcome.mode, max);
        let mut matcher = Matcher::new(query, outcome.mode);
        outcome.total = results.total;
        outcome.is_fuzzy = results.is_fuzzy;
        outcome.outside_index = results.outside_index;
        outcome.rows = results
            .hits
            .iter()
            .map(|hit| {
                let name = index.name(hit.id).to_owned();
                let kind = match index.kind(hit.id) {
                    Some(Kind::Folder) => EntryKind::Folder,
                    _ => EntryKind::File,
                };
                let path = index.path_of(hit.id);
                let candidate = Candidate {
                    name: &name,
                    folded_name: None,
                    depth: 0,
                    kind,
                    id: hit.id,
                };
                let highlights = matcher.highlights(&candidate, results.is_fuzzy);
                Row {
                    location: shorten(path.parent().unwrap_or(&path), &self.home),
                    name,
                    kind,
                    path,
                    highlights,
                }
            })
            .collect();
    }
}

/// `/home/me/Documents` → `~/Documents`.
#[must_use]
pub fn shorten(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Ok(rest) => format!("~/{}", rest.display()).replace('\\', "/"),
        Err(_) => path.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn searcher(paths: &[&str]) -> Searcher {
        let mut index = Index::new();
        index.add_root(Path::new("/home/me"));
        for path in paths {
            let kind = if path.ends_with('/') {
                Kind::Folder
            } else {
                Kind::File
            };
            index.insert_path(&Path::new("/home/me").join(path.trim_end_matches('/')), kind);
        }
        let shared: SharedIndex = Arc::new(RwLock::new(Some(Arc::new(RwLock::new(index)))));
        Searcher::new(shared, PathBuf::from("/home/me"))
    }

    #[test]
    fn rows_have_locations_and_highlights() {
        let searcher = searcher(&["Documents/facture-2026.pdf", "Factures/"]);
        let outcome = searcher.run("fact", SearchMode::All, 10);
        assert_eq!(outcome.total, 2);
        let rows: Vec<(&str, &str)> = outcome
            .rows
            .iter()
            .map(|row| (row.name.as_str(), row.location.as_str()))
            .collect();
        assert!(rows.contains(&("facture-2026.pdf", "~/Documents")));
        assert!(rows.contains(&("Factures", "~")));
        assert!(outcome.rows.iter().all(|row| row.highlights == [0, 1, 2, 3]));
    }

    #[test]
    fn empty_line_searches_nothing() {
        let outcome = searcher(&["a.txt"]).run("   ", SearchMode::All, 10);
        assert!(outcome.rows.is_empty());
        assert!(!outcome.indexing);
    }

    #[test]
    fn index_not_ready() {
        let searcher = Searcher::new(Arc::new(RwLock::new(None)), PathBuf::from("/home/me"));
        assert!(!searcher.is_ready());
        assert!(searcher.run("a", SearchMode::All, 10).indexing);
    }

    #[test]
    fn mode_filters_win_over_the_tab() {
        let outcome = searcher(&["x/"]).run("d: x", SearchMode::Files, 10);
        assert_eq!(outcome.mode, SearchMode::Folders);
        assert_eq!(outcome.rows.len(), 1);
    }

    #[test]
    fn shortening() {
        let home = Path::new("/home/me");
        assert_eq!(shorten(Path::new("/home/me"), home), "~");
        assert_eq!(shorten(Path::new("/home/me/a/b"), home), "~/a/b");
        assert_eq!(shorten(Path::new("/etc"), home), "/etc");
    }
}
