use std::path::{Path, PathBuf};

use proptest::prelude::*;
use serafind_core::{MatchRank, ParseContext, parse};

use super::*;

const ROOT: &str = "/home/me";

fn index_of(paths: &[&str]) -> Index {
    let mut index = Index::new();
    index.add_root(Path::new(ROOT));
    for path in paths {
        let kind = if path.ends_with('/') {
            Kind::Folder
        } else {
            Kind::File
        };
        index
            .insert_path(&Path::new(ROOT).join(path.trim_end_matches('/')), kind)
            .unwrap();
    }
    index
}

fn query(line: &str) -> Query {
    let context = ParseContext {
        home: Path::new(ROOT),
        current_folder: Path::new(ROOT),
        folder_exists: &|_| true,
    };
    parse(line, &context)
}

fn search(index: &Index, line: &str) -> Vec<String> {
    index
        .search(&query(line), SearchMode::All, 50)
        .hits
        .iter()
        .map(|hit| relative(index, hit.id))
        .collect()
}

/// The path under the root, with `/` between folders on every system.
fn relative(index: &Index, id: EntryId) -> String {
    index
        .path_of(id)
        .strip_prefix(ROOT)
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/")
}

fn all(index: &Index) -> Vec<String> {
    let mut paths: Vec<String> = index.ids().map(|id| relative(index, id)).collect();
    paths.sort();
    paths
}

// --- Building and finding ---

#[test]
fn missing_parent_folders_are_created() {
    let index = index_of(&["Documents/Taxes/2025/form.pdf"]);
    assert_eq!(
        all(&index),
        [
            "Documents",
            "Documents/Taxes",
            "Documents/Taxes/2025",
            "Documents/Taxes/2025/form.pdf"
        ]
    );
    let folder = index.find(Path::new("/home/me/Documents/Taxes")).unwrap();
    assert_eq!(index.kind(folder), Some(Kind::Folder));
}

#[test]
fn paths_are_rebuilt_from_parents() {
    let index = index_of(&["a/b/c.txt"]);
    let id = index.find(Path::new("/home/me/a/b/c.txt")).unwrap();
    assert_eq!(index.path_of(id), PathBuf::from("/home/me/a/b/c.txt"));
    assert_eq!(index.name(id), "c.txt");
}

#[test]
fn inserting_twice_keeps_one_entry() {
    let mut index = index_of(&["notes.txt"]);
    let before = index.len();
    index.insert_path(Path::new("/home/me/notes.txt"), Kind::File);
    assert_eq!(index.len(), before);
}

#[test]
fn paths_outside_the_roots_are_refused() {
    let mut index = index_of(&[]);
    assert_eq!(index.insert_path(Path::new("/etc/passwd"), Kind::File), None);
    assert_eq!(index.find(Path::new("/etc")), None);
}

#[test]
fn deepest_root_wins() {
    let mut index = index_of(&["Projects/"]);
    let inner = index.add_root(Path::new("/home/me/Projects/serafind"));
    let file = index
        .insert_path(Path::new("/home/me/Projects/serafind/README.md"), Kind::File)
        .unwrap();
    assert_eq!(
        index.path_of(file),
        PathBuf::from("/home/me/Projects/serafind/README.md")
    );
    assert_eq!(index.entries[file as usize].parent, inner);
}

// --- Removing and renaming ---

#[test]
fn removing_a_folder_removes_what_is_under_it() {
    let mut index = index_of(&["Documents/a.pdf", "Documents/Taxes/b.pdf", "notes.txt"]);
    assert_eq!(index.remove_path(Path::new("/home/me/Documents")), 4);
    assert_eq!(all(&index), ["notes.txt"]);
    assert_eq!(index.find(Path::new("/home/me/Documents/a.pdf")), None);
}

#[test]
fn roots_are_not_removed() {
    let mut index = index_of(&["notes.txt"]);
    assert_eq!(index.remove_path(Path::new(ROOT)), 0);
    assert_eq!(index.remove_path(Path::new("/home/me/nothing")), 0);
}

#[test]
fn renaming_a_folder_moves_its_content() {
    let mut index = index_of(&["Docs/Taxes/form.pdf", "Archive/"]);
    index
        .rename_path(Path::new("/home/me/Docs"), Path::new("/home/me/Archive/Old docs"))
        .unwrap();
    assert_eq!(
        all(&index),
        [
            "Archive",
            "Archive/Old docs",
            "Archive/Old docs/Taxes",
            "Archive/Old docs/Taxes/form.pdf"
        ]
    );
    let form = index
        .find(Path::new("/home/me/Archive/Old docs/Taxes/form.pdf"))
        .unwrap();
    assert_eq!(index.entries[form as usize].depth, 4);
    assert_eq!(search(&index, "old docs"), ["Archive/Old docs"]);
}

#[test]
fn renaming_over_an_entry_replaces_it() {
    let mut index = index_of(&["a.txt", "b.txt"]);
    index.rename_path(Path::new("/home/me/a.txt"), Path::new("/home/me/b.txt"));
    assert_eq!(all(&index), ["b.txt"]);
}

#[test]
fn renaming_to_itself_changes_nothing() {
    let mut index = index_of(&["a.txt"]);
    assert!(
        index
            .rename_path(Path::new("/home/me/a.txt"), Path::new("/home/me/a.txt"))
            .is_some()
    );
    assert_eq!(all(&index), ["a.txt"]);
}

#[test]
fn renaming_out_of_the_roots_removes() {
    let mut index = index_of(&["a.txt"]);
    assert_eq!(
        index.rename_path(Path::new("/home/me/a.txt"), Path::new("/tmp/a.txt")),
        None
    );
    assert!(all(&index).is_empty());
}

#[test]
fn compaction_keeps_live_entries() {
    let names: Vec<String> = (0..3000).map(|n| format!("dir{}/file{n}.txt", n % 7)).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let mut index = index_of(&refs);
    index.rename_path(Path::new("/home/me/dir6"), Path::new("/home/me/moved"));
    for n in 0..5 {
        index.remove_path(Path::new(&format!("/home/me/dir{n}")));
    }
    let before = all(&index);
    assert!(index.compact_if_needed());
    assert_eq!(all(&index), before);
    assert_eq!(index.removed, 0);
    assert!(index.find(Path::new("/home/me/moved/file6.txt")).is_some());
    assert!(!index.compact_if_needed());
}

// --- Searching ---

#[test]
fn search_uses_folded_names() {
    let index = index_of(&["Facturé EDF.pdf", "photo.jpg"]);
    assert_eq!(search(&index, "facture edf"), ["Facturé EDF.pdf"]);
    assert_eq!(search(&index, "EDF"), ["Facturé EDF.pdf"]);
    assert_eq!(search(&index, "edf ext:jpg"), Vec::<String>::new());
}

#[test]
fn search_by_mode_and_folder() {
    let index = index_of(&["Documents/report.pdf", "Projects/report/", "report.txt"]);
    let mut files = index.search(&query("report f:"), SearchMode::All, 10).hits;
    files.sort_by_key(|hit| hit.id);
    assert_eq!(files.len(), 2);
    assert_eq!(search(&index, "report d:"), ["Projects/report"]);
    assert_eq!(search(&index, "report in:~/Documents"), ["Documents/report.pdf"]);
}

#[test]
fn start_folder_outside_the_index() {
    let index = index_of(&["report.txt"]);
    let results = index.search(&query("report in:/etc"), SearchMode::All, 10);
    assert!(results.outside_index);
    assert!(results.hits.is_empty());
}

#[test]
fn roots_are_never_results() {
    let index = index_of(&["notes.txt"]);
    assert_eq!(search(&index, "me"), Vec::<String>::new());
}

#[test]
fn apps_mode_finds_no_files() {
    let index = index_of(&["firefox.desktop"]);
    assert!(
        index
            .search(&query("firefox"), SearchMode::Apps, 10)
            .hits
            .is_empty()
    );
}

#[test]
fn fuzzy_fallback_through_the_index() {
    let index = index_of(&["facture-2026.pdf"]);
    let results = index.search(&query("fctr"), SearchMode::All, 10);
    assert!(results.is_fuzzy);
    assert_eq!(results.hits[0].score.match_rank(), MatchRank::Fuzzy);
}

#[cfg(unix)]
#[test]
fn names_that_are_not_unicode_are_kept_exactly() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    let mut index = index_of(&[]);
    let name = OsStr::from_bytes(b"caf\xe9.txt");
    let path = Path::new(ROOT).join(name);
    let id = index.insert_path(&path, Kind::File).unwrap();
    assert_eq!(index.path_of(id), path);
    assert_eq!(index.find(&path), Some(id));
    assert_eq!(index.name(id), "caf\u{fffd}.txt");
}

// --- Properties ---

proptest! {
    #[test]
    fn every_inserted_path_is_found_back(
        paths in prop::collection::vec("[a-c]{1,2}(/[a-c]{1,2}){0,3}", 1..30)
    ) {
        let mut index = index_of(&[]);
        for path in &paths {
            let full = Path::new(ROOT).join(path);
            // A path may already be a folder of an earlier path: insert as folder then.
            let kind = if index.find(&full).is_some_and(|id| index.kind(id) == Some(Kind::Folder)) {
                Kind::Folder
            } else {
                Kind::File
            };
            if paths.iter().any(|other| other.starts_with(&format!("{path}/"))) {
                index.insert_path(&full, Kind::Folder);
            } else {
                index.insert_path(&full, kind);
            }
        }
        for path in &paths {
            let full = Path::new(ROOT).join(path);
            let id = index.find(&full);
            prop_assert!(id.is_some());
            prop_assert_eq!(index.path_of(id.unwrap()), full);
        }
    }
}

#[test]
fn content_candidates_are_files_with_the_file_filters() {
    let index = index_of(&[
        "notes.txt",
        "Projects/app/main.rs",
        "Projects/app/README.md",
        "Projects/",
        "photo.jpg",
    ]);
    let root = PathBuf::from("/home/me");
    let candidates = |line: &str| {
        let query = parse(
            line,
            &ParseContext {
                home: &root,
                current_folder: &root,
                folder_exists: &|_| true,
            },
        );
        index.content_candidates(&query).map(|files| {
            files
                .iter()
                .map(|path| {
                    path.strip_prefix(&root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/")
                })
                .collect::<Vec<_>>()
        })
    };
    assert_eq!(
        candidates("todo").unwrap(),
        [
            "notes.txt",
            "photo.jpg",
            "Projects/app/main.rs",
            "Projects/app/README.md"
        ]
    );
    assert_eq!(
        candidates("todo ext:rs,md -ext:md").unwrap(),
        ["Projects/app/main.rs"]
    );
    assert_eq!(
        candidates("todo in:~/Projects").unwrap(),
        ["Projects/app/main.rs", "Projects/app/README.md"]
    );
    assert_eq!(candidates("todo in:/elsewhere"), None);
}
