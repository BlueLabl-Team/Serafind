//! Keeps the index up to date: filesystem events are grouped (about 250 ms), then applied.
//!
//! On Linux, inotify needs one watch per folder: only the indexed folders are watched, not the
//! excluded ones, and watches follow folders as they appear and disappear. On Windows (and other
//! systems) each root is watched recursively and events under excluded folders are dropped.
//!
//! When the system reports lost events (queue overflow), the folder is walked again.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex, RwLock};
use std::thread::JoinHandle;
use std::time::Duration;

use notify::event::{ModifyKind, RenameMode};
use notify::{EventKind, RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{
    DebounceEventResult, DebouncedEvent, Debouncer, RecommendedCache, new_debouncer,
};

use crate::{Exclusions, Index, Kind, add_subtree, build};

/// Watch one folder at a time (inotify) rather than whole trees.
const WATCH_EACH_FOLDER: bool = cfg!(target_os = "linux");

/// How long events are grouped before being applied, by default.
pub const DEFAULT_DELAY: Duration = Duration::from_millis(250);

/// An index kept up to date by watching the filesystem. Searches take a read lock on [`index`].
///
/// [`index`]: LiveIndex::index
pub struct LiveIndex {
    index: Arc<RwLock<Index>>,
    status: Arc<Mutex<WatchStatus>>,
    stop: Option<mpsc::Sender<()>>,
    worker: Option<JoinHandle<()>>,
}

/// How the watching goes, for diagnostics.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WatchStatus {
    /// Folders currently watched (one per root on systems with recursive watching).
    pub watched: usize,
    /// Folders that could not be watched (inotify limit reached, permissions...).
    pub watch_errors: usize,
    /// Batches of changes applied so far.
    pub batches: usize,
    /// Times a folder had to be walked again because events were lost.
    pub rescans: usize,
}

struct Shared {
    roots: Vec<PathBuf>,
    exclusions: Exclusions,
    index: Arc<RwLock<Index>>,
    status: Arc<Mutex<WatchStatus>>,
    debouncer: Debouncer<RecommendedWatcher, RecommendedCache>,
}

impl LiveIndex {
    /// Walks `roots`, then watches them. Events are grouped for `delay` before being applied.
    ///
    /// # Errors
    /// When the system watcher cannot be created.
    pub fn start(roots: Vec<PathBuf>, exclusions: Exclusions, delay: Duration) -> notify::Result<Self> {
        let (events_in, events) = mpsc::channel();
        let debouncer = new_debouncer(delay, None, move |result: DebounceEventResult| {
            // The worker may be gone while stopping: nothing to do then.
            let _ = events_in.send(result);
        })
        .map_err(|error| notify::Error::generic(&error.to_string()))?;

        let (index, _report) = build(&roots, &exclusions);
        let index = Arc::new(RwLock::new(index));
        let status = Arc::new(Mutex::new(WatchStatus::default()));
        let mut shared = Shared {
            roots,
            exclusions,
            index: Arc::clone(&index),
            status: Arc::clone(&status),
            debouncer,
        };
        shared.watch_all();

        let (stop, stopped) = mpsc::channel();
        let worker = std::thread::Builder::new()
            .name("serafind-index watcher".to_owned())
            .spawn(move || run(&mut shared, &events, &stopped))
            .map_err(|error| notify::Error::generic(&error.to_string()))?;
        Ok(Self {
            index,
            status,
            stop: Some(stop),
            worker: Some(worker),
        })
    }

    /// The index, shared with the watcher thread.
    #[must_use]
    pub fn index(&self) -> &Arc<RwLock<Index>> {
        &self.index
    }

    /// How the watching goes.
    ///
    /// # Panics
    /// When the watcher thread panicked while holding the status.
    #[must_use]
    pub fn status(&self) -> WatchStatus {
        self.status.lock().expect("watch status lock").clone()
    }
}

impl Drop for LiveIndex {
    fn drop(&mut self) {
        drop(self.stop.take());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn run(shared: &mut Shared, events: &Receiver<DebounceEventResult>, stopped: &Receiver<()>) {
    loop {
        match stopped.try_recv() {
            Err(mpsc::TryRecvError::Empty) => {}
            _ => return,
        }
        match events.recv_timeout(Duration::from_millis(100)) {
            Ok(Ok(batch)) => shared.apply(&batch),
            // Errors without events (a watched folder vanished...): the events say what changed.
            Ok(Err(_)) | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

/// What a batch of events asks the index to do, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Change {
    Created(PathBuf),
    Removed(PathBuf),
    Renamed(PathBuf, PathBuf),
    Rescan(PathBuf),
}

impl Shared {
    fn watch_all(&mut self) {
        let folders: Vec<PathBuf> = if WATCH_EACH_FOLDER {
            let index = self.index.read().expect("index lock");
            let mut folders = self.roots.clone();
            folders.extend(
                index
                    .ids()
                    .filter(|&id| index.kind(id) == Some(Kind::Folder))
                    .map(|id| index.path_of(id)),
            );
            folders
        } else {
            self.roots.clone()
        };
        self.watch(&folders);
    }

    fn watch(&mut self, folders: &[PathBuf]) {
        let mode = if WATCH_EACH_FOLDER {
            RecursiveMode::NonRecursive
        } else {
            RecursiveMode::Recursive
        };
        let (mut watched, mut errors) = (0, 0);
        for folder in folders {
            match self.debouncer.watch(folder, mode) {
                Ok(()) => watched += 1,
                Err(_) => errors += 1,
            }
        }
        let mut status = self.status.lock().expect("watch status lock");
        status.watched += watched;
        status.watch_errors += errors;
    }

    /// Stops watching a folder that left the index while still on disk. (The system drops the
    /// watches of deleted folders itself.)
    fn unwatch(&mut self, folder: &Path) {
        if WATCH_EACH_FOLDER && self.debouncer.unwatch(folder).is_ok() {
            let mut status = self.status.lock().expect("watch status lock");
            status.watched = status.watched.saturating_sub(1);
        }
    }

    fn apply(&mut self, batch: &[DebouncedEvent]) {
        let changes = changes_of(batch);
        if changes.is_empty() {
            return;
        }
        let shared_index = Arc::clone(&self.index);
        let mut index = shared_index.write().expect("index lock");
        for change in changes {
            match change {
                Change::Created(path) => self.create(&mut index, &path),
                Change::Removed(path) => {
                    index.remove_path(&path);
                }
                Change::Renamed(from, to) => self.rename(&mut index, &from, &to),
                Change::Rescan(path) => self.rescan(&mut index, &path),
            }
        }
        index.compact_if_needed();
        drop(index);
        self.status.lock().expect("watch status lock").batches += 1;
    }

    /// Adds what appeared at `path`; a folder is watched and walked.
    fn create(&mut self, index: &mut Index, path: &Path) {
        let Ok(metadata) = fs::symlink_metadata(path) else {
            return; // Already gone again.
        };
        let is_folder = metadata.is_dir();
        if self.is_excluded(path, is_folder) {
            return;
        }
        let kind = if is_folder { Kind::Folder } else { Kind::File };
        if index.insert_path(path, kind).is_some() && is_folder {
            self.adopt(index, path);
        }
    }

    fn rename(&mut self, index: &mut Index, from: &Path, to: &Path) {
        let is_folder = fs::symlink_metadata(to).is_ok_and(|metadata| metadata.is_dir());
        if self.is_excluded(to, is_folder) {
            // Moved into an excluded place (or renamed to a hidden name): gone from the index.
            for folder in folders_under(index, from) {
                self.unwatch(&folder);
            }
            index.remove_path(from);
            return;
        }
        if index.find(from).is_some() && index.rename_path(from, to).is_some() {
            if is_folder {
                // Watches are tied to paths: watch the folders at their new place, and walk again
                // for what was created there while events still carried the old paths.
                self.adopt(index, to);
            }
        } else {
            // Moved in from outside the index (or from an excluded place).
            self.create(index, to);
        }
    }

    /// Clears what is under the folder of `path` and walks it again (events were lost).
    fn rescan(&mut self, index: &mut Index, path: &Path) {
        let folder = if path.is_dir() {
            path
        } else {
            path.parent().unwrap_or(path)
        };
        index.clear_under(folder);
        self.adopt(index, folder);
        self.status.lock().expect("watch status lock").rescans += 1;
    }

    /// Walks a folder of the index. With one watch per folder, it also watches the folder and
    /// what is under it, and walks again until no new folder appears: whatever was created before
    /// its watch existed is caught by the walk. (Recursive watches already cover new folders.)
    fn adopt(&mut self, index: &mut Index, folder: &Path) {
        if !WATCH_EACH_FOLDER {
            add_subtree(index, folder, &self.exclusions);
            return;
        }
        let mut watched: Vec<PathBuf> = Vec::new();
        // A few rounds are enough: each one only has to catch folders created during the last.
        for _ in 0..8 {
            add_subtree(index, folder, &self.exclusions);
            let unwatched: Vec<PathBuf> = folders_under(index, folder)
                .into_iter()
                .filter(|path| !watched.contains(path))
                .collect();
            if unwatched.is_empty() {
                return;
            }
            self.watch(&unwatched);
            watched.extend(unwatched);
        }
    }

    /// True when `path` (or a folder above it, up to its root) is excluded.
    fn is_excluded(&self, path: &Path, is_folder: bool) -> bool {
        let Some(root) = self
            .roots
            .iter()
            .filter(|root| path.starts_with(root))
            .max_by_key(|root| root.as_os_str().len())
        else {
            return true;
        };
        if is_folder && self.exclusions.excludes_folder(path, is_system_hidden(path)) {
            return true;
        }
        path.ancestors()
            .skip(1)
            .take_while(|ancestor| ancestor.starts_with(root) && *ancestor != root.as_path())
            .any(|ancestor| self.exclusions.excludes_folder(ancestor, false))
    }
}

fn folders_under(index: &Index, path: &Path) -> Vec<PathBuf> {
    let Some(folder) = index.find(path) else {
        return Vec::new();
    };
    let mut folders = vec![path.to_path_buf()];
    folders.extend(
        index
            .ids_under(folder)
            .filter(|&id| index.kind(id) == Some(Kind::Folder))
            .map(|id| index.path_of(id)),
    );
    folders
}

/// Turns grouped filesystem events into index changes.
fn changes_of(batch: &[DebouncedEvent]) -> Vec<Change> {
    let mut changes = Vec::new();
    for event in batch {
        if event.need_rescan() {
            if let Some(path) = event.paths.first() {
                changes.push(Change::Rescan(path.clone()));
            }
            continue;
        }
        match event.kind {
            EventKind::Create(_) => changes.extend(event.paths.iter().cloned().map(Change::Created)),
            EventKind::Remove(_) => changes.extend(event.paths.iter().cloned().map(Change::Removed)),
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)) if event.paths.len() == 2 => {
                changes.push(Change::Renamed(event.paths[0].clone(), event.paths[1].clone()));
            }
            EventKind::Modify(ModifyKind::Name(RenameMode::From)) => {
                changes.extend(event.paths.iter().cloned().map(Change::Removed));
            }
            EventKind::Modify(ModifyKind::Name(RenameMode::To)) => {
                changes.extend(event.paths.iter().cloned().map(Change::Created));
            }
            EventKind::Modify(ModifyKind::Name(_)) => {
                // Unknown rename side: check what is on disk now.
                for path in &event.paths {
                    changes.push(if path.exists() {
                        Change::Created(path.clone())
                    } else {
                        Change::Removed(path.clone())
                    });
                }
            }
            // Content and metadata changes do not change names: nothing to do.
            _ => {}
        }
    }
    changes
}

#[cfg(windows)]
fn is_system_hidden(path: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0)
}

#[cfg(not(windows))]
fn is_system_hidden(_path: &Path) -> bool {
    false
}

#[cfg(test)]
mod tests;
