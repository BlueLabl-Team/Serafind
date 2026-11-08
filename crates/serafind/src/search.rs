//! Runs a search line and prepares the rows to show.
//!
//! The resident process searches its applications and its index with [`search`]. The bar asks
//! it through the local socket ([`Searcher::remote`]); tests give the bar an index directly
//! ([`Searcher::local`]). Either way the bar understands the line itself, for its chips.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use serafind_apps::App;

use serafind_core::{Candidate, EntryKind, Matcher, ParseContext, Query, SearchMode, parse};
use serafind_index::{Index, Kind};
use serafind_ipc::{Client, Request, Response, Results};

/// An index that may not be built yet.
pub type SharedIndex = Arc<RwLock<Option<Arc<RwLock<Index>>>>>;

/// The installed applications, replaced when they change.
pub type SharedApps = Arc<RwLock<Arc<Vec<App>>>>;

/// How many times each application (by id) was started from Serafind.
pub type Launches = HashMap<String, u32>;

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
    /// For an application: its id, to start it.
    pub app: Option<String>,
    /// For an application: its icon file.
    pub icon: Option<PathBuf>,
    /// Content mode: the first matching line of the file.
    pub line: Option<serafind_ipc::Line>,
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
    /// Content mode: files are still being read.
    pub searching: bool,
    /// Content mode: how much of the files was read, in percent.
    pub progress: Option<u8>,
    /// Why the search could not run.
    pub problem: Option<String>,
}

enum Source {
    #[cfg(test)]
    Local(SharedIndex, Vec<App>),
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
        Self::local_with_apps(index, Vec::new(), home)
    }

    /// Searches an index and applications of this process (tests).
    #[cfg(test)]
    #[must_use]
    pub fn local_with_apps(index: SharedIndex, apps: Vec<App>, home: PathBuf) -> Self {
        Self {
            source: Source::Local(index, apps),
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
            Source::Local(..) => {
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
            Source::Local(shared, apps) => {
                let index = shared.read().ok()?.clone();
                let index = index.as_ref().and_then(|index| index.read().ok());
                let query = understand(line, &self.home);
                Some(search(index.as_deref(), apps, None, &query, tab, max, &self.home))
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

/// Searches the applications and the index (`None` while it is being built), and prepares the
/// rows (names, shortened folders, highlights). In the All mode applications come first. When
/// one side matches strictly, the fuzzy matches of the other are left out. `launches` puts the
/// applications started most often first, among equally good matches.
#[must_use]
pub fn search(
    index: Option<&Index>,
    apps: &[App],
    launches: Option<&Launches>,
    query: &Query,
    tab: SearchMode,
    max: usize,
    home: &Path,
) -> Results {
    let mode = query.effective_mode(tab);
    let mut matcher = Matcher::new(query, mode);
    let wants_apps = matches!(mode, SearchMode::All | SearchMode::Apps) && query.start_folder.is_none();
    let (mut app_rows, app_total, apps_fuzzy) = if wants_apps {
        search_apps(&mut matcher, apps, launches, max, home)
    } else {
        (Vec::new(), 0, false)
    };
    let wants_files = mode != SearchMode::Apps;
    let found = match index {
        Some(index) if wants_files => Some((index.search(query, mode, max), index)),
        _ => None,
    };
    let (mut file_rows, file_total, files_fuzzy, outside_index) = match &found {
        Some((found, index)) => (
            file_rows(&mut matcher, found, index, home),
            found.total,
            found.is_fuzzy,
            found.outside_index,
        ),
        None => (Vec::new(), 0, false, false),
    };
    let (mut app_total, mut file_total) = (app_total, file_total);
    if !app_rows.is_empty() && !apps_fuzzy && files_fuzzy {
        file_rows.clear();
        file_total = 0;
    }
    if !file_rows.is_empty() && !files_fuzzy && apps_fuzzy {
        app_rows.clear();
        app_total = 0;
    }
    let fuzzy = (app_rows.is_empty() || apps_fuzzy)
        && (file_rows.is_empty() || files_fuzzy)
        && !(app_rows.is_empty() && file_rows.is_empty());
    let mut rows = app_rows;
    rows.extend(file_rows);
    rows.truncate(max);
    Results {
        mode: Some(to_wire_mode(mode)),
        rows,
        total: (app_total + file_total) as u64,
        fuzzy,
        outside_index,
        indexing: wants_files && index.is_none(),
        ..Results::default()
    }
}

/// The best applications: rows, how many matched, and whether they come from the fuzzy fallback.
fn search_apps(
    matcher: &mut Matcher,
    apps: &[App],
    launches: Option<&Launches>,
    max: usize,
    home: &Path,
) -> (Vec<serafind_ipc::Row>, usize, bool) {
    let candidates = apps.iter().enumerate().map(|(position, app)| Candidate {
        name: &app.name,
        folded_name: None,
        depth: 0,
        kind: EntryKind::App,
        id: u32::try_from(position).unwrap_or(u32::MAX),
    });
    let mut ranked = matcher.best(candidates, apps.len().max(1));
    if let Some(launches) = launches {
        let count = |id: u32| {
            apps.get(id as usize)
                .and_then(|app| launches.get(&app.id))
                .copied()
                .unwrap_or(0)
        };
        // Stable: equally good and equally used apps keep their order.
        ranked
            .entries
            .sort_by_key(|(score, candidate)| (score.match_rank(), std::cmp::Reverse(count(candidate.id))));
    }
    ranked.entries.truncate(max);
    let rows = ranked
        .entries
        .iter()
        .filter_map(|(_, candidate)| {
            let app = apps.get(candidate.id as usize)?;
            let highlights = matcher
                .highlights(candidate, ranked.is_fuzzy)
                .into_iter()
                .map(|index| u32::try_from(index).unwrap_or(u32::MAX))
                .collect();
            let path = app.executable.as_ref().unwrap_or(&app.source);
            Some(serafind_ipc::Row {
                name: app.name.clone(),
                location: app
                    .executable
                    .as_ref()
                    .map_or_else(String::new, |program| shorten(program, home)),
                path: path.to_string_lossy().into_owned(),
                kind: serafind_ipc::Kind::App,
                highlights,
                app: Some(app.id.clone()),
                icon: app.icon.as_ref().map(|icon| icon.to_string_lossy().into_owned()),
                line: None,
            })
        })
        .collect();
    (rows, ranked.total, ranked.is_fuzzy)
}

/// The rows of the files and folders found.
fn file_rows(
    matcher: &mut Matcher,
    found: &serafind_index::SearchResults,
    index: &Index,
    home: &Path,
) -> Vec<serafind_ipc::Row> {
    found
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
                app: None,
                icon: None,
                line: None,
            }
        })
        .collect()
}

fn fill(outcome: &mut Outcome, results: Results) {
    if let Some(mode) = results.mode {
        outcome.mode = from_wire_mode(mode);
    }
    outcome.total = usize::try_from(results.total).unwrap_or(usize::MAX);
    outcome.is_fuzzy = results.fuzzy;
    outcome.outside_index = results.outside_index;
    outcome.indexing = results.indexing;
    outcome.searching = results.searching;
    outcome.progress = results.progress;
    outcome.problem = results.problem;
    outcome.rows = results
        .rows
        .into_iter()
        .map(|row| Row {
            name: row.name,
            location: row.location,
            kind: from_wire_kind(row.kind),
            path: PathBuf::from(row.path),
            highlights: row.highlights.into_iter().map(|index| index as usize).collect(),
            app: row.app,
            icon: row.icon.map(PathBuf::from),
            line: row.line,
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
        assert_eq!(outcome.rows, []);
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

    fn app(id: &str, name: &str) -> App {
        App {
            id: id.to_owned(),
            name: name.to_owned(),
            source: PathBuf::from(format!("/usr/share/applications/{id}")),
            executable: Some(PathBuf::from(format!(
                "/usr/bin/{}",
                id.trim_end_matches(".desktop")
            ))),
            icon: Some(PathBuf::from("/icons/app.svg")),
            launch: serafind_apps::Launch::Command {
                arguments: vec![id.to_owned()],
                terminal: false,
                folder: None,
            },
        }
    }

    fn apps() -> Vec<App> {
        vec![
            app("firefox.desktop", "Firefox"),
            app("org.gnome.Nautilus.desktop", "Files"),
            app("libreoffice-writer.desktop", "LibreOffice Writer"),
        ]
    }

    fn found(paths: &[&str], line: &str, mode: SearchMode, launches: Option<&Launches>) -> Results {
        let mut index = Index::new();
        index.add_root(Path::new("/home/me"));
        for path in paths {
            index.insert_path(&Path::new("/home/me").join(path), Kind::File);
        }
        let home = Path::new("/home/me");
        search(
            Some(&index),
            &apps(),
            launches,
            &understand(line, home),
            mode,
            10,
            home,
        )
    }

    fn names(results: &Results) -> Vec<&str> {
        results.rows.iter().map(|row| row.name.as_str()).collect()
    }

    #[test]
    fn applications_come_first_in_all_mode() {
        let results = found(&["files-backup.zip"], "files", SearchMode::All, None);
        assert_eq!(names(&results), ["Files", "files-backup.zip"]);
        assert_eq!(results.total, 2);
        let app = &results.rows[0];
        assert_eq!(app.kind, serafind_ipc::Kind::App);
        assert_eq!(app.app.as_deref(), Some("org.gnome.Nautilus.desktop"));
        assert_eq!(app.path, "/usr/bin/org.gnome.Nautilus");
        assert_eq!(app.location, "/usr/bin/org.gnome.Nautilus", "the program");
        assert_eq!(app.icon.as_deref(), Some("/icons/app.svg"));
        assert_eq!(app.highlights, [0, 1, 2, 3, 4]);
    }

    #[test]
    fn modes_and_filters_choose_apps_or_files() {
        let paths = ["firefox-notes.txt"];
        assert_eq!(
            names(&found(&paths, "firefox", SearchMode::Apps, None)),
            ["Firefox"]
        );
        assert_eq!(
            names(&found(&paths, "firefox", SearchMode::Files, None)),
            ["firefox-notes.txt"]
        );
        assert_eq!(
            names(&found(&paths, "firefox ext:txt", SearchMode::All, None)),
            ["firefox-notes.txt"]
        );
        // `in:` searches a folder (one that exists on every system): no applications there.
        let folder = std::env::temp_dir();
        let line = format!(
            "firefox in:\"{}\"",
            folder.display().to_string().trim_end_matches(['/', '\\'])
        );
        let outside = found(&paths, &line, SearchMode::All, None);
        assert_eq!(names(&outside), Vec::<&str>::new());
        assert!(outside.outside_index);
    }

    #[test]
    fn strict_matches_hide_the_fuzzy_ones_of_the_other_side() {
        // "wrter" only matches the app by fuzzy matching; the file matches strictly.
        let results = found(&["wrter.txt"], "wrter", SearchMode::All, None);
        assert_eq!(names(&results), ["wrter.txt"]);
        assert!(!results.fuzzy);
        let results = found(&[], "wrter", SearchMode::All, None);
        assert_eq!(names(&results), ["LibreOffice Writer"]);
        assert!(results.fuzzy);
    }

    #[test]
    fn applications_used_most_come_first() {
        let launches = Launches::from([("firefox.desktop".to_owned(), 5)]);
        // Files and Firefox start with "f" (same rank, shorter first); Writer only contains it.
        let plain = found(&[], "f", SearchMode::Apps, None);
        assert_eq!(names(&plain), ["Files", "Firefox", "LibreOffice Writer"]);
        let learned = found(&[], "f", SearchMode::Apps, Some(&launches));
        assert_eq!(names(&learned), ["Firefox", "Files", "LibreOffice Writer"]);
    }

    #[test]
    fn applications_without_an_index() {
        let home = Path::new("/home/me");
        let results = search(
            None,
            &apps(),
            None,
            &understand("fire", home),
            SearchMode::All,
            10,
            home,
        );
        assert_eq!(names(&results), ["Firefox"]);
        assert!(results.indexing);
        let results = search(
            None,
            &apps(),
            None,
            &understand("fire", home),
            SearchMode::Apps,
            10,
            home,
        );
        assert!(!results.indexing);
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
