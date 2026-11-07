//! A problem in a settings or theme file, with where it is, for the bar to show.

use std::fmt;
use std::path::{Path, PathBuf};

/// A settings or theme file that cannot be used. The previous settings stay in place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsError {
    /// The file.
    pub file: PathBuf,
    /// The line (from 1), when known.
    pub line: Option<usize>,
    /// The column (from 1), when known.
    pub column: Option<usize>,
    /// What is wrong.
    pub message: String,
}

impl SettingsError {
    pub(crate) fn without_place(file: &Path, message: String) -> Self {
        Self {
            file: file.to_path_buf(),
            line: None,
            column: None,
            message,
        }
    }

    pub(crate) fn io(file: &Path, error: &std::io::Error) -> Self {
        Self::without_place(file, format!("cannot read the file: {error}"))
    }

    /// A TOML error, placed with the span the parser gives.
    pub(crate) fn toml(file: &Path, text: &str, error: &toml::de::Error) -> Self {
        let (line, column) = error.span().map_or((None, None), |span| {
            let (line, column) = line_and_column(text, span.start);
            (Some(line), Some(column))
        });
        Self {
            file: file.to_path_buf(),
            line,
            column,
            message: error.message().trim().to_owned(),
        }
    }

    /// A value out of range, placed on the line of its key when it can be found.
    pub(crate) fn value(file: &Path, text: &str, key: &str, problem: &str) -> Self {
        Self {
            file: file.to_path_buf(),
            line: line_of_key(text, key),
            column: None,
            message: format!("`{key}` {problem}"),
        }
    }
}

impl fmt::Display for SettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = self.file.file_name().map_or_else(
            || self.file.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        match (self.line, self.column) {
            (Some(line), Some(column)) => write!(f, "{name}:{line}:{column}: {}", self.message),
            (Some(line), None) => write!(f, "{name}:{line}: {}", self.message),
            _ => write!(f, "{name}: {}", self.message),
        }
    }
}

impl std::error::Error for SettingsError {}

fn line_and_column(text: &str, offset: usize) -> (usize, usize) {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    let before = &text[..offset];
    let line = before.matches('\n').count() + 1;
    let column = before.rsplit('\n').next().map_or(0, |last| last.chars().count()) + 1;
    (line, column)
}

/// The line where `section.key` is set (`[section]` then `key = ...`), if any.
fn line_of_key(text: &str, dotted: &str) -> Option<usize> {
    let (section, key) = dotted.rsplit_once('.').unwrap_or(("", dotted));
    let mut current = String::new();
    for (number, line) in text.lines().enumerate() {
        let line = line.trim();
        if let Some(header) = line.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
            header.trim().clone_into(&mut current);
        } else if current == section && line.split_once('=').is_some_and(|(name, _)| name.trim() == key) {
            return Some(number + 1);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_and_columns_start_at_one() {
        assert_eq!(line_and_column("abc\ndéf", 0), (1, 1));
        assert_eq!(line_and_column("abc\ndéf", 7), (2, 3));
        assert_eq!(line_and_column("abc\ndéf", 6), (2, 2), "inside é: start of é");
    }

    #[test]
    fn keys_are_found_in_their_section() {
        let text = "[window]\nwidth = 10\n[results]\n  visible = 3\nmax = 2\n";
        assert_eq!(line_of_key(text, "window.width"), Some(2));
        assert_eq!(line_of_key(text, "results.visible"), Some(4));
        assert_eq!(line_of_key(text, "window.top"), None);
    }

    #[test]
    fn display() {
        let error = SettingsError {
            file: PathBuf::from("/x/config.toml"),
            line: Some(3),
            column: Some(1),
            message: "unknown field `widht`".to_owned(),
        };
        assert_eq!(error.to_string(), "config.toml:3:1: unknown field `widht`");
    }
}
