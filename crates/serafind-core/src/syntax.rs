//! The search line: `invoice ext:pdf in:~/Documents -draft c:"TODO fix" f:`.
//!
//! The line is split into parts (words, `"quoted phrases"` and `key:value` filters, any of them
//! excluded with a leading `-`). Each part is reported, with the reason when it cannot be used, so
//! the bar can show it as a chip. A part that cannot be used is ignored: the rest still searches.
//!
//! | Filter | Meaning |
//! |---|---|
//! | `ext:pdf,docx` / `-ext:` | Extensions (without the dot) / extensions left out |
//! | `in:~/Documents` | Starting folder (`~`, absolute, or relative to the current folder); the last one wins |
//! | `app:` `a:` | Apps mode; a value is a name word (`a:fire`) |
//! | `d:` / `f:` | Folders / Files mode; a value is a name word |
//! | `type:folder` / `type:file` | Same as `d:` / `f:`; `-type:file` means folders and the reverse |
//! | `content:` `c:` / `-c:` | Content mode: text the file must contain / must not contain |
//!
//! Several mode filters: the last one wins, the earlier ones are reported as replaced (two content
//! filters are the same mode and both apply). A mode filter wins over the tab chosen in the bar.

use std::fmt;
use std::ops::Range;
use std::path::{Component, Path, PathBuf};

use crate::SearchMode;

/// What the parser needs from the outside world. Kept as plain values and a callback so that
/// parsing stays pure and testable.
pub struct ParseContext<'a> {
    /// The user's home directory, for `~`.
    pub home: &'a Path,
    /// Base of relative `in:` folders.
    pub current_folder: &'a Path,
    /// Whether a folder exists (the parser does not touch the disk itself).
    pub folder_exists: &'a dyn Fn(&Path) -> bool,
}

/// A search line as understood.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    /// Every part of the line, in order, as typed.
    pub parts: Vec<Part>,
    /// Words (or phrases) the name must contain; `*` and `?` make a pattern.
    pub words: Vec<String>,
    /// Words the name must not contain.
    pub excluded_words: Vec<String>,
    /// Extensions the name must have (one of them), lower case, without the dot.
    pub extensions: Vec<String>,
    /// Extensions the name must not have.
    pub excluded_extensions: Vec<String>,
    /// Texts the file must contain (Content mode).
    pub content: Vec<String>,
    /// Texts the file must not contain (Content mode).
    pub excluded_content: Vec<String>,
    /// The mode asked by a filter; `None` keeps the tab chosen in the bar.
    pub mode: Option<SearchMode>,
    /// The `in:` folder, as a full path; `None` keeps the default roots.
    pub start_folder: Option<PathBuf>,
}

impl Query {
    /// The mode the search runs in: the one asked by a filter, else the tab chosen in the bar.
    #[must_use]
    pub fn effective_mode(&self, tab: SearchMode) -> SearchMode {
        self.mode.unwrap_or(tab)
    }

    /// True when some part of the line is reported as a problem.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.parts.iter().any(|part| part.error.is_some())
    }

    /// True when the line asks for nothing (empty, only mode filters, or only ignored parts).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
            && self.excluded_words.is_empty()
            && self.extensions.is_empty()
            && self.excluded_extensions.is_empty()
            && self.content.is_empty()
            && self.excluded_content.is_empty()
            && self.start_folder.is_none()
    }
}

/// One part of the line, for a chip: what it was understood as, and where it is in the line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    /// What the part means.
    pub kind: PartKind,
    /// The part as typed, quotes and `-` included.
    pub text: String,
    /// Where the part is in the line (byte offsets), for the caret and completion.
    pub span: Range<usize>,
    /// What it means, for display: `pdf, docx`, `~/Documents`, `TODO fix`, a name word...
    pub value: String,
    /// Typed with a leading `-`.
    pub excluded: bool,
    /// The problem with this part; `None` when it is understood.
    pub error: Option<PartError>,
}

impl Part {
    /// True when the part is taken into account by the search.
    #[must_use]
    pub fn is_used(&self) -> bool {
        self.error.as_ref().is_none_or(|error| !error.ignores_part())
    }
}

/// What a part of the line means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartKind {
    /// A word or phrase of the name.
    Name,
    /// `ext:`.
    Extension,
    /// `in:`.
    Folder,
    /// A mode filter (`a:`, `d:`, `f:`, `type:`, `c:`).
    Mode(SearchMode),
}

/// Why a part of the line is reported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PartError {
    /// `ext:` with nothing after it.
    MissingExtension,
    /// `in:` with nothing after it.
    MissingFolder,
    /// `in:` names a folder that does not exist.
    FolderNotFound,
    /// `-in:`.
    ExcludedFolder,
    /// A later `in:` replaced this one.
    ReplacedFolder,
    /// `type:` with a value other than file or folder.
    UnknownType,
    /// `-a:`, `-d:` or `-f:`: a mode cannot be left out.
    ExcludedMode,
    /// A later mode filter replaced this one.
    ReplacedMode(SearchMode),
    /// `c:` with nothing after it.
    MissingContent,
    /// `key:` that is not a filter: the part is searched as a name word.
    UnknownFilter(String),
}

impl PartError {
    /// True when the part is ignored; false when it is still used another way.
    #[must_use]
    pub fn ignores_part(&self) -> bool {
        !matches!(self, Self::UnknownFilter(_))
    }
}

impl fmt::Display for PartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingExtension => f.write_str("An extension is expected after ext:, e.g. ext:pdf."),
            Self::MissingFolder => f.write_str("A folder is expected after in:, e.g. in:~/Documents."),
            Self::FolderNotFound => f.write_str("Folder not found."),
            Self::ExcludedFolder => f.write_str("A folder cannot be excluded with in:."),
            Self::ReplacedFolder => f.write_str("Replaced by the next in: filter."),
            Self::UnknownType => f.write_str("type: expects file or folder."),
            Self::ExcludedMode => f.write_str("A mode cannot be excluded."),
            Self::ReplacedMode(mode) => write!(f, "Replaced by the next mode filter ({mode:?})."),
            Self::MissingContent => f.write_str("A text is expected after c:, e.g. c:\"TODO fix\"."),
            Self::UnknownFilter(key) => write!(
                f,
                "Unknown filter '{key}:' (known: {}): searched as a name.",
                FILTER_KEYS.join(", ")
            ),
        }
    }
}

/// The filters, as typed before the colon.
pub const FILTER_KEYS: [&str; 9] = ["ext", "in", "app", "a", "d", "f", "type", "content", "c"];

/// Reads a search line.
#[must_use]
pub fn parse(line: &str, context: &ParseContext<'_>) -> Query {
    let mut parts: Vec<Part> = tokenize(line).map(|token| understand(token, context)).collect();
    resolve_last_wins(&mut parts);

    let mut query = Query::default();
    for part in parts.iter().filter(|part| part.is_used()) {
        let value = part.value.clone();
        match part.kind {
            PartKind::Name => push(&mut query.words, &mut query.excluded_words, part.excluded, value),
            PartKind::Extension => {
                let list = value.split(", ").map(str::to_owned);
                if part.excluded {
                    query.excluded_extensions.extend(list);
                } else {
                    query.extensions.extend(list);
                }
            }
            PartKind::Folder => query.start_folder = Some(resolve_folder(&raw_value(part), context)),
            PartKind::Mode(mode) => {
                query.mode = Some(mode);
                if mode == SearchMode::Content {
                    push(
                        &mut query.content,
                        &mut query.excluded_content,
                        part.excluded,
                        value,
                    );
                } else if !value.is_empty() && !is_type_filter(part) {
                    query.words.push(value);
                }
            }
        }
    }
    query.parts = parts;
    query
}

fn push(included: &mut Vec<String>, excluded: &mut Vec<String>, is_excluded: bool, value: String) {
    if is_excluded {
        excluded.push(value);
    } else {
        included.push(value);
    }
}

/// A word or filter of the line, before it is understood.
struct Token<'a> {
    text: &'a str,
    span: Range<usize>,
    key: Option<String>,
    value: String,
    excluded: bool,
}

/// Splits the line on white space outside quotes. Quotes are removed from values.
fn tokenize(line: &str) -> impl Iterator<Item = Token<'_>> {
    let mut chars = line.char_indices().peekable();
    std::iter::from_fn(move || {
        while chars.next_if(|(_, c)| c.is_whitespace()).is_some() {}
        let (start, _) = *chars.peek()?;
        let mut end = start;
        let mut raw = String::new();
        let mut quoted = false;
        while let Some((index, c)) = chars.next_if(|&(_, c)| quoted || !c.is_whitespace()) {
            end = index + c.len_utf8();
            if c == '"' {
                quoted = !quoted;
            } else {
                raw.push(c);
            }
        }
        let text = &line[start..end];
        Some(make_token(text, start..end, raw))
    })
}

fn make_token(text: &str, span: Range<usize>, raw: String) -> Token<'_> {
    // A typed "-" excludes; a quoted one ("-x") is part of the word.
    let excluded = text.len() > 1 && text.starts_with('-');
    let value = if excluded { raw[1..].to_owned() } else { raw };

    // "key:value" with a key of letters only, so "10:30" stays a word. A one-letter key followed by
    // a separator is a Windows drive ("C:\Users", "d:/photos"), not a filter.
    let key_end = value.find(':').filter(|&colon| {
        colon > 0
            && value[..colon].chars().all(|c| c.is_ascii_alphabetic())
            && !(colon == 1 && value[colon + 1..].starts_with(['/', '\\']))
    });
    let (key, value) = match key_end {
        Some(colon) => (
            Some(value[..colon].to_ascii_lowercase()),
            value[colon + 1..].to_owned(),
        ),
        None => (None, value),
    };
    Token {
        text,
        span,
        key,
        value,
        excluded,
    }
}

fn understand(token: Token<'_>, context: &ParseContext<'_>) -> Part {
    let mut part = Part {
        kind: PartKind::Name,
        text: token.text.to_owned(),
        span: token.span,
        value: token.value,
        excluded: token.excluded,
        error: None,
    };
    let Some(key) = token.key else {
        return part;
    };

    let mode = |mode| PartKind::Mode(mode);
    match key.as_str() {
        "ext" => {
            part.kind = PartKind::Extension;
            let list: Vec<String> = part
                .value
                .split(',')
                .map(|extension| extension.trim().trim_start_matches('.').to_lowercase())
                .filter(|extension| !extension.is_empty())
                .collect();
            if list.is_empty() {
                part.error = Some(PartError::MissingExtension);
            }
            part.value = list.join(", ");
        }
        "in" => {
            part.kind = PartKind::Folder;
            part.error = if part.excluded {
                Some(PartError::ExcludedFolder)
            } else if part.value.is_empty() {
                Some(PartError::MissingFolder)
            } else if !(context.folder_exists)(&resolve_folder(&part.value, context)) {
                Some(PartError::FolderNotFound)
            } else {
                None
            };
            if part.error.is_none() {
                part.value = shorten_home(&resolve_folder(&part.value, context), context.home);
            }
        }
        "app" | "a" => part.kind = mode(SearchMode::Apps),
        "d" => part.kind = mode(SearchMode::Folders),
        "f" => part.kind = mode(SearchMode::Files),
        "type" => {
            // "-type:file" means folders, "-type:folder" files.
            let files = match part.value.to_lowercase().as_str() {
                "file" | "files" | "f" => Some(!part.excluded),
                "folder" | "folders" | "dir" | "directory" | "d" => Some(part.excluded),
                _ => None,
            };
            if let Some(files) = files {
                part.kind = mode(if files {
                    SearchMode::Files
                } else {
                    SearchMode::Folders
                });
                part.excluded = false;
            } else {
                part.kind = mode(SearchMode::Files);
                part.error = Some(PartError::UnknownType);
            }
            return part;
        }
        "content" | "c" => {
            part.kind = mode(SearchMode::Content);
            if part.value.is_empty() {
                part.error = Some(PartError::MissingContent);
            }
            return part;
        }
        _ => {
            // Not a filter: searched as an ordinary word, and pointed out.
            part.value = if part.excluded {
                part.text[1..].replace('"', "")
            } else {
                part.text.replace('"', "")
            };
            part.error = Some(PartError::UnknownFilter(key));
            return part;
        }
    }

    if matches!(part.kind, PartKind::Mode(_)) && part.excluded {
        part.error = Some(PartError::ExcludedMode);
    }
    part
}

/// Several `in:`: the last one wins. Several modes: the last one wins (same mode parts all stay).
fn resolve_last_wins(parts: &mut [Part]) {
    let used = |part: &Part| part.is_used();
    let last_folder = parts
        .iter()
        .rposition(|part| part.kind == PartKind::Folder && used(part));
    let last_mode = parts
        .iter()
        .rev()
        .filter(|part| used(part))
        .find_map(|part| match part.kind {
            PartKind::Mode(mode) => Some(mode),
            _ => None,
        });

    for (index, part) in parts.iter_mut().enumerate() {
        if !part.is_used() {
            continue;
        }
        match part.kind {
            PartKind::Folder if Some(index) != last_folder => part.error = Some(PartError::ReplacedFolder),
            PartKind::Mode(mode) => {
                if let Some(last) = last_mode.filter(|&last| last != mode) {
                    part.error = Some(PartError::ReplacedMode(last));
                }
            }
            _ => {}
        }
    }
}

fn is_type_filter(part: &Part) -> bool {
    part.text
        .trim_start_matches('-')
        .to_ascii_lowercase()
        .starts_with("type:")
}

/// The value typed after the colon of a filter, quotes removed.
fn raw_value(part: &Part) -> String {
    let text = part.text.trim_start_matches('-').replace('"', "");
    text.split_once(':')
        .map_or(text.clone(), |(_, value)| value.to_owned())
}

/// `~`, `~/x`, an absolute path, or a path relative to the current folder; `.` and `..` resolved.
fn resolve_folder(value: &str, context: &ParseContext<'_>) -> PathBuf {
    let path = if value == "~" {
        context.home.to_path_buf()
    } else if let Some(rest) = value.strip_prefix("~/").or_else(|| value.strip_prefix("~\\")) {
        context.home.join(rest)
    } else if Path::new(value).is_absolute() {
        PathBuf::from(value)
    } else {
        context.current_folder.join(value)
    };
    normalize_lexically(&path)
}

fn normalize_lexically(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    normalized.push(component);
                }
            }
            other => normalized.push(other),
        }
    }
    normalized
}

/// `/home/me/Documents` → `~/Documents`.
fn shorten_home(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

#[cfg(test)]
mod tests;
