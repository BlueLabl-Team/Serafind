//! The first walk: every root is walked in parallel, then the entries are added to the index.
//!
//! Symbolic links are indexed but never followed, mount points are not crossed, and `.gitignore`
//! files are not read: only the [`Exclusions`] apply.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use hashbrown::HashMap;
use ignore::{DirEntry, WalkBuilder, WalkState};

use crate::{EntryId, Exclusions, Index, Kind};

/// What the walk found, besides the index.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WalkReport {
    /// Entries added (roots included).
    pub entries: usize,
    /// Entries that could not be read (permissions, vanished while walking...).
    pub errors: usize,
    /// How long the walk and the indexing took.
    pub duration: Duration,
}

/// Walks `roots` and builds an index of them. Roots that do not exist are skipped.
#[must_use]
pub fn build(roots: &[PathBuf], exclusions: &Exclusions) -> (Index, WalkReport) {
    let started = Instant::now();
    let mut index = Index::new();
    let mut errors = 0;
    for root in roots.iter().filter(|root| root.is_dir()) {
        errors += add_tree(&mut index, root, exclusions);
    }
    index.shrink_to_fit();
    let report = WalkReport {
        entries: index.len(),
        errors,
        duration: started.elapsed(),
    };
    (index, report)
}

/// Walks `root` and adds it to the index as a root. Returns the number of unreadable entries.
pub fn add_tree(index: &mut Index, root: &Path, exclusions: &Exclusions) -> usize {
    let (found, errors) = walk(root, exclusions);
    let root_id = index.add_root(root);
    insert_found(index, root, root_id, found);
    errors
}

/// Walks the folder `folder` (already in the index, e.g. a folder just created or moved in) and
/// adds what is under it. Returns the number of unreadable entries.
pub fn add_subtree(index: &mut Index, folder: &Path, exclusions: &Exclusions) -> usize {
    let Some(folder_id) = index.find(folder) else {
        return 0;
    };
    let (found, errors) = walk(folder, exclusions);
    insert_found(index, folder, folder_id, found);
    errors
}

struct Found {
    path: PathBuf,
    depth: usize,
    kind: Kind,
}

fn walk(root: &Path, exclusions: &Exclusions) -> (Vec<Found>, usize) {
    let threads = std::thread::available_parallelism().map_or(4, |count| count.get().min(8));
    let filter_exclusions = exclusions.clone();
    let walker = WalkBuilder::new(root)
        .standard_filters(false)
        .follow_links(false)
        .same_file_system(true)
        .threads(threads)
        .filter_entry(move |entry| !is_excluded(entry, &filter_exclusions))
        .build_parallel();

    let (sender, receiver) = mpsc::channel::<Result<Found, ()>>();
    walker.run(|| {
        let sender = sender.clone();
        Box::new(move |result| {
            let message = match result {
                Ok(entry) if entry.depth() == 0 => return WalkState::Continue,
                Ok(entry) => Ok(Found {
                    kind: if entry.file_type().is_some_and(|kind| kind.is_dir()) {
                        Kind::Folder
                    } else {
                        Kind::File
                    },
                    depth: entry.depth(),
                    path: entry.into_path(),
                }),
                Err(_) => Err(()),
            };
            if sender.send(message).is_err() {
                return WalkState::Quit;
            }
            WalkState::Continue
        })
    });
    drop(sender);

    let mut found = Vec::new();
    let mut errors = 0;
    for message in receiver {
        match message {
            Ok(entry) => found.push(entry),
            Err(()) => errors += 1,
        }
    }
    (found, errors)
}

/// Adds walked entries under `base` (whose id is `base_id`), parents before children.
fn insert_found(index: &mut Index, base: &Path, base_id: EntryId, mut found: Vec<Found>) {
    found.sort_unstable_by_key(|entry| entry.depth);
    let mut folders: HashMap<PathBuf, EntryId> = HashMap::new();
    folders.insert(base.to_path_buf(), base_id);
    for entry in found {
        let (Some(parent_path), Some(name)) = (entry.path.parent(), entry.path.file_name()) else {
            continue;
        };
        let Some(&parent) = folders.get(parent_path) else {
            continue;
        };
        let id = index.insert(parent, name, entry.kind);
        if entry.kind == Kind::Folder {
            folders.insert(entry.path, id);
        }
    }
}

fn is_excluded(entry: &DirEntry, exclusions: &Exclusions) -> bool {
    entry.depth() > 0
        && entry.file_type().is_some_and(|kind| kind.is_dir())
        && exclusions.excludes_folder(entry.path(), is_system_hidden(entry))
}

#[cfg(windows)]
fn is_system_hidden(entry: &DirEntry) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    entry
        .metadata()
        .is_ok_and(|metadata| metadata.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0)
}

#[cfg(not(windows))]
fn is_system_hidden(_entry: &DirEntry) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_fs::TempDir;
    use assert_fs::prelude::*;

    fn tree() -> TempDir {
        let root = TempDir::new().unwrap();
        for file in [
            "Documents/invoice-2026.pdf",
            "Documents/Taxes/2025/form.pdf",
            "Projects/app/src/main.rs",
            "Projects/app/node_modules/left-pad/index.js",
            "Projects/app/target/debug/app",
            ".config/app/settings.toml",
            ".bashrc",
            "notes.txt",
        ] {
            root.child(file).touch().unwrap();
        }
        root.child("Empty").create_dir_all().unwrap();
        root
    }

    fn indexed_paths(index: &Index, root: &Path) -> Vec<String> {
        let mut paths: Vec<String> = index
            .ids()
            .map(|id| {
                let path = index.path_of(id);
                path.strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();
        paths.sort();
        paths
    }

    #[test]
    fn walks_everything_but_the_exclusions() {
        let root = tree();
        let (index, report) = build(&[root.to_path_buf()], &Exclusions::defaults(root.path()));
        assert_eq!(
            indexed_paths(&index, root.path()),
            [
                ".bashrc",
                "Documents",
                "Documents/Taxes",
                "Documents/Taxes/2025",
                "Documents/Taxes/2025/form.pdf",
                "Documents/invoice-2026.pdf",
                "Empty",
                "Projects",
                "Projects/app",
                "Projects/app/src",
                "Projects/app/src/main.rs",
                "notes.txt",
            ]
        );
        assert_eq!(report.entries, 13, "12 entries and the root");
        assert_eq!(report.errors, 0);
    }

    #[test]
    fn without_exclusions_everything_is_there() {
        let root = tree();
        let (index, _) = build(&[root.to_path_buf()], &Exclusions::none());
        let paths = indexed_paths(&index, root.path());
        assert!(paths.contains(&".config/app/settings.toml".to_owned()));
        assert!(paths.contains(&"Projects/app/node_modules/left-pad/index.js".to_owned()));
    }

    #[test]
    fn kinds_are_recorded() {
        let root = tree();
        let (index, _) = build(&[root.to_path_buf()], &Exclusions::defaults(root.path()));
        let kind = |relative: &str| {
            index
                .find(&root.path().join(relative))
                .and_then(|id| index.kind(id))
        };
        assert_eq!(kind("Documents"), Some(Kind::Folder));
        assert_eq!(kind("notes.txt"), Some(Kind::File));
        assert_eq!(kind("Projects/app/target"), None);
    }

    #[test]
    fn missing_roots_are_skipped() {
        let (index, report) = build(&[PathBuf::from("/definitely/not/here")], &Exclusions::none());
        assert!(index.is_empty());
        assert_eq!(report.entries, 0);
    }

    #[cfg(unix)]
    #[test]
    fn symbolic_links_are_indexed_not_followed() {
        let root = tree();
        std::os::unix::fs::symlink(root.path().join("Documents"), root.path().join("docs-link")).unwrap();
        let (index, _) = build(&[root.to_path_buf()], &Exclusions::defaults(root.path()));
        let link = index
            .find(&root.path().join("docs-link"))
            .expect("the link is indexed");
        assert_eq!(index.kind(link), Some(Kind::File));
        assert_eq!(index.find(&root.path().join("docs-link/invoice-2026.pdf")), None);
    }

    #[test]
    fn subtrees_can_be_added_later() {
        let root = tree();
        let (mut index, _) = build(&[root.to_path_buf()], &Exclusions::defaults(root.path()));
        root.child("Music/Album/track.flac").touch().unwrap();
        index.insert_path(&root.path().join("Music"), Kind::Folder);
        add_subtree(
            &mut index,
            &root.path().join("Music"),
            &Exclusions::defaults(root.path()),
        );
        assert!(index.find(&root.path().join("Music/Album/track.flac")).is_some());
    }
}
