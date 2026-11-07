//! Settings and themes of Serafind.
//!
//! Everything lives in one folder ([`default_dir`]): `config.toml` and a `themes` folder. Both
//! are written, commented, on first launch ([`write_defaults`]); a missing file means the
//! defaults. [`load`] reads them; [`SettingsWatcher`] says when they change, so they can be read
//! again. When a file cannot be used, [`load`] returns the error and the caller keeps the
//! previous settings.
//!
//! This crate knows nothing about the user interface: colors are plain RGBA values.

mod config;
mod error;
mod theme;
mod watch;

use std::io;
use std::path::{Path, PathBuf};

pub use config::{
    Appearance, Backend, Behavior, Chips, Config, Escape, IconSet, IndexSettings, Layout, ModeSelector,
    Results, Rows, Screen, Shortcut, Window,
};
pub use error::SettingsError;
pub use theme::{BUILT_IN, Color, Colors, Font, IconColors, Icons, Shape, Theme, ThemeMode};
pub use watch::SettingsWatcher;

/// The commented `config.toml` written on first launch.
pub const DEFAULT_CONFIG: &str = include_str!("../defaults/config.toml");

/// Name of the settings file.
pub const CONFIG_FILE: &str = "config.toml";

/// Name of the themes folder.
pub const THEMES_DIR: &str = "themes";

/// Whether the desktop is in dark or light mode (for `theme = "system"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SystemMode {
    /// Dark mode (also when unknown).
    #[default]
    Dark,
    /// Light mode.
    Light,
}

/// Settings ready to use: the configuration and its theme (with the configuration's overrides).
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// `config.toml`.
    pub config: Config,
    /// The theme in use.
    pub theme: Theme,
}

impl Settings {
    /// The defaults, without reading any file.
    ///
    /// # Panics
    /// Never in practice: the default themes are built in (checked by tests).
    #[must_use]
    pub fn defaults(mode: SystemMode) -> Self {
        let config = Config::default();
        let name = theme_name(&config, mode);
        let mut theme = Theme::built_in(name).expect("default themes are built in");
        apply_overrides(&config, &mut theme);
        Self { config, theme }
    }
}

/// The settings folder: `~/.config/serafind` on Linux, `%APPDATA%\Serafind` on Windows.
#[must_use]
pub fn default_dir() -> Option<PathBuf> {
    let name = if cfg!(windows) { "Serafind" } else { "serafind" };
    dirs::config_dir().map(|dir| dir.join(name))
}

/// Writes the commented default files that are missing (never replaces a file). Returns the files
/// written.
///
/// # Errors
/// When the folder or a file cannot be created.
pub fn write_defaults(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let themes = dir.join(THEMES_DIR);
    std::fs::create_dir_all(&themes)?;
    let mut written = Vec::new();
    let files = std::iter::once((dir.join(CONFIG_FILE), DEFAULT_CONFIG)).chain(
        BUILT_IN
            .iter()
            .map(|(name, text)| (themes.join(format!("{name}.toml")), *text)),
    );
    for (path, text) in files {
        if !path.exists() {
            std::fs::write(&path, text)?;
            written.push(path);
        }
    }
    Ok(written)
}

/// Reads `config.toml` (defaults when missing) and the theme it chooses.
///
/// # Errors
/// The first problem found: unreadable file, syntax error, unknown key, wrong or out-of-range
/// value, missing theme.
pub fn load(dir: &Path, mode: SystemMode) -> Result<Settings, SettingsError> {
    let path = dir.join(CONFIG_FILE);
    let config = if path.is_file() {
        let text = std::fs::read_to_string(&path).map_err(|error| SettingsError::io(&path, &error))?;
        let config: Config =
            toml::from_str(&text).map_err(|error| SettingsError::toml(&path, &text, &error))?;
        if let Some((key, problem)) = config.problems().into_iter().next() {
            return Err(SettingsError::value(&path, &text, key, &problem));
        }
        config
    } else {
        Config::default()
    };
    let mut theme = Theme::load(theme_name(&config, mode), &dir.join(THEMES_DIR))?;
    apply_overrides(&config, &mut theme);
    Ok(Settings { config, theme })
}

fn theme_name(config: &Config, mode: SystemMode) -> &str {
    let appearance = &config.appearance;
    match (appearance.theme.as_str(), mode) {
        ("system", SystemMode::Dark) => &appearance.dark_theme,
        ("system", SystemMode::Light) => &appearance.light_theme,
        (name, _) => name,
    }
}

fn apply_overrides(config: &Config, theme: &mut Theme) {
    let appearance = &config.appearance;
    if let Some(family) = &appearance.font_family {
        theme.font.family.clone_from(family);
    }
    if let Some(size) = appearance.font_size {
        theme.font.size = size;
    }
    if let Some(icons) = appearance.icons {
        theme.icons.set = icons;
    }
}

#[cfg(test)]
mod tests;
