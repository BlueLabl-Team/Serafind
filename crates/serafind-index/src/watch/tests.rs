//! Real filesystem tests: changes are made on disk, then the index is polled until it follows.

use std::fs;
use std::time::Instant;

use assert_fs::TempDir;
use assert_fs::prelude::*;

use super::*;

const DELAY: Duration = Duration::from_millis(50);
const PATIENCE: Duration = Duration::from_secs(5);

fn start(root: &TempDir) -> LiveIndex {
    LiveIndex::start(vec![root.to_path_buf()], Exclusions::defaults(root.path()), DELAY).unwrap()
}

/// Waits until `condition` holds on the index, or fails after a few seconds.
fn eventually(live: &LiveIndex, what: &str, condition: impl Fn(&Index) -> bool) {
    let started = Instant::now();
    while started.elapsed() < PATIENCE {
        if condition(&live.index().read().unwrap()) {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("the index never got there: {what} (status: {:?})", live.status());
}

fn has(index: &Index, root: &TempDir, relative: &str) -> bool {
    index.find(&root.path().join(relative)).is_some()
}

#[test]
fn initial_walk_and_watches() {
    let root = TempDir::new().unwrap();
    root.child("Documents/a.txt").touch().unwrap();
    root.child("node_modules/x/y.js").touch().unwrap();
    let live = start(&root);
    let index = live.index().read().unwrap();
    assert!(has(&index, &root, "Documents/a.txt"));
    assert!(!has(&index, &root, "node_modules"));
    let status = live.status();
    assert_eq!(status.watch_errors, 0);
    if WATCH_EACH_FOLDER {
        assert_eq!(status.watched, 2, "the root and Documents, not node_modules");
    } else {
        assert_eq!(status.watched, 1, "the root, recursively");
    }
}

#[test]
fn created_files_and_folders_appear() {
    let root = TempDir::new().unwrap();
    let live = start(&root);
    root.child("notes.txt").touch().unwrap();
    eventually(&live, "notes.txt", |index| has(index, &root, "notes.txt"));

    // A whole tree at once: the folder is walked, then watched.
    fs::create_dir_all(root.path().join("Music/Album")).unwrap();
    root.child("Music/Album/track.flac").touch().unwrap();
    eventually(&live, "track.flac", |index| {
        has(index, &root, "Music/Album/track.flac")
    });

    // And files created later in the new folder are seen too.
    root.child("Music/Album/cover.jpg").touch().unwrap();
    eventually(&live, "cover.jpg", |index| {
        has(index, &root, "Music/Album/cover.jpg")
    });
}

#[test]
fn removed_entries_disappear() {
    let root = TempDir::new().unwrap();
    root.child("Old/a.txt").touch().unwrap();
    root.child("b.txt").touch().unwrap();
    let live = start(&root);
    fs::remove_file(root.path().join("b.txt")).unwrap();
    fs::remove_dir_all(root.path().join("Old")).unwrap();
    eventually(&live, "b.txt and Old gone", |index| {
        !has(index, &root, "b.txt") && !has(index, &root, "Old") && !has(index, &root, "Old/a.txt")
    });
}

#[test]
fn renamed_folders_keep_their_content_and_watches() {
    let root = TempDir::new().unwrap();
    root.child("Docs/Taxes/form.pdf").touch().unwrap();
    let live = start(&root);
    fs::rename(root.path().join("Docs"), root.path().join("Papers")).unwrap();
    eventually(&live, "Papers/Taxes/form.pdf", |index| {
        has(index, &root, "Papers/Taxes/form.pdf") && !has(index, &root, "Docs")
    });

    // The moved folders are still watched at their new place.
    root.child("Papers/Taxes/receipt.pdf").touch().unwrap();
    eventually(&live, "Papers/Taxes/receipt.pdf", |index| {
        has(index, &root, "Papers/Taxes/receipt.pdf")
    });
}

#[test]
fn excluded_places_stay_out() {
    let root = TempDir::new().unwrap();
    root.child("app/src/main.rs").touch().unwrap();
    let live = start(&root);
    fs::create_dir_all(root.path().join("app/node_modules/pkg")).unwrap();
    root.child("app/node_modules/pkg/index.js").touch().unwrap();
    fs::create_dir_all(root.path().join(".hidden-folder")).unwrap();
    root.child(".bashrc").touch().unwrap();
    // .bashrc is a hidden file: indexed. Once it is there, the other events were handled too.
    eventually(&live, ".bashrc", |index| has(index, &root, ".bashrc"));
    let index = live.index().read().unwrap();
    assert!(!has(&index, &root, "app/node_modules"));
    assert!(!has(&index, &root, ".hidden-folder"));
}

#[test]
fn renaming_to_a_hidden_name_removes() {
    let root = TempDir::new().unwrap();
    root.child("Visible/a.txt").touch().unwrap();
    let live = start(&root);
    fs::rename(root.path().join("Visible"), root.path().join(".Visible")).unwrap();
    eventually(&live, "Visible gone", |index| !has(index, &root, "Visible"));
    assert!(!has(&live.index().read().unwrap(), &root, ".Visible"));
}

#[test]
fn moved_in_from_outside_is_added() {
    let outside = TempDir::new().unwrap();
    outside.child("Imported/data.csv").touch().unwrap();
    let root = TempDir::new().unwrap();
    let live = start(&root);
    // Same file system (both in the temp folder), so this is a real move.
    fs::rename(outside.path().join("Imported"), root.path().join("Imported")).unwrap();
    eventually(&live, "Imported/data.csv", |index| {
        has(index, &root, "Imported/data.csv")
    });
}

#[test]
fn changes_of_events() {
    use notify::event::{CreateKind, Event, RemoveKind};
    let event = |kind, paths: &[&str]| {
        DebouncedEvent::new(
            Event {
                kind,
                paths: paths.iter().map(PathBuf::from).collect(),
                attrs: notify::event::EventAttributes::default(),
            },
            Instant::now(),
        )
    };
    let changes = changes_of(&[
        event(EventKind::Create(CreateKind::File), &["/r/a"]),
        event(EventKind::Remove(RemoveKind::Any), &["/r/b"]),
        event(
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
            &["/r/c", "/r/d"],
        ),
        event(
            EventKind::Modify(ModifyKind::Data(notify::event::DataChange::Any)),
            &["/r/e"],
        ),
        event(EventKind::Modify(ModifyKind::Name(RenameMode::From)), &["/r/f"]),
        event(EventKind::Modify(ModifyKind::Name(RenameMode::To)), &["/r/g"]),
    ]);
    assert_eq!(
        changes,
        [
            Change::Created("/r/a".into()),
            Change::Removed("/r/b".into()),
            Change::Renamed("/r/c".into(), "/r/d".into()),
            Change::Removed("/r/f".into()),
            Change::Created("/r/g".into()),
        ]
    );
}
