//! Applications on Linux: the `.desktop` files of the XDG data folders (system, user, Flatpak,
//! Snap).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::desktop_entry::{DesktopEntry, ExecContext, exec_arguments, locale_keys};
use crate::icons::IconFinder;
use crate::{App, Launch, find_program};

/// The folders holding `.desktop` files, most important first (a user file hides a system file
/// of the same name).
#[must_use]
pub fn application_folders() -> Vec<PathBuf> {
    let mut bases: Vec<PathBuf> = Vec::new();
    if let Some(data_home) = dirs::data_dir() {
        bases.push(data_home);
    }
    let data_dirs = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".to_owned());
    bases.extend(std::env::split_paths(&data_dirs));
    // Flatpak and Snap, in case the session did not add them to XDG_DATA_DIRS.
    if let Some(data_home) = dirs::data_dir() {
        bases.push(data_home.join("flatpak/exports/share"));
    }
    bases.push(PathBuf::from("/var/lib/flatpak/exports/share"));
    let mut folders: Vec<PathBuf> = Vec::new();
    for base in bases {
        let folder = base.join("applications");
        if !folders.contains(&folder) {
            folders.push(folder);
        }
    }
    let snap = PathBuf::from("/var/lib/snapd/desktop/applications");
    if !folders.contains(&snap) {
        folders.push(snap);
    }
    folders
}

/// What decides which applications are shown: the desktop and the language.
#[derive(Debug, Clone, Default)]
pub struct Environment {
    /// `XDG_CURRENT_DESKTOP`, split (`ubuntu:GNOME` gives two names).
    pub desktops: Vec<String>,
    /// Locales for translated names, best first.
    pub locales: Vec<String>,
}

impl Environment {
    /// From the environment variables of this process.
    #[must_use]
    pub fn current() -> Self {
        let desktops = std::env::var("XDG_CURRENT_DESKTOP")
            .unwrap_or_default()
            .split(':')
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect();
        let locale = ["LC_ALL", "LC_MESSAGES", "LANG"]
            .iter()
            .find_map(|name| std::env::var(name).ok().filter(|value| !value.is_empty()))
            .unwrap_or_default();
        Self {
            desktops,
            locales: locale_keys(&locale),
        }
    }
}

/// Every application of `folders`.
#[must_use]
pub fn discover(folders: &[PathBuf], environment: &Environment, icons: &IconFinder) -> Vec<App> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut apps = Vec::new();
    for folder in folders {
        for file in desktop_files(folder) {
            let Some(id) = desktop_id(folder, &file) else {
                continue;
            };
            // The first file of an id wins, even when it hides the application.
            if !seen.insert(id.clone()) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&file) else {
                continue;
            };
            if let Some(app) = read_app(id, &file, &DesktopEntry::parse(&text), environment, icons) {
                apps.push(app);
            }
        }
    }
    apps
}

/// The `.desktop` files of a folder and its subfolders.
fn desktop_files(folder: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut folders = vec![folder.to_path_buf()];
    while let Some(folder) = folders.pop() {
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            // Follows links: Flatpak and Snap export links to their files.
            if path.is_dir() {
                folders.push(path);
            } else if path.extension().is_some_and(|extension| extension == "desktop") {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

/// The desktop file id: the path under the applications folder, `/` replaced by `-`.
fn desktop_id(folder: &Path, file: &Path) -> Option<String> {
    let relative = file.strip_prefix(folder).ok()?;
    Some(relative.to_string_lossy().replace('/', "-"))
}

/// The application of a `.desktop` file, unless it is hidden, for another desktop, or cannot be
/// started.
#[must_use]
pub fn read_app(
    id: String,
    file: &Path,
    entry: &DesktopEntry,
    environment: &Environment,
    icons: &IconFinder,
) -> Option<App> {
    if entry.get("Type") != Some("Application") || entry.flag("NoDisplay") || entry.flag("Hidden") {
        return None;
    }
    let shown_in = entry.list("OnlyShowIn");
    let on_this_desktop = |names: &[&str]| {
        environment
            .desktops
            .iter()
            .any(|desktop| names.iter().any(|name| name.eq_ignore_ascii_case(desktop)))
    };
    if !shown_in.is_empty() && !on_this_desktop(&shown_in) {
        return None;
    }
    if on_this_desktop(&entry.list("NotShowIn")) {
        return None;
    }
    if let Some(try_exec) = entry.get("TryExec")
        && find_program(try_exec).is_none()
    {
        return None;
    }
    let name = entry.localized("Name", &environment.locales)?.to_owned();
    let icon_name = entry.get("Icon");
    let file_text = file.to_string_lossy();
    let arguments = exec_arguments(
        entry.get("Exec")?,
        &ExecContext {
            icon: icon_name,
            name: &name,
            file: &file_text,
        },
    )?;
    let executable = find_program(&arguments[0]);
    Some(App {
        id,
        name,
        source: file.to_path_buf(),
        executable,
        icon: icon_name.and_then(|icon| icons.find(icon)),
        launch: Launch::Command {
            arguments,
            terminal: entry.flag("Terminal"),
            folder: entry
                .get("Path")
                .filter(|path| !path.is_empty())
                .map(PathBuf::from),
        },
    })
}

#[cfg(test)]
mod tests {
    use assert_fs::TempDir;
    use assert_fs::prelude::*;

    use super::*;

    fn environment() -> Environment {
        Environment {
            desktops: vec!["GNOME".to_owned()],
            locales: locale_keys("fr_FR.UTF-8"),
        }
    }

    fn app(text: &str) -> Option<App> {
        let entry = DesktopEntry::parse(text);
        read_app(
            "test.desktop".to_owned(),
            Path::new("/apps/test.desktop"),
            &entry,
            &environment(),
            &IconFinder::empty(),
        )
    }

    #[test]
    fn an_application() {
        let app = app("[Desktop Entry]\nType=Application\nName=Files\nName[fr]=Fichiers\nExec=nautilus --new-window %U\nIcon=org.gnome.Nautilus\n").unwrap();
        assert_eq!(app.name, "Fichiers");
        assert_eq!(
            app.launch,
            Launch::Command {
                arguments: vec!["nautilus".to_owned(), "--new-window".to_owned()],
                terminal: false,
                folder: None,
            }
        );
    }

    #[rstest::rstest]
    #[case("Type=Link\nName=A\nExec=a\n")]
    #[case("Type=Application\nName=A\nExec=a\nNoDisplay=true\n")]
    #[case("Type=Application\nName=A\nExec=a\nHidden=true\n")]
    #[case("Type=Application\nName=A\nExec=a\nOnlyShowIn=KDE;\n")]
    #[case("Type=Application\nName=A\nExec=a\nNotShowIn=GNOME;\n")]
    #[case("Type=Application\nName=A\nExec=a\nTryExec=surely-not-an-installed-program\n")]
    #[case("Type=Application\nName=A\n")]
    #[case("Type=Application\nExec=a\n")]
    fn hidden_or_unusable(#[case] body: &str) {
        assert_eq!(app(&format!("[Desktop Entry]\n{body}")), None);
    }

    #[test]
    fn shown_on_this_desktop() {
        assert!(app("[Desktop Entry]\nType=Application\nName=A\nExec=a\nOnlyShowIn=XFCE;GNOME;\n").is_some());
    }

    #[test]
    fn terminal_applications_and_folders() {
        let app = app("[Desktop Entry]\nType=Application\nName=Top\nExec=htop\nTerminal=true\nPath=/tmp\n")
            .unwrap();
        assert_eq!(
            app.launch,
            Launch::Command {
                arguments: vec!["htop".to_owned()],
                terminal: true,
                folder: Some(PathBuf::from("/tmp")),
            }
        );
    }

    #[test]
    fn user_files_hide_system_files_and_ids_use_subfolders() {
        let user = TempDir::new().unwrap();
        let system = TempDir::new().unwrap();
        user.child("editor.desktop")
            .write_str("[Desktop Entry]\nType=Application\nName=My Editor\nExec=my-editor\n")
            .unwrap();
        system
            .child("editor.desktop")
            .write_str("[Desktop Entry]\nType=Application\nName=Editor\nExec=editor\n")
            .unwrap();
        system
            .child("kde/game.desktop")
            .write_str("[Desktop Entry]\nType=Application\nName=Game\nExec=game\n")
            .unwrap();
        system
            .child("gone.desktop")
            .write_str("[Desktop Entry]\nType=Application\nName=Gone\nExec=gone\n")
            .unwrap();
        user.child("gone.desktop")
            .write_str("[Desktop Entry]\nType=Application\nName=Gone\nExec=gone\nHidden=true\n")
            .unwrap();
        let folders = [user.path().to_path_buf(), system.path().to_path_buf()];
        let apps = discover(&folders, &environment(), &IconFinder::empty());
        let mut found: Vec<(&str, &str)> = apps
            .iter()
            .map(|app| (app.id.as_str(), app.name.as_str()))
            .collect();
        found.sort_unstable();
        assert_eq!(
            found,
            [("editor.desktop", "My Editor"), ("kde-game.desktop", "Game")]
        );
    }
}
