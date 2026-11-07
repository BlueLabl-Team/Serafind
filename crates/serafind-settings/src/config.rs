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
    /// The icon in the notification area.
    pub tray: Tray,
    /// How actions on a result are done.
    pub actions: Actions,
    /// The keys of the bar.
    pub keys: BarKeys,
    /// The Content mode.
    pub content: ContentSettings,
}

/// `[content]`: the search inside files.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ContentSettings {
    /// Files larger than this (in megabytes) are not read.
    pub max_file_size: u32,
    /// The search starts after this pause in typing, in milliseconds.
    pub typing_delay: u32,
}

impl Default for ContentSettings {
    fn default() -> Self {
        Self {
            max_file_size: 10,
            typing_delay: 250,
        }
    }
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
    /// Icons of applications: their own, or the generic application icon of the icon set.
    pub app_icons: AppIcons,
}

/// Icons of applications.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AppIcons {
    /// Each application's own icon (from the icon theme of the desktop).
    Real,
    /// The application icon of the icon set, the same for all.
    Generic,
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
#[expect(clippy::struct_excessive_bools, reason = "independent on/off settings")]
pub struct Behavior {
    /// Hide the bar when it loses the focus.
    pub hide_on_focus_loss: bool,
    /// Show the last search again when the bar opens.
    pub keep_last_query: bool,
    /// What Escape does.
    pub escape: Escape,
    /// Keep the bar's window hidden between openings (instant, more memory) instead of closing
    /// it. Not possible on Wayland, where the window is always closed.
    pub keep_window: bool,
    /// How a result is opened with the mouse.
    pub click: Click,
    /// Close the bar after opening a result or showing it in its folder.
    pub close_after_open: bool,
    /// Close the bar after copying a path or a name.
    pub close_after_copy: bool,
    /// Close the bar after opening a terminal.
    pub close_after_terminal: bool,
    /// Put the applications started most often first (counts kept in `usage.json`).
    pub learn_from_usage: bool,
}

/// How a result is opened with the mouse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Click {
    /// A click selects, a double click opens.
    Double,
    /// A click opens.
    Single,
}

/// `[actions]`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Actions {
    /// `"auto"`, or the terminal command; `{dir}` is replaced by the folder.
    pub terminal: String,
    /// `"auto"`, or the command that opens a file at a line; `{file}` and `{line}` are replaced.
    pub open_at_line: String,
}

impl Default for Actions {
    fn default() -> Self {
        Self {
            terminal: "auto".to_owned(),
            open_at_line: "auto".to_owned(),
        }
    }
}

/// `[keys]`: the keys of the bar, each written like `Ctrl+Shift+C`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BarKeys {
    /// Open the selected result.
    pub open: String,
    /// Show the selected result in its folder.
    pub reveal: String,
    /// Copy the full path of the selected result.
    pub copy_path: String,
    /// Copy the name of the selected result.
    pub copy_name: String,
    /// Open a terminal in the folder of the selected result.
    pub terminal: String,
    /// Close the bar.
    pub close: String,
    /// Select the next result.
    pub next_result: String,
    /// Select the previous result.
    pub previous_result: String,
    /// Select one page of results further.
    pub next_page: String,
    /// Select one page of results back.
    pub previous_page: String,
    /// Next mode.
    pub next_mode: String,
    /// Previous mode.
    pub previous_mode: String,
    /// Accept the proposed folder of `in:` (wins over `next_mode` when it is the same key).
    pub complete: String,
    /// Modifiers that, with a digit 1 to 5, choose a mode.
    pub mode_with_digit: String,
    /// Modifiers that, with a digit 1 to 9, open that result.
    pub open_with_digit: String,
}

impl Default for BarKeys {
    fn default() -> Self {
        let text = |text: &str| text.to_owned();
        Self {
            open: text("Enter"),
            reveal: text("Ctrl+Enter"),
            copy_path: text("Ctrl+C"),
            copy_name: text("Ctrl+Shift+C"),
            terminal: text("Ctrl+T"),
            close: text("Escape"),
            next_result: text("Down"),
            previous_result: text("Up"),
            next_page: text("PageDown"),
            previous_page: text("PageUp"),
            next_mode: text("Tab"),
            previous_mode: text("Shift+Tab"),
            complete: text("Tab"),
            mode_with_digit: text("Ctrl"),
            open_with_digit: text("Alt"),
        }
    }
}

impl BarKeys {
    /// Every key setting with its name, except the modifier-only ones.
    #[must_use]
    pub fn named(&self) -> [(&'static str, &str); 13] {
        [
            ("open", &self.open),
            ("reveal", &self.reveal),
            ("copy_path", &self.copy_path),
            ("copy_name", &self.copy_name),
            ("terminal", &self.terminal),
            ("close", &self.close),
            ("next_result", &self.next_result),
            ("previous_result", &self.previous_result),
            ("next_page", &self.next_page),
            ("previous_page", &self.previous_page),
            ("next_mode", &self.next_mode),
            ("previous_mode", &self.previous_mode),
            ("complete", &self.complete),
        ]
    }
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
    /// Serafind sets up the global shortcut itself (false: bind `serafind toggle` in the desktop).
    pub enabled: bool,
    /// The shortcut, e.g. `Super+F`.
    pub toggle: String,
    /// Replaces `toggle` on Windows.
    pub windows: Option<String>,
    /// Replaces `toggle` on Linux.
    pub linux: Option<String>,
}

/// `[tray]`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Tray {
    /// Show an icon in the notification area (system tray).
    pub enabled: bool,
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
            app_icons: AppIcons::Real,
        }
    }
}

impl Default for BarSettings {
    fn default() -> Self {
        Self {
            width: 680,
            padding: 10,
            row_height: 34,
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
            keep_window: true,
            click: Click::Double,
            close_after_open: true,
            close_after_copy: false,
            close_after_terminal: true,
            learn_from_usage: false,
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
            enabled: true,
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
        let shortcut_key = if cfg!(windows) && self.shortcut.windows.is_some() {
            "shortcut.windows"
        } else if cfg!(target_os = "linux") && self.shortcut.linux.is_some() {
            "shortcut.linux"
        } else {
            "shortcut.toggle"
        };
        if let Err(problem) = crate::Keys::parse(self.shortcut.for_this_system()) {
            problems.push((shortcut_key, problem));
        }
        for (name, text) in self.keys.named() {
            if let Err(problem) = crate::Keys::parse_bar(text) {
                problems.push((key_setting(name), problem));
            }
        }
        for (name, text) in [
            ("mode_with_digit", &self.keys.mode_with_digit),
            ("open_with_digit", &self.keys.open_with_digit),
        ] {
            match crate::Modifiers::parse(text) {
                Ok(modifiers) if modifiers.logo => {
                    problems.push((key_setting(name), "the bar cannot use Super".to_owned()));
                }
                Ok(_) => {}
                Err(problem) => problems.push((key_setting(name), problem)),
            }
        }
        if self.actions.open_at_line.trim().is_empty() {
            problems.push(("actions.open_at_line", "must be \"auto\" or a command".to_owned()));
        }
        if !(1..=1024).contains(&self.content.max_file_size) {
            problems.push(("content.max_file_size", "must be between 1 and 1024".to_owned()));
        }
        if self.content.typing_delay > 2000 {
            problems.push(("content.typing_delay", "must be between 0 and 2000".to_owned()));
        }
        if self.actions.terminal.trim().is_empty() {
            problems.push(("actions.terminal", "must be \"auto\" or a command".to_owned()));
        }
        problems
    }
}

/// `keys.<name>` as a static string, for error messages.
fn key_setting(name: &str) -> &'static str {
    match name {
        "open" => "keys.open",
        "reveal" => "keys.reveal",
        "copy_path" => "keys.copy_path",
        "copy_name" => "keys.copy_name",
        "terminal" => "keys.terminal",
        "close" => "keys.close",
        "next_result" => "keys.next_result",
        "previous_result" => "keys.previous_result",
        "next_page" => "keys.next_page",
        "previous_page" => "keys.previous_page",
        "next_mode" => "keys.next_mode",
        "previous_mode" => "keys.previous_mode",
        "complete" => "keys.complete",
        "mode_with_digit" => "keys.mode_with_digit",
        _ => "keys.open_with_digit",
    }
}
