//! The window around the bar: placement, size, focus, settings reload and the index in the
//! background.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use eframe::egui::{self, ViewportCommand};
use serafind_index::{Exclusions, LiveIndex};
use serafind_settings::{IndexSettings, Settings, SettingsWatcher, SystemMode};

use crate::bar::{self, Bar, Request};
use crate::search::{Searcher, SharedIndex};

/// Transparent room around the bar for its shadow.
pub const SHADOW_MARGIN: i8 = 14;

/// The application: the bar in a borderless window.
pub struct App {
    bar: Bar,
    settings_dir: Option<PathBuf>,
    settings_changed: Arc<AtomicBool>,
    _settings_watcher: Option<SettingsWatcher>,
    /// Keeps the watcher of the index alive.
    _live_index: Arc<Mutex<Option<LiveIndex>>>,
    mode: SystemMode,
    font_family: String,
    settings_error: Option<String>,
    font_warning: Option<String>,
    placed: bool,
    was_focused: bool,
    size: egui::Vec2,
}

impl App {
    /// Builds the app: applies the theme, starts the index in the background and watches the
    /// settings folder.
    #[must_use]
    pub fn new(
        ctx: &egui::Context,
        settings: Settings,
        settings_error: Option<String>,
        settings_dir: Option<PathBuf>,
        home: PathBuf,
    ) -> Self {
        let shared: SharedIndex = Arc::new(RwLock::new(None));
        let live_index = Arc::new(Mutex::new(None));
        start_index(ctx, &settings.config.index, &home, &shared, &live_index);

        let settings_changed = Arc::new(AtomicBool::new(false));
        let watcher = settings_dir.as_ref().and_then(|dir| {
            let changed = Arc::clone(&settings_changed);
            let ctx = ctx.clone();
            SettingsWatcher::start(dir, Duration::from_millis(150), move || {
                changed.store(true, Ordering::Release);
                ctx.request_repaint();
            })
            .ok()
        });

        let font_warning = bar::apply_theme(ctx, &settings);
        let font_family = settings.theme.font.family.clone();
        let mode = system_mode(ctx);
        let mut app = Self {
            bar: Bar::new(settings, Searcher::new(shared, home)),
            settings_dir,
            settings_changed,
            _settings_watcher: watcher,
            _live_index: live_index,
            mode,
            font_family,
            settings_error,
            font_warning,
            placed: false,
            was_focused: false,
            size: egui::Vec2::ZERO,
        };
        app.update_problems();
        app
    }

    fn reload_settings(&mut self, ctx: &egui::Context) {
        let Some(dir) = &self.settings_dir else {
            return;
        };
        match serafind_settings::load(dir, self.mode) {
            Ok(settings) => {
                let previous = self.bar.settings().clone();
                if settings.theme != previous.theme {
                    if settings.theme.font.family == self.font_family {
                        style_only(ctx, &settings);
                    } else {
                        // Loading fonts again is slower: only when the family changes.
                        self.font_warning = bar::apply_theme(ctx, &settings);
                        self.font_family.clone_from(&settings.theme.font.family);
                    }
                }
                if settings.config.window != previous.config.window {
                    self.placed = false;
                }
                self.bar.set_settings(settings);
                self.settings_error = None;
            }
            Err(error) => self.settings_error = Some(error.to_string()),
        }
        self.update_problems();
    }

    fn update_problems(&mut self) {
        let mut problems = Vec::new();
        if let Some(error) = &self.settings_error {
            problems.push(format!(
                "{}: {error}",
                crate::strings::Strings::ENGLISH.settings_error
            ));
        }
        if let Some(warning) = &self.font_warning {
            problems.push(warning.clone());
        }
        self.bar.set_problems(problems);
    }

    /// Puts the window at its place on the screen (once the screen size is known).
    fn place(&mut self, ctx: &egui::Context) {
        let monitor = ctx.input(|input| input.viewport().monitor_size);
        let Some(monitor) = monitor else {
            return;
        };
        let window = &self.bar.settings().config.window;
        let width = f32::from(window.width) + 2.0 * f32::from(SHADOW_MARGIN);
        let x = ((monitor.x - width) / 2.0).max(0.0);
        let y = monitor.y * window.top;
        ctx.send_viewport_cmd(ViewportCommand::OuterPosition(egui::pos2(x, y)));
        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
        self.placed = true;
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.settings_changed.swap(false, Ordering::AcqRel) {
            self.reload_settings(ctx);
        }
        let mode = system_mode(ctx);
        if mode != self.mode {
            self.mode = mode;
            self.reload_settings(ctx);
        }
        let focused = ctx.input(|input| input.viewport().focused);
        if focused == Some(true) {
            self.was_focused = true;
        } else if focused == Some(false)
            && self.was_focused
            && self.bar.settings().config.behavior.hide_on_focus_loss
        {
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if !self.placed {
            self.place(&ctx);
        }
        let shadow = self.bar.settings().theme.shape.shadow;
        let margin: i8 = if shadow { SHADOW_MARGIN } else { 0 };
        let content = egui::Frame::new()
            .inner_margin(egui::Margin::same(margin))
            .show(ui, |ui| {
                if shadow {
                    paint_shadow(ui, &self.bar.settings().theme);
                }
                self.bar.ui(ui);
            })
            .response
            .rect;

        let width = f32::from(self.bar.settings().config.window.width) + 2.0 * f32::from(margin);
        let size = egui::vec2(width, content.height().ceil());
        if (size - self.size).length() > 0.5 {
            self.size = size;
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(size));
        }

        for request in self.bar.take_requests() {
            match request {
                Request::Close => ctx.send_viewport_cmd(ViewportCommand::Close),
                // Opening and showing in the folder come with step 5 of the roadmap.
                Request::Open(_) | Request::Reveal(_) => {
                    self.bar.notify(crate::strings::Strings::ENGLISH.action_not_ready);
                }
            }
        }
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }
}

/// A soft shadow behind the bar, drawn in the transparent margin.
fn paint_shadow(ui: &egui::Ui, theme: &serafind_settings::Theme) {
    let rect = ui.max_rect();
    let radius = u8::try_from(theme.shape.radius).unwrap_or(u8::MAX);
    let shadow = egui::Shadow {
        offset: [0, 4],
        blur: 16,
        spread: 0,
        color: crate::style::color(theme.colors.shadow),
    };
    ui.painter()
        .add(shadow.as_shape(rect, egui::CornerRadius::same(radius)));
}

fn style_only(ctx: &egui::Context, settings: &Settings) {
    crate::style::apply(ctx, &settings.theme);
}

fn system_mode(ctx: &egui::Context) -> SystemMode {
    match ctx.system_theme() {
        Some(egui::Theme::Light) => SystemMode::Light,
        _ => SystemMode::Dark,
    }
}

/// Builds the index in a background thread, then keeps it up to date; the bar shows
/// "Indexing…" until it is ready.
fn start_index(
    ctx: &egui::Context,
    settings: &IndexSettings,
    home: &std::path::Path,
    shared: &SharedIndex,
    live_index: &Arc<Mutex<Option<LiveIndex>>>,
) {
    let roots: Vec<PathBuf> = settings
        .roots
        .iter()
        .map(|root| expand_home(root, home))
        .collect();
    let mut exclusions = Exclusions::defaults(home);
    exclusions.names.clone_from(&settings.exclude_names);
    if !settings.exclude_paths.is_empty() {
        exclusions.paths = settings
            .exclude_paths
            .iter()
            .map(|path| expand_home(path, home))
            .collect();
    }
    exclusions.hidden_folders = !settings.hidden_folders;

    let ctx = ctx.clone();
    let shared = Arc::clone(shared);
    let live_index = Arc::clone(live_index);
    std::thread::Builder::new()
        .name("serafind-index start".to_owned())
        .spawn(move || {
            if let Ok(live) = LiveIndex::start(roots, exclusions, serafind_index::DEFAULT_DELAY) {
                if let Ok(mut slot) = shared.write() {
                    *slot = Some(Arc::clone(live.index()));
                }
                if let Ok(mut slot) = live_index.lock() {
                    *slot = Some(live);
                }
                ctx.request_repaint();
            }
        })
        .expect("a thread can be started");
}

/// `~` and `~/x` from the home folder.
#[must_use]
pub fn expand_home(path: &str, home: &std::path::Path) -> PathBuf {
    if path == "~" {
        home.to_path_buf()
    } else if let Some(rest) = path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        home.join(rest)
    } else {
        PathBuf::from(path)
    }
}
