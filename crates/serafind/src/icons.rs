//! Icons of the results: one family per kind of entry, drawn with the Phosphor icon font.

use serafind_core::EntryKind;
use serafind_settings::{Color, Theme};

/// The kinds of entries that get their own icon and color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    /// A folder.
    Folder,
    /// Text and office documents.
    Document,
    /// PDF files.
    Pdf,
    /// Pictures.
    Image,
    /// Music and sounds.
    Audio,
    /// Videos.
    Video,
    /// Archives.
    Archive,
    /// Source code.
    Code,
    /// An application.
    App,
    /// Anything else.
    Other,
}

/// Phosphor code points (regular weight), only those used.
mod glyph {
    pub const APP_WINDOW: &str = "\u{E5DA}";
    pub const FILE: &str = "\u{E230}";
    pub const FILE_AUDIO: &str = "\u{EA20}";
    pub const FILE_CODE: &str = "\u{E914}";
    pub const FILE_IMAGE: &str = "\u{EA24}";
    pub const FILE_PDF: &str = "\u{E702}";
    pub const FILE_TEXT: &str = "\u{E23A}";
    pub const FILE_VIDEO: &str = "\u{EA22}";
    pub const FILE_ZIP: &str = "\u{E958}";
    pub const FOLDER: &str = "\u{E24A}";
    pub const MAGNIFYING_GLASS: &str = "\u{E30C}";
    pub const WARNING_CIRCLE: &str = "\u{E4E2}";
    pub const CARET_DOWN: &str = "\u{E136}";
}

/// The magnifying glass of the input.
pub const SEARCH: &str = glyph::MAGNIFYING_GLASS;
/// The warning sign of settings errors.
pub const WARNING: &str = glyph::WARNING_CIRCLE;
/// The arrow of the mode selector.
pub const CARET_DOWN: &str = glyph::CARET_DOWN;

impl Family {
    /// The family of an entry, from its kind and the extension of its name.
    #[must_use]
    pub fn of(kind: EntryKind, name: &str) -> Self {
        match kind {
            EntryKind::App => return Self::App,
            EntryKind::Folder => return Self::Folder,
            EntryKind::File => {}
        }
        let extension = name
            .rsplit_once('.')
            .filter(|(stem, _)| !stem.is_empty())
            .map(|(_, extension)| extension.to_ascii_lowercase());
        match extension.as_deref() {
            Some("pdf") => Self::Pdf,
            Some(
                "txt" | "md" | "odt" | "ods" | "odp" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx"
                | "rtf" | "csv" | "epub" | "tex",
            ) => Self::Document,
            Some(
                "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "tif" | "tiff" | "heic" | "avif"
                | "ico" | "raw" | "cr2" | "nef" | "psd" | "xcf",
            ) => Self::Image,
            Some("mp3" | "flac" | "ogg" | "opus" | "wav" | "m4a" | "aac" | "wma" | "aiff") => Self::Audio,
            Some("mp4" | "mkv" | "webm" | "avi" | "mov" | "wmv" | "m4v" | "flv") => Self::Video,
            Some(
                "zip" | "tar" | "gz" | "tgz" | "xz" | "bz2" | "zst" | "7z" | "rar" | "iso" | "deb" | "rpm",
            ) => Self::Archive,
            Some(
                "rs" | "py" | "js" | "ts" | "tsx" | "jsx" | "c" | "h" | "cpp" | "hpp" | "cs" | "java" | "kt"
                | "go" | "rb" | "php" | "sh" | "zsh" | "ps1" | "html" | "css" | "scss" | "json" | "toml"
                | "yaml" | "yml" | "xml" | "sql" | "lua" | "swift" | "dart" | "vue" | "axaml" | "xaml",
            ) => Self::Code,
            _ => Self::Other,
        }
    }

    /// The Phosphor glyph.
    #[must_use]
    pub fn glyph(self) -> &'static str {
        match self {
            Self::Folder => glyph::FOLDER,
            Self::Document => glyph::FILE_TEXT,
            Self::Pdf => glyph::FILE_PDF,
            Self::Image => glyph::FILE_IMAGE,
            Self::Audio => glyph::FILE_AUDIO,
            Self::Video => glyph::FILE_VIDEO,
            Self::Archive => glyph::FILE_ZIP,
            Self::Code => glyph::FILE_CODE,
            Self::App => glyph::APP_WINDOW,
            Self::Other => glyph::FILE,
        }
    }
}

/// The color of a family's icon: the theme's own color for it when set, otherwise folders in
/// `accent` and everything else in `text_secondary`.
#[must_use]
pub fn color_of(family: Family, theme: &Theme) -> Color {
    let colors = &theme.icons.colors;
    let own = match family {
        Family::Folder => colors.folder,
        Family::Document => colors.document,
        Family::Pdf => colors.pdf,
        Family::Image => colors.image,
        Family::Audio => colors.audio,
        Family::Video => colors.video,
        Family::Archive => colors.archive,
        Family::Code => colors.code,
        Family::App => colors.app,
        Family::Other => colors.other,
    };
    own.unwrap_or(if family == Family::Folder {
        theme.colors.accent
    } else {
        theme.colors.text_secondary
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case(EntryKind::Folder, "Documents", Family::Folder)]
    #[case(EntryKind::Folder, "photos.jpg", Family::Folder)]
    #[case(EntryKind::App, "Firefox", Family::App)]
    #[case(EntryKind::File, "Invoice.PDF", Family::Pdf)]
    #[case(EntryKind::File, "notes.md", Family::Document)]
    #[case(EntryKind::File, "holiday.jpeg", Family::Image)]
    #[case(EntryKind::File, "song.flac", Family::Audio)]
    #[case(EntryKind::File, "film.mkv", Family::Video)]
    #[case(EntryKind::File, "backup.tar.gz", Family::Archive)]
    #[case(EntryKind::File, "main.rs", Family::Code)]
    #[case(EntryKind::File, ".bashrc", Family::Other)]
    #[case(EntryKind::File, "README", Family::Other)]
    fn families(#[case] kind: EntryKind, #[case] name: &str, #[case] family: Family) {
        assert_eq!(Family::of(kind, name), family);
    }

    #[test]
    fn default_colors_unless_set() {
        let mut theme = Theme::built_in("dark").unwrap();
        assert_eq!(color_of(Family::Folder, &theme), theme.colors.accent);
        assert_eq!(color_of(Family::Pdf, &theme), theme.colors.text_secondary);
        let red = Color::parse("#ff0000").unwrap();
        theme.icons.colors.pdf = Some(red);
        assert_eq!(color_of(Family::Pdf, &theme), red);
    }
}
