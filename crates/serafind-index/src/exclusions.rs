//! What the index leaves out: folders by name anywhere, folders by full path, and hidden folders.
//!
//! Hidden files are indexed (`.bashrc`, `.env`); hidden folders are not entered (`.config`,
//! `.local`). A folder is hidden when its name starts with a dot, or on Windows when it has the
//! hidden attribute.

use std::path::{Path, PathBuf};

/// Folders the index does not enter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exclusions {
    /// Folder names left out wherever they are (`node_modules`, `.git`...).
    pub names: Vec<String>,
    /// Folders left out by full path (the trash, `AppData`...).
    pub paths: Vec<PathBuf>,
    /// Whether hidden folders are left out.
    pub hidden_folders: bool,
}

/// Folder names left out by default, wherever they are.
pub const DEFAULT_NAMES: [&str; 13] = [
    ".git",
    ".hg",
    ".svn",
    "node_modules",
    "target",
    "__pycache__",
    ".venv",
    "venv",
    ".tox",
    ".gradle",
    ".idea",
    ".cache",
    "$Recycle.Bin",
];

impl Exclusions {
    /// The default exclusions for a home directory.
    #[must_use]
    pub fn defaults(home: &Path) -> Self {
        let paths = if cfg!(windows) {
            vec![home.join("AppData")]
        } else {
            vec![home.join(".local/share/Trash")]
        };
        Self {
            names: DEFAULT_NAMES.iter().map(|&name| name.to_owned()).collect(),
            paths,
            hidden_folders: true,
        }
    }

    /// Nothing left out.
    #[must_use]
    pub fn none() -> Self {
        Self {
            names: Vec::new(),
            paths: Vec::new(),
            hidden_folders: false,
        }
    }

    /// True when the folder at `path` is not entered. `hidden` says whether the system marks it
    /// hidden (Windows attribute); a leading dot always counts as hidden.
    #[must_use]
    pub fn excludes_folder(&self, path: &Path, hidden: bool) -> bool {
        let name = path.file_name().map(|name| name.to_string_lossy());
        let Some(name) = name else {
            return false;
        };
        (self.hidden_folders && (hidden || name.starts_with('.')))
            || self.names.iter().any(|excluded| *excluded == name)
            || self.paths.iter().any(|excluded| excluded == path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case("/home/me/Projects/app/node_modules", true)]
    #[case("/home/me/Projects/app/target", true)]
    #[case("/home/me/.config", true)]
    #[case("/home/me/.local/share/Trash", true)]
    #[case("/home/me/Documents", false)]
    #[case("/home/me/build", false)]
    #[case("/home/me/my.target", false)]
    fn default_exclusions(#[case] path: &str, #[case] excluded: bool) {
        let exclusions = Exclusions::defaults(Path::new("/home/me"));
        assert_eq!(exclusions.excludes_folder(Path::new(path), false), excluded);
    }

    #[test]
    fn system_hidden_folders_are_left_out() {
        let exclusions = Exclusions::defaults(Path::new("/home/me"));
        assert!(exclusions.excludes_folder(Path::new("/home/me/Secret"), true));
        assert!(!Exclusions::none().excludes_folder(Path::new("/home/me/Secret"), true));
    }
}
