//! Says when `config.toml` or a theme changes, so the settings can be read again.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{DebounceEventResult, Debouncer, RecommendedCache, new_debouncer};

/// Watches the settings folder (and its themes) while alive.
pub struct SettingsWatcher {
    _debouncer: Debouncer<RecommendedWatcher, RecommendedCache>,
}

impl SettingsWatcher {
    /// Calls `on_change` (from a background thread) shortly after a `.toml` file of `dir` or of
    /// its themes folder is written, created, removed or renamed. Editors often write a file in
    /// several steps: changes are grouped for `delay`.
    ///
    /// # Errors
    /// When `dir` cannot be watched (it must exist).
    pub fn start(dir: &Path, delay: Duration, on_change: impl Fn() + Send + 'static) -> notify::Result<Self> {
        let folder = dir.to_path_buf();
        let known = Mutex::new(fingerprint(&folder));
        let mut debouncer = new_debouncer(delay, None, move |result: DebounceEventResult| {
            let Ok(events) = result else {
                return;
            };
            // Reading a file (opening it) is not a change: without this, loading the settings
            // would report a change, which would load them again, forever.
            let touches_settings = events
                .iter()
                .filter(|event| !matches!(event.kind, notify::EventKind::Access(_)))
                .flat_map(|event| event.paths.iter())
                .any(|path| is_settings_file(path));
            if !touches_settings {
                return;
            }
            // Only a real change counts: some systems report a read as a change of the file
            // (its access time), which would also load the settings again, forever.
            let now = fingerprint(&folder);
            let changed = known.lock().is_ok_and(|mut known| {
                let changed = *known != now;
                *known = now;
                changed
            });
            if changed {
                on_change();
            }
        })
        .map_err(|error| notify::Error::generic(&error.to_string()))?;
        // The folder is small: watch it with its themes folder, even one created later.
        debouncer.watch(dir, RecursiveMode::Recursive)?;
        Ok(Self {
            _debouncer: debouncer,
        })
    }
}

/// The settings files with their modification time and size, sorted: what tells a real change.
fn fingerprint(dir: &Path) -> Vec<(PathBuf, Option<SystemTime>, u64)> {
    let mut files = Vec::new();
    for folder in [dir.to_path_buf(), dir.join(crate::THEMES_DIR)] {
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|extension| extension != "toml") {
                continue;
            }
            let metadata = entry.metadata().ok();
            files.push((
                path,
                metadata.as_ref().and_then(|metadata| metadata.modified().ok()),
                metadata.map_or(0, |metadata| metadata.len()),
            ));
        }
    }
    files.sort();
    files
}

fn is_settings_file(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension == "toml") || path.ends_with(crate::THEMES_DIR)
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Instant;

    use assert_fs::TempDir;
    use assert_fs::prelude::*;

    use super::*;

    #[test]
    fn writing_a_settings_file_is_reported() {
        let dir = TempDir::new().unwrap();
        crate::write_defaults(dir.path()).unwrap();
        let (sender, changes) = mpsc::channel();
        let _watcher = SettingsWatcher::start(dir.path(), Duration::from_millis(50), move || {
            let _ = sender.send(Instant::now());
        })
        .unwrap();

        dir.child("notes.txt").write_str("not a setting").unwrap();
        dir.child("config.toml")
            .write_str("[bar]\nvisible_results = 5\n")
            .unwrap();
        assert!(
            changes.recv_timeout(Duration::from_secs(5)).is_ok(),
            "config.toml change reported"
        );

        while changes.try_recv().is_ok() {}
        dir.child("themes/mine.toml")
            .write_str("[shape]\nradius = 20\n")
            .unwrap();
        assert!(
            changes.recv_timeout(Duration::from_secs(5)).is_ok(),
            "theme change reported"
        );
    }

    #[test]
    fn reading_the_settings_is_not_a_change() {
        let dir = TempDir::new().unwrap();
        crate::write_defaults(dir.path()).unwrap();
        let (sender, changes) = mpsc::channel();
        let _watcher = SettingsWatcher::start(dir.path(), Duration::from_millis(50), move || {
            let _ = sender.send(());
        })
        .unwrap();
        for _ in 0..3 {
            let _ = std::fs::read_to_string(dir.path().join("config.toml")).unwrap();
            let _ = crate::load(dir.path(), crate::SystemMode::Dark).unwrap();
        }
        assert!(
            changes.recv_timeout(Duration::from_millis(500)).is_err(),
            "reading is not reported"
        );
    }
}
