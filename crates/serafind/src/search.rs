//! Runs a search line and prepares the rows to show.
//!
//! The resident process searches its index with [`search_index`]. The bar asks it through the
//! local socket ([`Searcher::remote`]); tests give the bar an index directly ([`Searcher::local`]).
//! Either way the bar understands the line itself, for its chips.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use serafind_core::{Candidate, EntryKind, Matcher, ParseContext, Query, SearchMode, parse};
use serafind_index::{Index, Kind};
use serafind_ipc::{Client, Request, Response, Results};

/// An index that may not be built yet.
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
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent facts shown in the footer"
)]
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
    /// The resident process could not be reached.
    pub unreachable: bool,
}

enum Source {
    #[cfg(test)]
    Local(SharedIndex),
    Remote(Mutex<Client>),
}

/// Searches with the home folder as base of `~` and of relative `in:` folders.
pub struct Searcher {
    source: Source,
    home: PathBuf,
}

impl Searcher {
    /// Searches an index of this process (tests).
    #[cfg(test)]
    #[must_use]
    pub fn local(index: SharedIndex, home: PathBuf) -> Self {
        Self {
            source: Source::Local(index),
            home,
        }
    }

    /// Asks the resident process.
    #[must_use]
    pub fn remote(client: Client, home: PathBuf) -> Self {
        Self {
            source: Source::Remote(Mutex::new(client)),
            home,
        }
    }

    /// Asks the resident process to do `action` on `path`.
    ///
    /// # Errors
    /// Why it was not done.
    pub fn act(&self, action: serafind_ipc::Action, path: &Path) -> Result<(), String> {
        match &self.source {
            #[cfg(test)]
            Source::Local(_) => {
                let _ = (action, path);
                Ok(())
            }
            Source::Remote(client) => {
                let request = Request::Act {
                    action,
                    path: path.to_string_lossy().into_owned(),
                    activation_token: None,
                };
                let mut client = client.lock().map_err(|_| "the connection is broken".to_owned())?;
                match client.call(&request) {
                    Ok(Response::Ok) => Ok(()),
                    Ok(Response::Error { message }) => Err(message),
                    Ok(other) => Err(format!("unexpected answer: {other:?}")),
                    Err(error) => Err(format!("Serafind does not answer: {error}")),
                }
            }
        }
    }

    /// The home folder.
    #[must_use]
    pub fn home(&self) -> &Path {
        &self.home
    }

    /// Understands `line` and searches it in `tab` mode (a mode filter of the line wins).
    #[must_use]
    pub fn run(&self, line: &str, tab: SearchMode, max: usize) -> Outcome {
        let query = understand(line, &self.home);
        let mut outcome = Outcome {
            mode: query.effective_mode(tab),
            ..Outcome::default()
        };
        if !query.is_empty() {
            match self.results(line, tab, max) {
                Some(results) => fill(&mut outcome, results),
                None => outcome.unreachable = true,
            }
        }
        outcome.query = query;
        outcome
    }

    fn results(&self, line: &str, tab: SearchMode, max: usize) -> Option<Results> {
        match &self.source {
            #[cfg(test)]
            Source::Local(shared) => {
                let index = shared.read().ok()?.clone();
                Some(match index {
                    Some(index) => {
                        let query = understand(line, &self.home);
                        search_index(&*index.read().ok()?, &query, tab, max, &self.home)
                    }
                    None => Results {
                        indexing: true,
                        ..Results::default()
                    },
                })
            }
            Source::Remote(client) => {
                let request = Request::Search {
                    line: line.to_owned(),
                    mode: to_wire_mode(tab),
                    max: u32::try_from(max).unwrap_or(u32::MAX),
                };
                match client.lock().ok()?.call(&request) {
                    Ok(Response::Results(results)) => Some(results),
                    _ => None,
                }
            }
        }
    }
}

/// Understands a search line, with `home` as base of `~` and of relative `in:` folders.
#[must_use]
pub fn understand(line: &str, home: &Path) -> Query {
    let exists = |path: &Path| path.is_dir();
    let context = ParseContext {
        home,
        current_folder: home,
        folder_exists: &exists,
    };
    parse(line, &context)
}

/// Searches `index` and prepares the rows (names, shortened folders, highlights).
#[must_use]
pub fn search_index(index: &Index, query: &Query, tab: SearchMode, max: usize, home: &Path) -> Results {
    let mode = query.effective_mode(tab);
    let found = index.search(query, mode, max);
    let mut matcher = Matcher::new(query, mode);
    let rows = found
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
            let highlights = matcher
                .highlights(&candidate, found.is_fuzzy)
                .into_iter()
                .map(|index| u32::try_from(index).unwrap_or(u32::MAX))
                .collect();
            serafind_ipc::Row {
                location: shorten(path.parent().unwrap_or(&path), home),
                name,
                path: path.to_string_lossy().into_owned(),
                kind: to_wire_kind(kind),
                highlights,
            }
        })
        .collect();
    Results {
        mode: Some(to_wire_mode(mode)),
        rows,
        total: found.total as u64,
        fuzzy: found.is_fuzzy,
        outside_index: found.outside_index,
        indexing: false,
    }
}

fn fill(outcome: &mut Outcome, results: Results) {
    if let Some(mode) = results.mode {
        outcome.mode = from_wire_mode(mode);
    }
    outcome.total = usize::try_from(results.total).unwrap_or(usize::MAX);
    outcome.is_fuzzy = results.fuzzy;
    outcome.outside_index = results.outside_index;
    outcome.indexing = results.indexing;
    outcome.rows = results
        .rows
        .into_iter()
        .map(|row| Row {
            name: row.name,
            location: row.location,
            kind: from_wire_kind(row.kind),
            path: PathBuf::from(row.path),
            highlights: row.highlights.into_iter().map(|index| index as usize).collect(),
        })
        .collect();
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

/// The protocol's name of a mode.
#[must_use]
pub fn to_wire_mode(mode: SearchMode) -> serafind_ipc::Mode {
    match mode {
        SearchMode::All => serafind_ipc::Mode::All,
        SearchMode::Apps => serafind_ipc::Mode::Apps,
        SearchMode::Folders => serafind_ipc::Mode::Folders,
        SearchMode::Files => serafind_ipc::Mode::Files,
        SearchMode::Content => serafind_ipc::Mode::Content,
    }
}

/// A mode from its protocol name.
#[must_use]
pub fn from_wire_mode(mode: serafind_ipc::Mode) -> SearchMode {
    match mode {
        serafind_ipc::Mode::All => SearchMode::All,
        serafind_ipc::Mode::Apps => SearchMode::Apps,
        serafind_ipc::Mode::Folders => SearchMode::Folders,
        serafind_ipc::Mode::Files => SearchMode::Files,
        serafind_ipc::Mode::Content => SearchMode::Content,
    }
}

fn to_wire_kind(kind: EntryKind) -> serafind_ipc::Kind {
    match kind {
        EntryKind::App => serafind_ipc::Kind::App,
        EntryKind::Folder => serafind_ipc::Kind::Folder,
        EntryKind::File => serafind_ipc::Kind::File,
    }
}

fn from_wire_kind(kind: serafind_ipc::Kind) -> EntryKind {
    match kind {
        serafind_ipc::Kind::App => EntryKind::App,
        serafind_ipc::Kind::Folder => EntryKind::Folder,
        serafind_ipc::Kind::File => EntryKind::File,
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
        Searcher::local(shared, PathBuf::from("/home/me"))
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
        let searcher = Searcher::local(Arc::new(RwLock::new(None)), PathBuf::from("/home/me"));
        assert!(searcher.run("a", SearchMode::All, 10).indexing);
    }

    #[test]
    fn mode_filters_win_over_the_tab() {
        let outcome = searcher(&["x/"]).run("d: x", SearchMode::Files, 10);
        assert_eq!(outcome.mode, SearchMode::Folders);
        assert_eq!(outcome.rows.len(), 1);
    }

    #[test]
    fn modes_go_through_the_protocol() {
        for mode in SearchMode::ALL {
            assert_eq!(from_wire_mode(to_wire_mode(mode)), mode);
        }
    }

    #[test]
    fn shortening() {
        let home = Path::new("/home/me");
        assert_eq!(shorten(Path::new("/home/me"), home), "~");
        assert_eq!(shorten(Path::new("/home/me/a/b"), home), "~/a/b");
        assert_eq!(shorten(Path::new("/etc"), home), "/etc");
    }
}
