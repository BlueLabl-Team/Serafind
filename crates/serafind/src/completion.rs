//! Completion of the folder typed after `in:`: `~/Doc` gives `~/Documents/`.

use std::path::{MAIN_SEPARATOR, Path, PathBuf};

/// Folders completing `typed` (what follows `in:`), each written the way it will be typed back,
/// ending with a separator so that the next completion goes one level deeper. Case is ignored;
/// names with the typed case come first, then alphabetical order. Hidden folders only when the
/// typed name starts with a dot.
#[must_use]
pub fn complete_folder(typed: &str, home: &Path, current: &Path, limit: usize) -> Vec<String> {
    if typed == "~" {
        return vec!["~/".to_owned()];
    }
    let separator = typed.rfind(|c| c == '/' || (cfg!(windows) && c == '\\'));
    let (folder_part, prefix) = match separator {
        Some(at) => (&typed[..=at], &typed[at + 1..]),
        None => ("", typed),
    };
    let folder = if folder_part.is_empty() {
        current.to_path_buf()
    } else {
        resolve(folder_part, home, current)
    };
    let Ok(entries) = std::fs::read_dir(&folder) else {
        return Vec::new();
    };
    let show_hidden = prefix.starts_with('.');
    let lower_prefix = prefix.to_lowercase();
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| {
            name.to_lowercase().starts_with(&lower_prefix) && (show_hidden || !name.starts_with('.'))
        })
        .collect();
    names.sort_by(|a, b| {
        let exact = |name: &String| !name.starts_with(prefix);
        exact(a)
            .cmp(&exact(b))
            .then_with(|| a.to_lowercase().cmp(&b.to_lowercase()))
    });
    names.truncate(limit);
    let separator = if folder_part.contains('\\') { '\\' } else { '/' };
    names
        .into_iter()
        .map(|name| format!("{folder_part}{name}{separator}"))
        .collect()
}

fn resolve(typed: &str, home: &Path, current: &Path) -> PathBuf {
    if let Some(rest) = typed.strip_prefix("~/").or_else(|| typed.strip_prefix("~\\")) {
        home.join(rest)
    } else if Path::new(typed).is_absolute() || typed.starts_with(MAIN_SEPARATOR) {
        PathBuf::from(typed)
    } else {
        current.join(typed)
    }
}

/// The `in:` being typed at the end of `line`, when the caret is at the end: where its value
/// starts (byte offset) and the value typed so far.
#[must_use]
pub fn typed_folder(line: &str, caret: usize) -> Option<(usize, &str)> {
    if caret != line.len() || line.ends_with(char::is_whitespace) {
        return None;
    }
    let start = line.rfind(char::is_whitespace).map_or(0, |at| at + 1);
    let token = &line[start..];
    let value = token.strip_prefix("in:").or_else(|| token.strip_prefix("IN:"))?;
    if value.contains('"') {
        return None;
    }
    Some((start + 3, value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_fs::TempDir;
    use assert_fs::prelude::*;

    fn home() -> TempDir {
        let home = TempDir::new().unwrap();
        for folder in [
            "Documents/Taxes",
            "Downloads",
            "documents-old",
            ".config",
            "Music",
        ] {
            home.child(folder).create_dir_all().unwrap();
        }
        home.child("Docs.txt").touch().unwrap();
        home
    }

    #[test]
    fn completes_from_home() {
        let home = home();
        let found = complete_folder("~/Do", home.path(), home.path(), 10);
        assert_eq!(found, ["~/Documents/", "~/Downloads/", "~/documents-old/"]);
    }

    #[test]
    fn goes_deeper_and_relative() {
        let home = home();
        assert_eq!(
            complete_folder("~/Documents/", home.path(), home.path(), 10),
            ["~/Documents/Taxes/"]
        );
        assert_eq!(complete_folder("mu", home.path(), home.path(), 10), ["Music/"]);
    }

    #[test]
    fn hidden_folders_only_when_asked() {
        let home = home();
        assert!(!complete_folder("~/", home.path(), home.path(), 10).contains(&"~/.config/".to_owned()));
        assert_eq!(
            complete_folder("~/.c", home.path(), home.path(), 10),
            ["~/.config/"]
        );
    }

    #[test]
    fn tilde_alone() {
        let home = home();
        assert_eq!(complete_folder("~", home.path(), home.path(), 10), ["~/"]);
    }

    #[test]
    fn typed_folder_at_the_end_only() {
        assert_eq!(typed_folder("invoice in:~/Doc", 16), Some((11, "~/Doc")));
        assert_eq!(typed_folder("in:", 3), Some((3, "")));
        assert_eq!(typed_folder("invoice in:~/Doc", 5), None);
        assert_eq!(typed_folder("in:~/Doc ", 9), None);
        assert_eq!(typed_folder("invoice", 7), None);
    }
}
