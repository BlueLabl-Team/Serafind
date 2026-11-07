//! The bar process: the bar in a borderless window, linked to the resident process.
//!
//! The resident process tells it to show, hide or quit. When the bar closes (Escape, focus lost),
//! the window is hidden and kept for next time when the platform allows it and `keep_window` is
//! set; otherwise (always on Wayland, where windows cannot be hidden) the process ends, and the
//! resident process starts a new one next time.

use std::collections::VecDeque;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use eframe::egui::{self, ViewportCommand};
use serafind_ipc::{Client, Event, Response, Stream};
use serafind_settings::{Settings, SettingsWatcher, SystemMode};
use serde::Deserialize;

use crate::bar::{self, Bar, Request};
use crate::search::Searcher;

/// Transparent room around the bar for its shadow.
pub const SHADOW_MARGIN: i8 = 14;

/// The bar's link to the resident process: the events it pushes, and a way to tell it whether
/// the bar is shown.
pub struct Link {
    events: Arc<Mutex<VecDeque<Event>>>,
    writer: Arc<Mutex<Stream>>,
}

/// What arrives on the subscribed connection: events, and the answers to our notifications.
#[derive(Deserialize)]
#[serde(untagged)]
enum Incoming {
    Event(Event),
    Response(
        #[cfg_attr(
            not(test),
            expect(dead_code, reason = "answers to notifications are not needed")
        )]
        Response,
    ),
}

impl Link {
    /// Subscribes to the resident process's events; `ctx` is woken up when one arrives.
    ///
    /// # Errors
    /// When the resident process cannot be reached.
    pub fn subscribe(ctx: &egui::Context) -> io::Result<Self> {
        let mut client = Client::connect()?;
        match client.call(&serafind_ipc::Request::Subscribe)? {
            Response::Ok => {}
            other => return Err(io::Error::other(format!("unexpected answer: {other:?}"))),
        }
        let (mut reader, writer) = client.into_connection().into_parts();
        let events = Arc::new(Mutex::new(VecDeque::new()));
        let queue = Arc::clone(&events);
        let ctx = ctx.clone();
        std::thread::Builder::new()
            .name("serafind-bar events".to_owned())
            .spawn(move || {
                loop {
                    let event = match serafind_ipc::read_message::<Incoming>(&mut reader) {
                        Ok(Some(Incoming::Event(event))) => event,
                        Ok(Some(Incoming::Response(_))) => continue,
                        // The resident process is gone: the bar has nothing to search any more.
                        Ok(None) | Err(_) => Event::Quit,
                    };
                    let quit = event == Event::Quit;
                    if let Ok(mut queue) = queue.lock() {
                        queue.push_back(event);
                    }
                    ctx.request_repaint();
                    if quit {
                        break;
                    }
                }
            })?;
        Ok(Self {
            events,
            writer: Arc::new(Mutex::new(writer)),
        })
    }

    fn take_events(&self) -> Vec<Event> {
        self.events
            .lock()
            .map(|mut queue| queue.drain(..).collect())
            .unwrap_or_default()
    }

    fn tell_visible(&self, visible: bool) {
        if let Ok(mut writer) = self.writer.lock() {
            let _ = serafind_ipc::write_message(&mut *writer, &serafind_ipc::Request::BarVisible { visible });
        }
    }
}

/// What the bar process starts with.
pub struct Startup {
    /// The settings (or the defaults when they could not be read).
    pub settings: Settings,
    /// Why the settings could not be read.
    pub settings_error: Option<String>,
    /// The settings folder, watched for changes.
    pub settings_dir: Option<PathBuf>,
    /// Searches through the resident process.
    pub searcher: Searcher,
    /// Events from the resident process.
    pub link: Option<Link>,
    /// The window may be hidden and kept between openings (not on Wayland).
    pub can_hide: bool,
    /// Why there is no global shortcut, shown in the footer.
    pub shortcut_problem: Option<String>,
}

/// The application: the bar in a borderless window.
#[expect(clippy::struct_excessive_bools, reason = "independent states of the window")]
pub struct App {
    bar: Bar,
    settings_dir: Option<PathBuf>,
    settings_changed: Arc<AtomicBool>,
    _settings_watcher: Option<SettingsWatcher>,
    link: Option<Link>,
    /// The window may be hidden and kept between openings (not on Wayland).
    can_hide: bool,
    hidden: bool,
    mode: SystemMode,
    font_family: String,
    settings_error: Option<String>,
    font_warning: Option<String>,
    shortcut_problem: Option<String>,
    placed: bool,
    was_focused: bool,
    size: egui::Vec2,
    resize: Resize,
    ime: ImeFilter,
}

/// Asks the window for the bar's size until it has it: the very first request can be dropped
/// by a Wayland compositor (it comes before the window is set up), and nothing would ask again
/// until the size changes.
#[derive(Debug, Default)]
struct Resize {
    wanted: egui::Vec2,
    asked_at: Option<std::time::Instant>,
    attempts: u8,
}

impl Resize {
    const RETRY_AFTER: Duration = Duration::from_millis(100);
    const MAX_ATTEMPTS: u8 = 20;

    /// The size to ask for now, if any: a new size, or the same one again while the window
    /// has not taken it (a few times at most, in case the desktop refuses).
    fn request(&mut self, wanted: egui::Vec2, actual: egui::Vec2) -> Option<egui::Vec2> {
        let close = |a: egui::Vec2, b: egui::Vec2| (a - b).length() <= 1.0;
        if !close(wanted, self.wanted) {
            *self = Self {
                wanted,
                asked_at: Some(std::time::Instant::now()),
                attempts: 1,
            };
            return Some(wanted);
        }
        let had_time = self.asked_at.is_none_or(|at| at.elapsed() >= Self::RETRY_AFTER);
        if close(wanted, actual) || !had_time || self.attempts >= Self::MAX_ATTEMPTS {
            return None;
        }
        self.asked_at = Some(std::time::Instant::now());
        self.attempts += 1;
        Some(wanted)
    }
}

impl App {
    /// Builds the app: applies the theme and watches the settings folder.
    #[must_use]
    pub fn new(ctx: &egui::Context, startup: Startup) -> Self {
        let Startup {
            settings,
            settings_error,
            settings_dir,
            searcher,
            link,
            can_hide,
            shortcut_problem,
        } = startup;
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
            bar: Bar::new(settings, searcher),
            settings_dir,
            settings_changed,
            _settings_watcher: watcher,
            link,
            can_hide,
            hidden: false,
            mode,
            font_family,
            settings_error,
            font_warning,
            shortcut_problem,
            placed: false,
            was_focused: false,
            size: egui::Vec2::ZERO,
            resize: Resize::default(),
            ime: ImeFilter::default(),
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
                if settings.theme != previous.theme || settings.config.bar != previous.config.bar {
                    if settings.theme.font.family == self.font_family {
                        crate::style::apply(ctx, &settings);
                    } else {
                        // Loading fonts again is slower: only when the family changes.
                        self.font_warning = bar::apply_theme(ctx, &settings);
                        self.font_family.clone_from(&settings.theme.font.family);
                    }
                }
                if settings.config.window != previous.config.window
                    || settings.config.bar.width != previous.config.bar.width
                {
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
        if let Some(problem) = &self.shortcut_problem {
            problems.push(format!(
                "{}: {problem}",
                crate::strings::Strings::ENGLISH.no_shortcut
            ));
        }
        self.bar.set_problems(problems);
    }

    /// Puts the window at its place on the screen (once the screen size is known), shows it and
    /// asks for the focus.
    fn place(&mut self, ctx: &egui::Context) {
        let monitor = ctx.input(|input| input.viewport().monitor_size);
        let Some(monitor) = monitor else {
            return;
        };
        let settings = &self.bar.settings().config;
        let width = f32::from(settings.bar.width) + 2.0 * f32::from(SHADOW_MARGIN);
        let x = ((monitor.x - width) / 2.0).max(0.0);
        let y = monitor.y * settings.window.top;
        ctx.send_viewport_cmd(ViewportCommand::OuterPosition(egui::pos2(x, y)));
        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
        self.placed = true;
    }

    /// Hides the window for next time, or ends the bar process when it cannot be hidden.
    fn close_bar(&mut self, ctx: &egui::Context) {
        if let Some(link) = &self.link {
            link.tell_visible(false);
        }
        if self.can_hide && self.bar.settings().config.behavior.keep_window {
            self.hidden = true;
            self.was_focused = false;
            self.bar.reset_for_opening();
            ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        } else {
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
    }

    /// Shows the kept window again.
    fn show_again(&mut self, ctx: &egui::Context) {
        if !self.hidden {
            ctx.send_viewport_cmd(ViewportCommand::Focus);
            return;
        }
        self.hidden = false;
        if let Some(link) = &self.link {
            link.tell_visible(true);
        }
        self.place(ctx);
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let events = self.link.as_ref().map(Link::take_events).unwrap_or_default();
        for event in events {
            match event {
                Event::Show { .. } => self.show_again(ctx),
                Event::Hide => self.close_bar(ctx),
                Event::Quit => ctx.send_viewport_cmd(ViewportCommand::Close),
            }
        }
        if self.settings_changed.swap(false, Ordering::AcqRel) {
            self.reload_settings(ctx);
        }
        let mode = system_mode(ctx);
        if mode != self.mode {
            self.mode = mode;
            self.reload_settings(ctx);
        }
        if self.hidden {
            return;
        }
        let focused = ctx.input(|input| input.viewport().focused);
        if focused == Some(true) {
            self.was_focused = true;
        } else if focused == Some(false)
            && self.was_focused
            && self.bar.settings().config.behavior.hide_on_focus_loss
        {
            self.close_bar(ctx);
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

        let width = f32::from(self.bar.settings().config.bar.width) + 2.0 * f32::from(margin);
        let size = egui::vec2(width, content.height().ceil());
        self.size = size;
        if let Some(size) = self.resize.request(size, ctx.content_rect().size()) {
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(size));
        }
        if self.resize.attempts < Resize::MAX_ATTEMPTS && (ctx.content_rect().size() - size).length() > 1.0 {
            // Check again shortly whether the window took the size.
            ctx.request_repaint_after(Resize::RETRY_AFTER);
        }

        for request in self.bar.take_requests() {
            match request {
                Request::Close => self.close_bar(&ctx),
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

    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        self.ime.filter(&mut raw_input.events);
    }
}

/// Drops the empty input-method compositions some input methods (`IBus`) send again and again
/// while nothing is being composed: each one would make egui draw a new frame, which makes the
/// input method answer again, and the bar would never rest.
#[derive(Debug, Default)]
pub struct ImeFilter {
    composing: bool,
}

impl ImeFilter {
    /// Removes the empty compositions that do not end a composition.
    pub fn filter(&mut self, events: &mut Vec<egui::Event>) {
        events.retain(|event| match event {
            egui::Event::Ime(egui::ImeEvent::Preedit { text, .. }) if text.is_empty() => {
                // An empty composition only matters when it ends one in progress.
                std::mem::replace(&mut self.composing, false)
            }
            egui::Event::Ime(egui::ImeEvent::Preedit { .. }) => {
                self.composing = true;
                true
            }
            egui::Event::Ime(egui::ImeEvent::Commit(_)) => {
                self.composing = false;
                true
            }
            _ => true,
        });
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
        color: if theme.mode == serafind_settings::ThemeMode::Dark {
            egui::Color32::from_black_alpha(150)
        } else {
            egui::Color32::from_black_alpha(60)
        },
    };
    ui.painter()
        .add(shadow.as_shape(rect, egui::CornerRadius::same(radius)));
}

fn system_mode(ctx: &egui::Context) -> SystemMode {
    match ctx.system_theme() {
        Some(egui::Theme::Light) => SystemMode::Light,
        _ => SystemMode::Dark,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preedit(text: &str) -> egui::Event {
        egui::Event::Ime(egui::ImeEvent::Preedit {
            text: text.to_owned(),
            active_range_chars: None,
        })
    }

    #[test]
    fn idle_empty_compositions_are_dropped() {
        let mut filter = ImeFilter::default();
        for _ in 0..3 {
            let mut events = vec![preedit("")];
            filter.filter(&mut events);
            assert!(events.is_empty());
        }
    }

    #[test]
    fn compositions_still_work() {
        let mut filter = ImeFilter::default();
        let mut events = vec![
            preedit("^"),
            egui::Event::Ime(egui::ImeEvent::Commit("ê".to_owned())),
            preedit(""),
        ];
        filter.filter(&mut events);
        assert_eq!(events.len(), 2, "the empty one after a commit is not needed");

        let mut events = vec![preedit("^"), preedit("")];
        filter.filter(&mut events);
        assert_eq!(events.len(), 2, "an empty one cancelling a composition is kept");
    }

    /// A moment long enough ago for the resize to be asked again.
    fn a_while_ago() -> Option<std::time::Instant> {
        std::time::Instant::now().checked_sub(Resize::RETRY_AFTER)
    }

    #[test]
    fn resizing_is_asked_again_until_the_window_takes_it() {
        let mut resize = Resize::default();
        let wanted = egui::vec2(708.0, 120.0);
        let small = egui::vec2(708.0, 80.0);
        assert_eq!(
            resize.request(wanted, small),
            Some(wanted),
            "a new size is asked at once"
        );
        assert_eq!(resize.request(wanted, small), None, "not again right away");
        resize.asked_at = a_while_ago();
        assert_eq!(
            resize.request(wanted, small),
            Some(wanted),
            "again once the window had time"
        );
        resize.asked_at = a_while_ago();
        assert_eq!(resize.request(wanted, wanted), None, "not when the window has it");
        resize.attempts = Resize::MAX_ATTEMPTS;
        resize.asked_at = a_while_ago();
        assert_eq!(
            resize.request(wanted, small),
            None,
            "gives up when the desktop refuses"
        );
        let other = egui::vec2(708.0, 300.0);
        assert_eq!(
            resize.request(other, small),
            Some(other),
            "a new size starts over"
        );
    }

    #[test]
    fn subscribed_connections_carry_events_and_answers() {
        let event: Incoming = serde_json::from_str(r#"{"type":"show"}"#).unwrap();
        assert!(matches!(
            event,
            Incoming::Event(Event::Show {
                activation_token: None
            })
        ));
        let answer: Incoming = serde_json::from_str(r#"{"type":"ok"}"#).unwrap();
        assert!(matches!(answer, Incoming::Response(Response::Ok)));
    }
}
