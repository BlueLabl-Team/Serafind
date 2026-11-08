//! Finds the file of an icon name (`firefox`, `org.gnome.Nautilus`) in the icon themes, as the
//! freedesktop.org Icon Theme Specification describes: the user's theme, the themes it inherits,
//! then `hicolor`, then `/usr/share/pixmaps`.

use std::path::{Path, PathBuf};

/// The size the icons are wanted at, in pixels; scalable icons are preferred.
const WANTED_SIZE: u32 = 64;

/// Looks icons up in a list of theme folders.
#[derive(Debug, Clone, Default)]
pub struct IconFinder {
    /// Folders to look in, best first: `<base>/<theme>/<subfolder>`.
    folders: Vec<PathBuf>,
    /// Folders of loose icons (`/usr/share/pixmaps`).
    loose: Vec<PathBuf>,
}

impl IconFinder {
    /// Finds nothing.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// For `theme`, looking in the icon base folders `bases` (`~/.local/share/icons`,
    /// `/usr/share/icons`...).
    #[must_use]
    pub fn new(theme: &str, bases: &[PathBuf], loose: Vec<PathBuf>) -> Self {
        let mut themes: Vec<(String, Ini)> = Vec::new();
        let mut pending = vec![theme.to_owned()];
        while let Some(theme) = pending.pop() {
            if themes.iter().any(|(name, _)| *name == theme) || themes.len() > 16 {
                continue;
            }
            // The index of a theme is in one of the base folders; its subfolders may be in all.
            let Some(index) = bases
                .iter()
                .find_map(|base| std::fs::read_to_string(base.join(&theme).join("index.theme")).ok())
            else {
                continue;
            };
            let index = Ini::parse(&index);
            // Inherited themes are tried in the order written: push them reversed.
            let inherited: Vec<String> = index
                .get("Icon Theme", "Inherits")
                .unwrap_or_default()
                .split(',')
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_owned)
                .collect();
            pending.extend(inherited.into_iter().rev());
            themes.push((theme, index));
        }
        if !themes.iter().any(|(name, _)| name == "hicolor")
            && let Some(index) = bases
                .iter()
                .find_map(|base| std::fs::read_to_string(base.join("hicolor/index.theme")).ok())
        {
            themes.push(("hicolor".to_owned(), Ini::parse(&index)));
        }
        let mut folders = Vec::new();
        for (theme, index) in &themes {
            let subfolders = theme_folders(index);
            for base in bases {
                let root = base.join(theme);
                if root.is_dir() {
                    folders.extend(subfolders.iter().map(|folder| root.join(folder)));
                }
            }
        }
        folders.retain(|folder| folder.is_dir());
        Self { folders, loose }
    }

    /// For the user's icon theme, in the usual folders.
    #[must_use]
    pub fn for_this_desktop() -> Self {
        let mut bases = Vec::new();
        if let Some(home) = dirs::home_dir() {
            bases.push(home.join(".icons"));
        }
        if let Some(data_home) = dirs::data_dir() {
            bases.push(data_home.join("icons"));
            bases.push(data_home.join("flatpak/exports/share/icons"));
        }
        let data_dirs = std::env::var("XDG_DATA_DIRS")
            .ok()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "/usr/local/share:/usr/share".to_owned());
        bases.extend(std::env::split_paths(&data_dirs).map(|base| base.join("icons")));
        bases.push(PathBuf::from("/var/lib/flatpak/exports/share/icons"));
        bases.dedup();
        Self::new(&user_theme(), &bases, vec![PathBuf::from("/usr/share/pixmaps")])
    }

    /// The file of an icon: an absolute path as is, else the best size in the themes.
    #[must_use]
    pub fn find(&self, icon: &str) -> Option<PathBuf> {
        let path = Path::new(icon);
        if path.is_absolute() {
            return path.is_file().then(|| path.to_path_buf());
        }
        self.folders
            .iter()
            .chain(&self.loose)
            .flat_map(|folder| ["svg", "png"].map(|extension| folder.join(format!("{icon}.{extension}"))))
            .find(|candidate| candidate.is_file())
    }
}

/// The subfolders of a theme (`index.theme`), application icons first, then scalable, then the
/// sizes nearest to [`WANTED_SIZE`] (larger first).
fn theme_folders(index: &Ini) -> Vec<String> {
    let Some(list) = index.get("Icon Theme", "Directories") else {
        return Vec::new();
    };
    let mut folders: Vec<(bool, bool, u32, String)> = list
        .split(',')
        .map(str::trim)
        .filter(|folder| !folder.is_empty())
        .map(|folder| {
            let number = |key: &str, default: u32| {
                index
                    .get(folder, key)
                    .and_then(|value| value.parse::<u32>().ok())
                    .unwrap_or(default)
            };
            let size = number("Size", 0) * number("Scale", 1);
            let scalable = index.get(folder, "Type") == Some("Scalable");
            let applications =
                index.get(folder, "Context") == Some("Applications") || folder.contains("apps");
            let distance = size.abs_diff(WANTED_SIZE) + if size < WANTED_SIZE { 1000 } else { 0 };
            (!applications, !scalable, distance, folder.to_owned())
        })
        .collect();
    folders.sort();
    folders.into_iter().map(|(_, _, _, folder)| folder).collect()
}

/// An INI-like file read once: `[group]` then `key=value` lines.
#[derive(Debug, Clone, Default)]
struct Ini {
    groups: std::collections::HashMap<String, std::collections::HashMap<String, String>>,
}

impl Ini {
    fn parse(text: &str) -> Self {
        let mut groups: std::collections::HashMap<String, std::collections::HashMap<String, String>> =
            std::collections::HashMap::new();
        let mut group = String::new();
        for line in text.lines() {
            let line = line.trim();
            if let Some(name) = line.strip_prefix('[').and_then(|line| line.strip_suffix(']')) {
                name.clone_into(&mut group);
            } else if let Some((key, value)) = line.split_once('=') {
                groups
                    .entry(group.clone())
                    .or_default()
                    .entry(key.trim().to_owned())
                    .or_insert_with(|| value.trim().to_owned());
            }
        }
        Self { groups }
    }

    fn get(&self, group: &str, key: &str) -> Option<&str> {
        self.groups.get(group)?.get(key).map(String::as_str)
    }
}

/// The icon theme the user chose: GNOME and GTK settings, KDE settings, else `hicolor`.
fn user_theme() -> String {
    if let Ok(output) = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "icon-theme"])
        .output()
        && output.status.success()
    {
        let theme = String::from_utf8_lossy(&output.stdout)
            .trim()
            .trim_matches('\'')
            .to_owned();
        if !theme.is_empty() {
            return theme;
        }
    }
    let config = dirs::config_dir().unwrap_or_default();
    if let Ok(text) = std::fs::read_to_string(config.join("kdeglobals"))
        && let Some(theme) = Ini::parse(&text).get("Icons", "Theme")
    {
        return theme.to_owned();
    }
    for file in ["gtk-4.0/settings.ini", "gtk-3.0/settings.ini"] {
        if let Ok(text) = std::fs::read_to_string(config.join(file))
            && let Some(theme) = Ini::parse(&text).get("Settings", "gtk-icon-theme-name")
        {
            return theme.to_owned();
        }
    }
    "hicolor".to_owned()
}

#[cfg(test)]
mod tests {
    use assert_fs::TempDir;
    use assert_fs::prelude::*;

    use super::*;

    #[test]
    fn finds_the_best_icon_through_inherited_themes() {
        let base = TempDir::new().unwrap();
        base.child("Mine/index.theme")
            .write_str("[Icon Theme]\nInherits=Parent\nDirectories=16x16/apps\n\n[16x16/apps]\nSize=16\nContext=Applications\n")
            .unwrap();
        base.child("Mine/16x16/apps/tiny.png").touch().unwrap();
        base.child("Parent/index.theme")
            .write_str("[Icon Theme]\nDirectories=48x48/apps,scalable/apps\n\n[48x48/apps]\nSize=48\nContext=Applications\n\n[scalable/apps]\nSize=128\nType=Scalable\nContext=Applications\n")
            .unwrap();
        base.child("Parent/48x48/apps/editor.png").touch().unwrap();
        base.child("Parent/scalable/apps/editor.svg").touch().unwrap();
        base.child("hicolor/index.theme")
            .write_str(
                "[Icon Theme]\nDirectories=64x64/apps\n\n[64x64/apps]\nSize=64\nContext=Applications\n",
            )
            .unwrap();
        base.child("hicolor/64x64/apps/game.png").touch().unwrap();
        // A second base folder without index.theme: the subfolders of the index still apply.
        let user = TempDir::new().unwrap();
        user.child("hicolor/64x64/apps/mine.png").touch().unwrap();
        let loose = base.child("pixmaps");
        loose.child("old.png").touch().unwrap();

        let finder = IconFinder::new(
            "Mine",
            &[user.path().to_path_buf(), base.path().to_path_buf()],
            vec![loose.path().to_path_buf()],
        );
        let found = |name: &str| {
            finder.find(name).map(|path| {
                path.strip_prefix(base.path())
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
        };
        assert_eq!(found("tiny").as_deref(), Some("Mine/16x16/apps/tiny.png"));
        assert_eq!(
            found("editor").as_deref(),
            Some("Parent/scalable/apps/editor.svg")
        );
        assert_eq!(found("game").as_deref(), Some("hicolor/64x64/apps/game.png"));
        assert_eq!(found("old").as_deref(), Some("pixmaps/old.png"));
        assert_eq!(found("missing"), None);
        assert_eq!(
            finder.find("mine"),
            Some(user.path().join("hicolor/64x64/apps/mine.png"))
        );
    }

    #[test]
    fn absolute_icon_paths() {
        let base = TempDir::new().unwrap();
        base.child("icon.png").touch().unwrap();
        let path = base.path().join("icon.png");
        assert_eq!(IconFinder::empty().find(&path.to_string_lossy()), Some(path));
        assert_eq!(IconFinder::empty().find("/no/such/icon.png"), None);
    }
}
