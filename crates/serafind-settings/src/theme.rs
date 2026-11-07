//! Themes: colors, font, shapes and icons of the bar.
//!
//! The built-in `dark` and `light` themes are the files written on first launch
//! (`defaults/themes/*.toml`). A theme file may give only some values: the others come from the
//! theme named by `extends` (the built-in `dark` one when it has none). Unknown keys are errors.

use std::fmt;
use std::path::Path;

use serde::{Deserialize, Deserializer};

use crate::SettingsError;
use crate::config::IconSet;

/// The built-in themes: name and file.
pub const BUILT_IN: [(&str, &str); 2] = [
    ("dark", include_str!("../defaults/themes/dark.toml")),
    ("light", include_str!("../defaults/themes/light.toml")),
];

/// A color, `#rrggbb` or `#rrggbbaa` in files.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Color {
    /// Red, green, blue and alpha (not premultiplied).
    pub rgba: [u8; 4],
}

impl Color {
    /// Reads `#rgb`, `#rrggbb` or `#rrggbbaa`.
    ///
    /// # Errors
    /// When the text is not such a color.
    pub fn parse(text: &str) -> Result<Self, String> {
        let invalid = || format!("invalid color {text:?}: expected \"#rrggbb\" or \"#rrggbbaa\"");
        let hex = text.strip_prefix('#').ok_or_else(invalid)?;
        if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(invalid());
        }
        let digit = |index: usize| u8::from_str_radix(&hex[index..=index], 16).map_err(|_| invalid());
        let byte = |index: usize| u8::from_str_radix(&hex[index..index + 2], 16).map_err(|_| invalid());
        let rgba = match hex.len() {
            3 => [digit(0)? * 17, digit(1)? * 17, digit(2)? * 17, 255],
            6 => [byte(0)?, byte(2)?, byte(4)?, 255],
            8 => [byte(0)?, byte(2)?, byte(4)?, byte(6)?],
            _ => return Err(invalid()),
        };
        Ok(Self { rgba })
    }
}

impl fmt::Debug for Color {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [red, green, blue, alpha] = self.rgba;
        write!(formatter, "#{red:02x}{green:02x}{blue:02x}")?;
        if alpha != 255 {
            write!(formatter, "{alpha:02x}")?;
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).map_err(serde::de::Error::custom)
    }
}

/// Declares a theme section twice: complete (every key required, for the built-in themes and the
/// result) and partial (every key optional, for user files), with a way to apply the second on
/// the first.
macro_rules! section {
    ($(#[$meta:meta])* $name:ident / $partial:ident { $($(#[$field_meta:meta])* $field:ident : $type:ty),* $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Deserialize)]
        #[serde(deny_unknown_fields)]
        pub struct $name {
            $($(#[$field_meta])* pub $field: $type,)*
        }

        #[derive(Debug, Clone, Default, Deserialize)]
        #[serde(default, deny_unknown_fields)]
        struct $partial {
            $($field: Option<$type>,)*
        }

        impl $partial {
            fn apply_to(self, target: &mut $name) {
                $(if let Some(value) = self.$field {
                    target.$field = value;
                })*
            }
        }
    };
}

section! {
    /// `[colors]`.
    Colors / PartialColors {
        /// The bar.
        background: Color,
        /// The input, a hovered row.
        surface: Color,
        /// Borders.
        border: Color,
        /// Text.
        text: Color,
        /// Folders, hints, counters.
        text_muted: Color,
        /// Caret, active mode, focus.
        accent: Color,
        /// The selected row.
        selection: Color,
        /// Matched letters.
        highlight: Color,
        /// Chip background.
        chip: Color,
        /// Chip text.
        chip_text: Color,
        /// Chips that cannot be used, settings errors.
        error: Color,
        /// Shadow of the bar.
        shadow: Color,
    }
}

section! {
    /// `[font]`.
    Font / PartialFont {
        /// `Inter` (built in), an installed font name, or a `.ttf` / `.otf` path.
        family: String,
        /// Size in points.
        size: u16,
    }
}

section! {
    /// `[shape]`.
    Shape / PartialShape {
        /// Corner radius of the bar, chips and rows.
        radius: u16,
        /// Inner spacing.
        padding: u16,
        /// Height of a result row.
        row_height: u16,
        /// Whether the bar casts a shadow.
        shadow: bool,
    }
}

section! {
    /// `[icons.colors]`: one color per kind of entry.
    IconColors / PartialIconColors {
        /// Folders.
        folder: Color,
        /// Text and office documents.
        document: Color,
        /// PDF files.
        pdf: Color,
        /// Pictures.
        image: Color,
        /// Music and sounds.
        audio: Color,
        /// Videos.
        video: Color,
        /// Archives.
        archive: Color,
        /// Source code.
        code: Color,
        /// Applications.
        app: Color,
        /// Anything else.
        other: Color,
    }
}

/// `[icons]`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Icons {
    /// The icon set.
    pub set: IconSet,
    /// One color per kind of entry.
    pub colors: IconColors,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct PartialIcons {
    set: Option<IconSet>,
    colors: Option<PartialIconColors>,
}

/// A complete theme.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Theme {
    /// Name shown to the user.
    pub name: String,
    /// Colors.
    pub colors: Colors,
    /// Font.
    pub font: Font,
    /// Shapes.
    pub shape: Shape,
    /// Icons.
    pub icons: Icons,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct PartialTheme {
    extends: Option<String>,
    name: Option<String>,
    colors: Option<PartialColors>,
    font: Option<PartialFont>,
    shape: Option<PartialShape>,
    icons: Option<PartialIcons>,
}

impl PartialTheme {
    fn apply_to(self, theme: &mut Theme) {
        if let Some(name) = self.name {
            theme.name = name;
        }
        if let Some(colors) = self.colors {
            colors.apply_to(&mut theme.colors);
        }
        if let Some(font) = self.font {
            font.apply_to(&mut theme.font);
        }
        if let Some(shape) = self.shape {
            shape.apply_to(&mut theme.shape);
        }
        if let Some(icons) = self.icons {
            if let Some(set) = icons.set {
                theme.icons.set = set;
            }
            if let Some(colors) = icons.colors {
                colors.apply_to(&mut theme.icons.colors);
            }
        }
    }
}

impl Theme {
    /// A built-in theme (`dark` or `light`).
    ///
    /// # Panics
    /// When a built-in theme file is incomplete (checked by tests).
    #[must_use]
    pub fn built_in(name: &str) -> Option<Self> {
        BUILT_IN
            .iter()
            .find(|(built_in, _)| *built_in == name)
            .map(|(_, text)| toml::from_str(text).expect("built-in themes are complete"))
    }

    /// Loads the theme `name`: the file `themes_dir/<name>.toml` when it exists, else the built-in
    /// theme of that name, following `extends`.
    ///
    /// # Errors
    /// When a file cannot be read or understood, a theme does not exist, or `extends` loops.
    pub fn load(name: &str, themes_dir: &Path) -> Result<Self, SettingsError> {
        Self::load_chain(name, themes_dir, &mut Vec::new())
    }

    fn load_chain(name: &str, themes_dir: &Path, chain: &mut Vec<String>) -> Result<Self, SettingsError> {
        let path = themes_dir.join(format!("{name}.toml"));
        // A user file extending the built-in theme of the same name (dark.toml: extends = "dark").
        let in_chain = chain.iter().any(|seen| seen == name);
        if in_chain || !path.is_file() {
            if let Some(theme) = Self::built_in(name) {
                return Ok(theme);
            }
            return Err(if in_chain {
                SettingsError::without_place(
                    &path,
                    format!("themes extend each other in a loop: {}", chain.join(" → ")),
                )
            } else {
                SettingsError::without_place(&path, format!("theme {name:?} not found"))
            });
        }
        if chain.len() >= 8 {
            return Err(SettingsError::without_place(
                &path,
                "too many themes extending each other".to_owned(),
            ));
        }
        chain.push(name.to_owned());

        let text = std::fs::read_to_string(&path).map_err(|error| SettingsError::io(&path, &error))?;
        let partial: PartialTheme =
            toml::from_str(&text).map_err(|error| SettingsError::toml(&path, &text, &error))?;
        let base_name = partial.extends.clone().unwrap_or_else(|| "dark".to_owned());
        let mut theme = Self::load_chain(&base_name, themes_dir, chain)?;
        partial.apply_to(&mut theme);
        Ok(theme)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_fs::TempDir;
    use assert_fs::prelude::*;
    use rstest::rstest;

    #[rstest]
    #[case("#4cc2ff", [0x4c, 0xc2, 0xff, 255])]
    #[case("#4CC2FF80", [0x4c, 0xc2, 0xff, 0x80])]
    #[case("#fff", [255, 255, 255, 255])]
    fn colors(#[case] text: &str, #[case] rgba: [u8; 4]) {
        assert_eq!(Color::parse(text), Ok(Color { rgba }));
    }

    #[rstest]
    #[case("4cc2ff")]
    #[case("#4cc2f")]
    #[case("#ggg")]
    #[case("#4cc2ff8")]
    #[case("#éééééé")]
    #[case("")]
    fn bad_colors(#[case] text: &str) {
        assert!(Color::parse(text).is_err());
    }

    #[test]
    fn built_in_themes_are_complete() {
        for (name, _) in BUILT_IN {
            let theme = Theme::built_in(name).unwrap();
            assert!(!theme.name.is_empty());
        }
        assert!(Theme::built_in("nope").is_none());
    }

    #[test]
    fn dark_theme_snapshot() {
        insta::assert_debug_snapshot!(Theme::built_in("dark").unwrap());
    }

    #[test]
    fn a_theme_can_change_a_few_values() {
        let dir = TempDir::new().unwrap();
        dir.child("orange.toml")
            .write_str("extends = \"light\"\nname = \"Orange\"\n[colors]\naccent = \"#ff8800\"\n[shape]\nradius = 16\n")
            .unwrap();
        let theme = Theme::load("orange", dir.path()).unwrap();
        let light = Theme::built_in("light").unwrap();
        assert_eq!(theme.name, "Orange");
        assert_eq!(theme.colors.accent, Color::parse("#ff8800").unwrap());
        assert_eq!(theme.shape.radius, 16);
        assert_eq!(theme.colors.background, light.colors.background);
        assert_eq!(theme.icons, light.icons);
    }

    #[test]
    fn themes_without_extends_start_from_dark() {
        let dir = TempDir::new().unwrap();
        dir.child("mine.toml").write_str("[font]\nsize = 18\n").unwrap();
        let theme = Theme::load("mine", dir.path()).unwrap();
        assert_eq!(theme.font.size, 18);
        assert_eq!(theme.colors, Theme::built_in("dark").unwrap().colors);
    }

    #[test]
    fn themes_extend_themes() {
        let dir = TempDir::new().unwrap();
        dir.child("base.toml")
            .write_str("extends = \"light\"\n[shape]\nradius = 4\n")
            .unwrap();
        dir.child("child.toml")
            .write_str("extends = \"base\"\n[font]\nsize = 20\n")
            .unwrap();
        let theme = Theme::load("child", dir.path()).unwrap();
        assert_eq!((theme.shape.radius, theme.font.size), (4, 20));
        assert_eq!(theme.colors, Theme::built_in("light").unwrap().colors);
    }

    #[test]
    fn a_user_file_overrides_a_built_in_theme() {
        let dir = TempDir::new().unwrap();
        dir.child("dark.toml")
            .write_str("extends = \"dark\"\n[shape]\nradius = 2\n")
            .unwrap();
        assert_eq!(Theme::load("dark", dir.path()).unwrap().shape.radius, 2);
        dir.child("light.toml")
            .write_str("[shape]\nradius = 3\n")
            .unwrap();
        // No extends: starts from the built-in dark theme.
        let light = Theme::load("light", dir.path()).unwrap();
        assert_eq!(light.shape.radius, 3);
    }

    #[test]
    fn loops_are_reported() {
        let dir = TempDir::new().unwrap();
        dir.child("a.toml").write_str("extends = \"b\"\n").unwrap();
        dir.child("b.toml").write_str("extends = \"a\"\n").unwrap();
        let error = Theme::load("a", dir.path()).unwrap_err();
        assert!(error.message.contains("loop"), "{error}");
    }

    #[test]
    fn unknown_keys_and_bad_values_are_errors_with_their_line() {
        let dir = TempDir::new().unwrap();
        dir.child("typo.toml")
            .write_str("[colors]\naccent = \"#ff8800\"\nbackgruond = \"#000000\"\n")
            .unwrap();
        let error = Theme::load("typo", dir.path()).unwrap_err();
        assert_eq!(error.line, Some(3));
        assert!(error.message.contains("backgruond"), "{error}");

        dir.child("bad.toml")
            .write_str("[colors]\naccent = \"orange\"\n")
            .unwrap();
        let error = Theme::load("bad", dir.path()).unwrap_err();
        assert_eq!(error.line, Some(2));
        assert!(error.message.contains("invalid color"), "{error}");
    }

    #[test]
    fn missing_themes_are_reported() {
        let dir = TempDir::new().unwrap();
        let error = Theme::load("nowhere", dir.path()).unwrap_err();
        assert!(error.message.contains("not found"), "{error}");
    }
}
