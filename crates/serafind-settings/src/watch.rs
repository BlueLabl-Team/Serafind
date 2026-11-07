//! Says when `config.toml` or a theme changes, so the settings can be read again.

use std::path::Path;
use std::time::Duration;

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
        let mut debouncer = new_debouncer(delay, None, move |result: DebounceEventResult| {
            let Ok(events) = result else {
                return;
            };
            let touches_settings = events
                .iter()
                .flat_map(|event| event.paths.iter())
                .any(|path| is_settings_file(path));
            if touches_settings {
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
            .write_str("[results]\nvisible = 5\n")
            .unwrap();
        assert!(
            changes.recv_timeout(Duration::from_secs(5)).is_ok(),
            "config.toml change reported"
        );

        while changes.try_recv().is_ok() {}
        dir.child("themes/mine.toml")
            .write_str("[font]\nsize = 20\n")
            .unwrap();
        assert!(
            changes.recv_timeout(Duration::from_secs(5)).is_ok(),
            "theme change reported"
        );
    }
}
