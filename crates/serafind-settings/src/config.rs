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
    /// Dimensions of the bar.
    pub bar: BarSettings,
    /// Place of the bar on the screen.
    pub window: Window,
    /// What the bar shows and how.
    pub layout: Layout,
    /// How the bar reacts.
    pub behavior: Behavior,
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
    /// Replaces the theme's icon set.
    pub icons: Option<IconSet>,
}

/// `[bar]`: every dimension of the bar, whatever the theme.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BarSettings {
    /// Width of the bar, in logical pixels.
    pub width: u16,
    /// Inner spacing.
    pub padding: u16,
    /// Height of a result row.
    pub row_height: u16,
    /// Text size (the search input is a little larger).
    pub font_size: u16,
    /// Rows shown before scrolling.
    pub visible_results: u16,
    /// Results kept at most (scrolling).
    pub max_results: u16,
}

/// `[window]`: where the bar appears.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Window {
    /// Distance from the top of the screen, as a fraction of its height.
    pub top: f32,
    /// Screen where the bar opens.
    pub screen: Screen,
    /// Linux only: how the window is shown.
    pub backend: Backend,
}

/// Linux only: the display protocol of the bar window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Backend {
    /// X11 (through `XWayland` on a Wayland desktop): exact position, but GNOME may refuse it the
    /// keyboard focus.
    X11,
    /// Native Wayland: keyboard focus and sharp text; the desktop chooses the position.
    Wayland,
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
            icons: None,
        }
    }
}

impl Default for BarSettings {
    fn default() -> Self {
        Self {
            width: 760,
            padding: 12,
            row_height: 38,
            font_size: 14,
            visible_results: 8,
            max_results: 50,
        }
    }
}

impl Default for Window {
    fn default() -> Self {
        Self {
            top: 0.25,
            screen: Screen::Mouse,
            backend: Backend::Wayland,
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
        let mut check = |ok: bool, key: &'static str, problem: &str| {
            if !ok {
                problems.push((key, problem.to_owned()));
            }
        };
        let bar = &self.bar;
        check(
            (200..=4000).contains(&bar.width),
            "bar.width",
            "must be between 200 and 4000",
        );
        check(bar.padding <= 64, "bar.padding", "must be between 0 and 64");
        check(
            (16..=120).contains(&bar.row_height),
            "bar.row_height",
            "must be between 16 and 120",
        );
        check(
            (8..=48).contains(&bar.font_size),
            "bar.font_size",
            "must be between 8 and 48",
        );
        check(
            (1..=100).contains(&bar.visible_results),
            "bar.visible_results",
            "must be between 1 and 100",
        );
        check(
            (1..=1000).contains(&bar.max_results),
            "bar.max_results",
            "must be between 1 and 1000",
        );
        check(
            bar.max_results >= bar.visible_results,
            "bar.max_results",
            "must not be smaller than bar.visible_results",
        );
        check(
            (0.0..=0.9).contains(&self.window.top),
            "window.top",
            "must be between 0.0 and 0.9",
        );
        check(
            !self.index.roots.is_empty(),
            "index.roots",
            "needs at least one folder",
        );
        check(
            !self.shortcut.for_this_system().trim().is_empty(),
            "shortcut.toggle",
            "must not be empty",
        );
        problems
    }
}
