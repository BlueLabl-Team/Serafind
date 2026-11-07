//! `config.toml`: behavior and display settings. Every key is optional; unknown keys are errors.
//!
//! The commented file written on first launch (`defaults/config.toml`) documents every key and
//! must give exactly [`Config::default`] (checked by a test).

use serde::Deserialize;

/// Every setting of `config.toml`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Theme choice and overrides of its font and icons.
    pub appearance: Appearance,
    /// Size and place of the bar.
    pub window: Window,
    /// What the bar shows and how.
    pub layout: Layout,
    /// How the bar reacts.
    pub behavior: Behavior,
    /// How many results.
    pub results: Results,
    /// What is indexed.
    pub index: IndexSettings,
    /// The global shortcut.
    pub shortcut: Shortcut,
}

/// `[appearance]`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Appearance {
    /// `"system"`, or a theme name.
    pub theme: String,
    /// Theme used in dark mode when `theme = "system"`.
    pub dark_theme: String,
    /// Theme used in light mode when `theme = "system"`.
    pub light_theme: String,
    /// Replaces the theme's font family.
    pub font_family: Option<String>,
    /// Replaces the theme's font size.
    pub font_size: Option<u16>,
    /// Replaces the theme's icon set.
    pub icons: Option<IconSet>,
}

/// `[window]`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Window {
    /// Width of the bar, in logical pixels.
    pub width: u16,
    /// Distance from the top of the screen, as a fraction of its height.
    pub top: f32,
    /// Screen where the bar opens.
    pub screen: Screen,
}

/// Where the bar opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Screen {
    /// The screen under the mouse pointer.
    Mouse,
    /// The primary screen.
    Primary,
}

/// `[layout]`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Layout {
    /// How the mode is chosen.
    pub mode_selector: ModeSelector,
    /// How the understood parts of the search are shown.
    pub chips: Chips,
    /// How result rows look.
    pub rows: Rows,
}

/// How the mode is chosen in the bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModeSelector {
    /// A discreet selector at the right of the input.
    Dropdown,
    /// A row of tabs always shown above the input.
    Tabs,
}

/// How the understood parts of the search are shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Chips {
    /// Chips under the input.
    Below,
    /// Colored in the input itself.
    Inline,
    /// Not shown.
    Hidden,
}

/// How result rows look.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Rows {
    /// Name and folder on one line.
    Single,
    /// Folder under the name.
    Double,
    /// One line with size and modification date.
    Details,
}

/// `[behavior]`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Behavior {
    /// Hide the bar when it loses the focus.
    pub hide_on_focus_loss: bool,
    /// Show the last search again when the bar opens.
    pub keep_last_query: bool,
    /// What Escape does.
    pub escape: Escape,
}

/// What the Escape key does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Escape {
    /// Close the bar.
    Close,
    /// Clear the search first, close on the next Escape.
    ClearThenClose,
}

/// `[results]`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Results {
    /// Rows shown before scrolling.
    pub visible: u16,
    /// Results kept at most.
    pub max: u16,
}

/// `[index]`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct IndexSettings {
    /// Indexed folders; `~` is the home folder.
    pub roots: Vec<String>,
    /// Folder names never entered.
    pub exclude_names: Vec<String>,
    /// Folders never entered, by path; empty for the system default.
    pub exclude_paths: Vec<String>,
    /// Enter hidden folders too.
    pub hidden_folders: bool,
}

/// `[shortcut]`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Shortcut {
    /// The shortcut, e.g. `Super+F`.
    pub toggle: String,
    /// Replaces `toggle` on Windows.
    pub windows: Option<String>,
    /// Replaces `toggle` on Linux.
    pub linux: Option<String>,
}

impl Shortcut {
    /// The shortcut for the running system.
    #[must_use]
    pub fn for_this_system(&self) -> &str {
        let specific = if cfg!(windows) {
            self.windows.as_deref()
        } else if cfg!(target_os = "linux") {
            self.linux.as_deref()
        } else {
            None
        };
        specific.unwrap_or(&self.toggle)
    }
}

/// Icon sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IconSet {
    /// The built-in Phosphor icon font.
    Phosphor,
    /// No icons.
    None,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: "system".to_owned(),
            dark_theme: "dark".to_owned(),
            light_theme: "light".to_owned(),
            font_family: None,
            font_size: None,
            icons: None,
        }
    }
}

impl Default for Window {
    fn default() -> Self {
        Self {
            width: 680,
            top: 0.25,
            screen: Screen::Mouse,
        }
    }
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            mode_selector: ModeSelector::Dropdown,
            chips: Chips::Below,
            rows: Rows::Single,
        }
    }
}

impl Default for Behavior {
    fn default() -> Self {
        Self {
            hide_on_focus_loss: true,
            keep_last_query: false,
            escape: Escape::Close,
        }
    }
}

impl Default for Results {
    fn default() -> Self {
        Self { visible: 8, max: 50 }
    }
}

impl Default for IndexSettings {
    fn default() -> Self {
        Self {
            roots: vec!["~".to_owned()],
            exclude_names: serafind_index::DEFAULT_NAMES
                .iter()
                .map(|&name| name.to_owned())
                .collect(),
            exclude_paths: Vec::new(),
            hidden_folders: false,
        }
    }
}

impl Default for Shortcut {
    fn default() -> Self {
        Self {
            toggle: "Super+F".to_owned(),
            windows: None,
            linux: None,
        }
    }
}

impl Config {
    /// Values that are well formed but out of range, as `(key, problem)`.
    #[must_use]
    pub fn problems(&self) -> Vec<(&'static str, String)> {
        let mut problems = Vec::new();
        if !(200..=4000).contains(&self.window.width) {
            problems.push(("window.width", "must be between 200 and 4000".to_owned()));
        }
        if !(0.0..=0.9).contains(&self.window.top) {
            problems.push(("window.top", "must be between 0.0 and 0.9".to_owned()));
        }
        if !(1..=100).contains(&self.results.visible) {
            problems.push(("results.visible", "must be between 1 and 100".to_owned()));
        }
        if !(1..=1000).contains(&self.results.max) {
            problems.push(("results.max", "must be between 1 and 1000".to_owned()));
        }
        if self.results.max < self.results.visible {
            problems.push((
                "results.max",
                "must not be smaller than results.visible".to_owned(),
            ));
        }
        if let Some(size) = self.appearance.font_size
            && !(8..=48).contains(&size)
        {
            problems.push(("appearance.font_size", "must be between 8 and 48".to_owned()));
        }
        if self.index.roots.is_empty() {
            problems.push(("index.roots", "needs at least one folder".to_owned()));
        }
        if self.shortcut.for_this_system().trim().is_empty() {
            problems.push(("shortcut.toggle", "must not be empty".to_owned()));
        }
        problems
    }
}
