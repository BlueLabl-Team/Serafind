//! Serafind's catalog of installed applications.
//!
//! - Linux: the `.desktop` files of the XDG data folders (system, user, Flatpak, Snap), shown on
//!   this desktop, with their name in the desktop's language and their icon from the icon theme.
//! - Windows: the shortcuts of the Start menu and the Microsoft Store apps.
//!
//! The catalog is read again when the application folders change ([`AppsWatcher`]).

pub mod desktop_entry;
pub mod icons;
#[cfg(not(windows))]
pub mod linux;
#[cfg(windows)]
pub mod windows;

use std::path::{Path, PathBuf};
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{DebounceEventResult, Debouncer, RecommendedCache, new_debouncer};

/// An installed application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct App {
    /// Unique: the desktop file id (`org.gnome.Nautilus.desktop`), the Start menu shortcut path,
    /// or the Store app id.
    pub id: String,
    /// The name shown and searched, in the desktop's language.
    pub name: String,
    /// The `.desktop` or `.lnk` file, or `shell:AppsFolder\<id>` for a Store app.
    pub source: PathBuf,
    /// The program started, when it is a file found in `PATH` (not for Store apps).
    pub executable: Option<PathBuf>,
    /// The icon file (PNG or SVG), when found.
    pub icon: Option<PathBuf>,
    /// How to start it.
    pub launch: Launch,
}

/// How an application is started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Launch {
    /// A command line (Linux `Exec`).
    Command {
        /// The program and its arguments.
        arguments: Vec<String>,
        /// It runs in a terminal (`Terminal=true`).
        terminal: bool,
        /// The folder it starts in (`Path=`).
        folder: Option<PathBuf>,
    },
    /// Opened by the system shell: a `.lnk` shortcut or a `shell:AppsFolder` target (Windows).
    Open(PathBuf),
}

/// The applications of this system, sorted by name.
#[must_use]
pub fn discover() -> Vec<App> {
    #[cfg(not(windows))]
    let mut apps = linux::discover(
        &application_folders(),
        &linux::Environment::current(),
        &icons::IconFinder::for_this_desktop(),
    );
    #[cfg(windows)]
    let mut apps = windows::discover(&application_folders());
    apps.sort_by_cached_key(|app| app.name.to_lowercase());
    apps
}

/// The folders whose changes mean the catalog must be read again.
#[must_use]
pub fn application_folders() -> Vec<PathBuf> {
    #[cfg(not(windows))]
    return linux::application_folders();
    #[cfg(windows)]
    return windows::application_folders();
}

/// Finds a program: a path as is when it exists, else in `PATH`.
#[must_use]
pub fn find_program(program: &str) -> Option<PathBuf> {
    let path = Path::new(program);
    if path.components().count() > 1 {
        return path.is_file().then(|| path.to_path_buf());
    }
    let extensions: &[&str] = if cfg!(windows) { &["", ".exe"] } else { &[""] };
    std::env::split_paths(&std::env::var_os("PATH")?).find_map(|folder| {
        extensions
            .iter()
            .map(|extension| folder.join(format!("{program}{extension}")))
            .find(|candidate| candidate.is_file())
    })
}

/// Calls back when an application folder changes (an application is installed or removed).
pub struct AppsWatcher {
    _debouncer: Debouncer<RecommendedWatcher, RecommendedCache>,
}

impl AppsWatcher {
    /// Watches the folders that exist among `folders`.
    ///
    /// # Errors
    /// When the system cannot watch files.
    pub fn start(
        folders: &[PathBuf],
        delay: Duration,
        on_change: impl Fn() + Send + 'static,
    ) -> notify::Result<Self> {
        let mut debouncer = new_debouncer(delay, None, move |result: DebounceEventResult| {
            // Reading the files (to list the apps) is not a change.
            let changed = result.is_ok_and(|events| {
                events
                    .iter()
                    .any(|event| !matches!(event.kind, notify::EventKind::Access(_)))
            });
            if changed {
                on_change();
            }
        })
        .map_err(|error| notify::Error::generic(&error.to_string()))?;
        for folder in folders.iter().filter(|folder| folder.is_dir()) {
            debouncer.watch(folder, RecursiveMode::Recursive)?;
        }
        Ok(Self {
            _debouncer: debouncer,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn programs_are_found_in_path_or_by_path() {
        #[cfg(unix)]
        {
            assert_eq!(find_program("sh").map(|path| path.is_absolute()), Some(true));
            assert_eq!(find_program("/bin/sh"), Some(PathBuf::from("/bin/sh")));
        }
        assert_eq!(find_program("surely-not-an-installed-program"), None);
        assert_eq!(find_program("/no/such/program"), None);
    }
}

#[cfg(test)]
mod real_system {
    /// `cargo test -p serafind-apps -- --ignored --nocapture`: the catalog of this computer.
    #[test]
    #[ignore = "reads the applications of this computer"]
    fn catalog_of_this_computer() {
        let started = std::time::Instant::now();
        let apps = super::discover();
        let elapsed = started.elapsed();
        for app in &apps {
            println!(
                "{:<36} {:<40} icon: {}",
                app.name,
                app.executable
                    .as_ref()
                    .map_or_else(String::new, |path| path.display().to_string()),
                app.icon
                    .as_ref()
                    .map_or_else(|| "-".to_owned(), |path| path.display().to_string())
            );
        }
        let with_icon = apps.iter().filter(|app| app.icon.is_some()).count();
        println!("{} apps, {with_icon} with an icon, in {elapsed:.0?}", apps.len());
    }
}
