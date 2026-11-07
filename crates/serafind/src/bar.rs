//! The search bar: input, mode, chips, results and footer, with its keyboard handling.
//!
//! The bar only draws itself into a [`egui::Ui`] and records what it wants from the window
//! ([`Request`]): it runs the same in the real window and in tests.

use std::path::{Path, PathBuf};

use serafind_ipc::Action;

use eframe::egui::{
    self, Align, Color32, CornerRadius, FontFamily, FontId, Layout, Margin, RichText, Sense, Stroke,
    TextFormat, Ui, text::LayoutJob,
};
use serafind_core::{EntryKind, PartKind, SearchMode};
use serafind_settings::{AppIcons, Chips, Click, Escape, IconSet, ModeSelector, Rows, Settings};

use crate::completion::{complete_folder, typed_folder};
use crate::icons::{self, Family};
use crate::search::{Outcome, Row, Searcher};
use crate::strings::Strings;
use crate::style::{self, BOLD, ICONS, color};

mod bindings;

use bindings::{Bindings, Command, DIGITS};

/// What the bar asks from the window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// Close (or hide) the bar.
    Close,
    /// Do something with an entry (open it, show it in its folder, open a terminal there).
    Act(Action, PathBuf),
}

/// The search bar and its state.
pub struct Bar {
    settings: Settings,
    strings: Strings,
    searcher: Searcher,
    line: String,
    tab: SearchMode,
    outcome: Outcome,
    /// What `outcome` was computed for: line, tab, maximum.
    searched: Option<(String, SearchMode, usize)>,
    /// The line and tab of the last search, to keep the selection when it runs again.
    last_search: Option<(String, SearchMode)>,
    selected: usize,
    scroll_to_selected: bool,
    completions: Vec<String>,
    completion: usize,
    notice: Option<String>,
    problems: Vec<String>,
    requests: Vec<Request>,
    focus_input: bool,
    bindings: Bindings,
    /// Icons of applications already loaded (`None`: could not be read).
    app_icons: std::cell::RefCell<std::collections::HashMap<PathBuf, Option<egui::TextureHandle>>>,
}

const INPUT_ID: &str = "serafind-input";

impl Bar {
    /// A bar searching with `searcher`, drawn with `settings`.
    #[must_use]
    pub fn new(settings: Settings, searcher: Searcher) -> Self {
        Self {
            bindings: Bindings::new(&settings.config.keys),
            app_icons: std::cell::RefCell::default(),
            settings,
            strings: Strings::ENGLISH,
            searcher,
            line: String::new(),
            tab: SearchMode::All,
            outcome: Outcome::default(),
            searched: None,
            last_search: None,
            selected: 0,
            scroll_to_selected: false,
            completions: Vec::new(),
            completion: 0,
            notice: None,
            problems: Vec::new(),
            requests: Vec::new(),
            focus_input: true,
        }
    }

    /// The settings in use.
    #[must_use]
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Uses new settings (after a reload).
    pub fn set_settings(&mut self, settings: Settings) {
        self.bindings = Bindings::new(&settings.config.keys);
        self.settings = settings;
        self.searched = None;
    }

    /// Problems to show in the footer (settings errors, missing font...); empty when all is well.
    pub fn set_problems(&mut self, problems: Vec<String>) {
        self.problems = problems;
    }

    /// The search line.
    #[cfg(test)]
    #[must_use]
    pub fn line(&self) -> &str {
        &self.line
    }

    /// The mode chosen with the tabs or the selector (a mode filter of the line may win).
    #[cfg(test)]
    #[must_use]
    pub fn tab(&self) -> SearchMode {
        self.tab
    }

    /// The last search.
    #[cfg(test)]
    #[must_use]
    pub fn outcome(&self) -> &Outcome {
        &self.outcome
    }

    /// The message shown in the footer.
    #[cfg(test)]
    #[must_use]
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    /// The selected row.
    #[cfg(test)]
    #[must_use]
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// Takes what the bar asked from the window since the last call.
    pub fn take_requests(&mut self) -> Vec<Request> {
        std::mem::take(&mut self.requests)
    }

    /// Does what an [`Request::Act`] asked, through the resident process. Returns true when the
    /// bar should close now; on failure the reason is shown in the footer.
    pub fn perform(&mut self, action: Action, path: &Path) -> bool {
        match self.searcher.act(action, path) {
            Ok(()) => {
                let behavior = &self.settings.config.behavior;
                match action {
                    Action::Open | Action::Reveal | Action::Launch => behavior.close_after_open,
                    Action::Terminal => behavior.close_after_terminal,
                }
            }
            Err(message) => {
                self.notice = Some(message);
                false
            }
        }
    }

    /// Prepares the bar for being shown again: empty, or the last search when asked.
    pub fn reset_for_opening(&mut self) {
        if !self.settings.config.behavior.keep_last_query {
            self.line.clear();
            self.tab = SearchMode::All;
        }
        self.selected = 0;
        self.notice = None;
        self.focus_input = true;
        self.searched = None;
        self.last_search = None;
    }

    /// Draws the bar and handles its keys.
    pub fn ui(&mut self, ui: &mut Ui) {
        self.handle_keys(ui.ctx());
        self.refresh(&ui.ctx().clone());
        let theme = self.settings.theme.clone();
        let colors = &theme.colors;
        let radius = u8::try_from(theme.shape.radius).unwrap_or(u8::MAX);
        let padding = i8::try_from(self.settings.config.bar.padding).unwrap_or(i8::MAX);

        egui::Frame::new()
            .fill(color(colors.surface_elevated))
            .stroke(Stroke::new(1.0, color(colors.accent)))
            .corner_radius(CornerRadius::same(radius))
            .inner_margin(Margin::same(padding))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                if self.settings.config.layout.mode_selector == ModeSelector::Tabs {
                    self.tabs(ui);
                }
                self.input_row(ui);
                if self.line.trim().is_empty() {
                    ui.label(
                        RichText::new(self.strings.filters_hint)
                            .small()
                            .color(color(colors.text_secondary)),
                    );
                } else if self.settings.config.layout.chips == Chips::Below
                    && !self.outcome.query.parts.is_empty()
                {
                    self.chips(ui);
                }
                if !self.outcome.rows.is_empty() {
                    ui.separator();
                    self.results(ui);
                }
                if !self.line.trim().is_empty() || !self.problems.is_empty() {
                    self.footer(ui);
                }
            });
    }

    // --- Search ---

    fn refresh(&mut self, ctx: &egui::Context) {
        let max = usize::from(self.settings.config.bar.max_results);
        let key = (self.line.clone(), self.tab, max);
        if self.searched.as_ref() == Some(&key) {
            return;
        }
        self.outcome = self.searcher.run(&self.line, self.tab, max);
        if self.outcome.indexing || self.outcome.unreachable {
            // Ask again shortly: the index will be ready, or the resident process back.
            self.searched = None;
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        } else {
            self.searched = Some(key);
        }
        // The same search again (settings reloaded, index ready): keep the selection.
        let same_search = self.last_search.as_ref() == Some(&(self.line.clone(), self.tab));
        if same_search {
            self.selected = self.selected.min(self.outcome.rows.len().saturating_sub(1));
        } else {
            self.selected = 0;
            self.scroll_to_selected = true;
            self.notice = None;
            self.last_search = Some((self.line.clone(), self.tab));
        }
        self.update_completions();
    }

    fn update_completions(&mut self) {
        self.completions = typed_folder(&self.line, self.line.len())
            .map(|(_, typed)| complete_folder(typed, self.searcher.home(), self.searcher.home(), 20))
            .unwrap_or_default();
        self.completion = 0;
    }

    /// The rest of the proposed folder after what is typed (shown greyed, Tab accepts it).
    fn ghost(&self) -> Option<&str> {
        let (_, typed) = typed_folder(&self.line, self.line.len())?;
        let proposal = self.completions.get(self.completion)?;
        proposal
            .get(typed.len()..)
            .filter(|rest| !rest.is_empty() && proposal.starts_with(typed))
    }

    // --- Keys ---

    fn handle_keys(&mut self, ctx: &egui::Context) {
        let rows = self.outcome.rows.len();
        let page = usize::from(self.settings.config.bar.visible_results).max(1);
        let bindings = self.bindings.clone();
        for (modifiers, key, command) in bindings.keys() {
            // Leave the key to the input when the command has nothing to do.
            let applies = match command {
                Command::Complete => {
                    self.ghost().is_some() || (!self.completions.is_empty() && self.line.ends_with('/'))
                }
                Command::CopyPath | Command::CopyName => Self::input_has_no_selection(ctx),
                _ => true,
            };
            if applies && Self::pressed(ctx, modifiers, key) {
                self.run(ctx, command, rows, page);
            }
        }
        for (index, key) in DIGITS.into_iter().enumerate() {
            if index < SearchMode::ALL.len()
                && ctx.input_mut(|input| input.consume_key(bindings.mode_with_digit, key))
            {
                self.tab = SearchMode::ALL[index];
            }
            if ctx.input_mut(|input| input.consume_key(bindings.open_with_digit, key)) {
                self.act(index, Action::Open);
            }
        }
    }

    /// Whether `modifiers` + `key` was pressed (and takes it). In the real window Ctrl+C and
    /// Ctrl+X do not arrive as keys but as Copy and Cut events: those count for them.
    fn pressed(ctx: &egui::Context, modifiers: egui::Modifiers, key: egui::Key) -> bool {
        let as_event = if modifiers == egui::Modifiers::COMMAND {
            match key {
                egui::Key::C => Some(egui::Event::Copy),
                egui::Key::X => Some(egui::Event::Cut),
                _ => None,
            }
        } else {
            None
        };
        ctx.input_mut(|input| {
            if input.consume_key(modifiers, key) {
                return true;
            }
            let Some(wanted) = as_event else {
                return false;
            };
            let before = input.events.len();
            input.events.retain(|event| *event != wanted);
            input.events.len() != before
        })
    }

    fn run(&mut self, ctx: &egui::Context, command: Command, rows: usize, page: usize) {
        match command {
            Command::Open => self.act(self.selected, Action::Open),
            Command::Reveal => self.act(self.selected, Action::Reveal),
            Command::Terminal => self.act(self.selected, Action::Terminal),
            Command::CopyPath => self.copy(ctx, self.selected, false),
            Command::CopyName => self.copy(ctx, self.selected, true),
            Command::Close => {
                if self.settings.config.behavior.escape == Escape::ClearThenClose && !self.line.is_empty() {
                    self.line.clear();
                } else {
                    self.requests.push(Request::Close);
                }
            }
            Command::NextResult if rows > 0 => self.select((self.selected + 1).min(rows - 1)),
            Command::PreviousResult => self.select(self.selected.saturating_sub(1)),
            Command::NextPage if rows > 0 => self.select((self.selected + page).min(rows - 1)),
            Command::PreviousPage => self.select(self.selected.saturating_sub(page)),
            Command::Complete => self.accept_completion(ctx),
            Command::NextMode => self.tab = self.tab.next(),
            Command::PreviousMode => self.tab = self.tab.previous(),
            Command::NextResult | Command::NextPage => {}
        }
    }

    fn input_has_no_selection(ctx: &egui::Context) -> bool {
        egui::text_edit::TextEditState::load(ctx, egui::Id::unique(INPUT_ID))
            .and_then(|state| state.cursor.char_range())
            .is_none_or(|range| range.primary.index == range.secondary.index)
    }

    fn select(&mut self, index: usize) {
        self.selected = index;
        self.scroll_to_selected = true;
    }

    fn act(&mut self, index: usize, action: Action) {
        let Some(row) = self.outcome.rows.get(index) else {
            return;
        };
        // Opening an application starts it; the other actions are for its program.
        let request = match (&row.app, action) {
            (Some(app), Action::Open) => Request::Act(Action::Launch, PathBuf::from(app)),
            _ => Request::Act(action, row.path.clone()),
        };
        self.requests.push(request);
    }

    /// Copies the path, or only the name, of a row.
    fn copy(&mut self, ctx: &egui::Context, index: usize, name_only: bool) {
        let Some(row) = self.outcome.rows.get(index) else {
            return;
        };
        if name_only {
            ctx.copy_text(row.name.clone());
            self.notice = Some(self.strings.name_copied.to_owned());
        } else {
            ctx.copy_text(row.path.display().to_string());
            self.notice = Some(self.strings.path_copied.to_owned());
        }
        if self.settings.config.behavior.close_after_copy {
            self.requests.push(Request::Close);
        }
    }

    fn accept_completion(&mut self, ctx: &egui::Context) {
        let Some((start, _)) = typed_folder(&self.line, self.line.len()) else {
            return;
        };
        let Some(proposal) = self.completions.get(self.completion).cloned() else {
            return;
        };
        self.line.truncate(start);
        self.line.push_str(&proposal);
        // Caret at the end, so the next Tab goes one level deeper.
        let id = egui::Id::unique(INPUT_ID);
        let mut state = egui::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
        let end = egui::text::CCursor::new(self.line.chars().count());
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(end)));
        state.store(ctx, id);
    }

    // --- Drawing ---

    fn tabs(&mut self, ui: &mut Ui) {
        let effective = self.outcome.mode;
        let accent = color(self.settings.theme.colors.accent);
        ui.horizontal(|ui| {
            for mode in SearchMode::ALL {
                let text = RichText::new(self.strings.mode(mode));
                let text = if mode == effective {
                    text.color(accent).strong()
                } else {
                    text
                };
                if ui.selectable_label(mode == effective, text).clicked() {
                    self.tab = mode;
                    self.focus_input = true;
                }
            }
        });
        ui.add_space(2.0);
    }

    fn input_row(&mut self, ui: &mut Ui) {
        let theme = self.settings.theme.clone();
        let size = f32::from(self.settings.config.bar.font_size);
        let muted = color(theme.colors.text_secondary);
        ui.horizontal(|ui| {
            if theme.icons.set == IconSet::Phosphor {
                ui.label(
                    RichText::new(icons::SEARCH)
                        .font(FontId::new(size * 1.2, FontFamily::Name(ICONS.into())))
                        .color(color(theme.colors.accent)),
                );
            }
            let selector_width = if self.settings.config.layout.mode_selector == ModeSelector::Dropdown {
                96.0
            } else {
                0.0
            };
            let ghost = self.ghost().map(str::to_owned);
            let inline = self.settings.config.layout.chips == Chips::Inline;
            let colors = theme.colors.clone();
            let home = self.searcher.home().to_path_buf();
            let mut layouter = move |ui: &Ui, text: &dyn egui::TextBuffer, wrap_width: f32| {
                let mut job = if inline {
                    inline_job(text.as_str(), &home, &colors, size)
                } else {
                    LayoutJob::simple_singleline(
                        text.as_str().to_owned(),
                        FontId::proportional(size * 1.2),
                        color(colors.text_primary),
                    )
                };
                if let Some(ghost) = &ghost {
                    job.append(
                        ghost,
                        0.0,
                        TextFormat::simple(FontId::proportional(size * 1.2), color(colors.text_disabled)),
                    );
                }
                job.wrap.max_width = wrap_width;
                ui.fonts_mut(|fonts| fonts.layout_job(job))
            };
            let input = egui::TextEdit::singleline(&mut self.line)
                .id(egui::Id::unique(INPUT_ID))
                .hint_text(
                    RichText::new(self.strings.placeholder)
                        .color(muted)
                        .size(size * 1.2),
                )
                .frame(egui::Frame::NONE)
                .desired_width(ui.available_width() - selector_width)
                .layouter(&mut layouter)
                .show(ui);
            let response = input.response.response;
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::TextInput, true, self.strings.input_label)
            });
            if self.focus_input || !response.has_focus() {
                response.request_focus();
                self.focus_input = false;
            }
            if self.settings.config.layout.mode_selector == ModeSelector::Dropdown {
                self.mode_dropdown(ui);
            }
        });
    }

    fn mode_dropdown(&mut self, ui: &mut Ui) {
        let effective = self.outcome.mode;
        let muted = color(self.settings.theme.colors.text_secondary);
        // The arrow is drawn with the icon font: Inter has its own glyphs at these code points.
        let caret = RichText::new(icons::CARET_DOWN)
            .family(FontFamily::Name(ICONS.into()))
            .color(muted);
        let label = (RichText::new(self.strings.mode(effective)).color(muted), caret);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            egui::containers::menu::MenuButton::new(label)
                .ui(ui, |ui| {
                    for mode in SearchMode::ALL {
                        if ui
                            .selectable_label(mode == effective, self.strings.mode(mode))
                            .clicked()
                        {
                            self.tab = mode;
                            self.focus_input = true;
                            ui.close();
                        }
                    }
                })
                .0
                .widget_info(|| {
                    egui::WidgetInfo::labeled(egui::Role::ComboBox, true, self.strings.mode_label)
                });
        });
    }

    fn chips(&self, ui: &mut Ui) {
        let theme = &self.settings.theme;
        let colors = &theme.colors;
        let radius = u8::try_from(theme.shape.radius / 2).unwrap_or(u8::MAX);
        ui.horizontal_wrapped(|ui| {
            for part in &self.outcome.query.parts {
                let label = self.chip_label(part);
                let used = part.is_used();
                let (fill, text, stroke) = if !used {
                    (
                        Color32::TRANSPARENT,
                        color(colors.error),
                        Stroke::new(1.0, color(colors.error)),
                    )
                } else if part.excluded {
                    (
                        color(colors.surface_hover),
                        color(colors.text_primary),
                        Stroke::NONE,
                    )
                } else {
                    (color(colors.selection), color(colors.text_primary), Stroke::NONE)
                };
                let response = egui::Frame::new()
                    .fill(fill)
                    .stroke(stroke)
                    .corner_radius(CornerRadius::same(radius))
                    .inner_margin(Margin::symmetric(6, 1))
                    .show(ui, |ui| ui.label(RichText::new(label).small().color(text)))
                    .response;
                if let Some(error) = &part.error {
                    response.on_hover_text(self.strings.part_error(error));
                }
            }
        });
    }

    fn chip_label(&self, part: &serafind_core::Part) -> String {
        let strings = &self.strings;
        let base = match part.kind {
            PartKind::Name => part.value.clone(),
            PartKind::Extension => format!("{}: {}", strings.chip_extension, part.value),
            PartKind::Folder => format!("{}: {}", strings.chip_folder, part.value),
            PartKind::Mode(SearchMode::Content) => format!("{}: {}", strings.chip_content, part.value),
            PartKind::Mode(mode) if part.value.is_empty() => strings.mode(mode).to_owned(),
            PartKind::Mode(mode) => format!("{}: {}", strings.mode(mode), part.value),
        };
        if part.excluded {
            format!("{} {base}", strings.chip_not)
        } else {
            base
        }
    }

    fn results(&mut self, ui: &mut Ui) {
        let rows_layout = self.settings.config.layout.rows;
        let row_height = f32::from(self.settings.config.bar.row_height)
            * if rows_layout == Rows::Double { 1.4 } else { 1.0 };
        let visible = f32::from(self.settings.config.bar.visible_results);
        let single_click = self.settings.config.behavior.click == Click::Single;
        let mut clicked = None;
        let mut opened = None;
        egui::ScrollArea::vertical()
            .max_height(row_height * visible + 2.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for (index, row) in self.outcome.rows.iter().enumerate() {
                    let response = self.result_row(ui, index, row, row_height, rows_layout);
                    if response.double_clicked() || (single_click && response.clicked()) {
                        opened = Some(index);
                    } else if response.clicked() {
                        clicked = Some(index);
                    }
                    if index == self.selected && self.scroll_to_selected {
                        response.scroll_to_me(None);
                    }
                }
            });
        self.scroll_to_selected = false;
        if let Some(index) = clicked {
            self.selected = index;
        }
        if let Some(index) = opened {
            self.selected = index;
            self.act(index, Action::Open);
        }
    }

    /// The own icon of an application row, loaded once; `None` for other rows, when turned off
    /// (`[appearance] app_icons`), or when it cannot be read.
    fn app_icon(&self, ctx: &egui::Context, row: &Row) -> Option<egui::TextureHandle> {
        let path = row.icon.as_ref()?;
        if self.settings.config.appearance.app_icons != AppIcons::Real {
            return None;
        }
        let mut icons = self.app_icons.borrow_mut();
        icons
            .entry(path.clone())
            .or_insert_with(|| {
                let image = crate::app_icon::load(path)?;
                let options = egui::TextureOptions {
                    mipmap_mode: Some(egui::TextureFilter::Linear),
                    ..egui::TextureOptions::LINEAR
                };
                Some(ctx.load_texture(path.to_string_lossy(), image, options))
            })
            .clone()
    }

    fn result_row(&self, ui: &mut Ui, index: usize, row: &Row, height: f32, layout: Rows) -> egui::Response {
        let theme = &self.settings.theme;
        let colors = &theme.colors;
        let size = f32::from(self.settings.config.bar.font_size);
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), height), Sense::click());
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &row.name));
        let painter = ui.painter_at(rect);
        let radius = CornerRadius::same(u8::try_from(theme.shape.radius / 2).unwrap_or(u8::MAX));
        if index == self.selected {
            painter.rect_filled(rect, radius, color(colors.selection));
        } else if response.hovered() {
            painter.rect_filled(rect, radius, color(colors.surface_hover));
        }

        let mut x = rect.left() + 8.0;
        let middle = rect.center().y;
        let family = Family::of(row.kind, &row.name);
        let icon_size = size * 1.3;
        if theme.icons.set == IconSet::Phosphor
            && let Some(texture) = self.app_icon(ui.ctx(), row)
        {
            let place = egui::Rect::from_center_size(
                egui::pos2(x + icon_size / 2.0, middle),
                egui::vec2(icon_size, icon_size),
            );
            let whole = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            painter.image(texture.id(), place, whole, Color32::WHITE);
            x += icon_size + 8.0;
        } else if theme.icons.set == IconSet::Phosphor {
            painter.text(
                egui::pos2(x, middle),
                egui::Align2::LEFT_CENTER,
                family.glyph(),
                FontId::new(size * 1.3, FontFamily::Name(ICONS.into())),
                color(icons::color_of(family, theme)),
            );
            x += size * 1.3 + 8.0;
        }

        // Right side: Alt+n, then details or the folder.
        let mut right = rect.right() - 8.0;
        if index < 9 {
            let shortcut = painter.text(
                egui::pos2(right, middle),
                egui::Align2::RIGHT_CENTER,
                format!("Alt+{}", index + 1),
                FontId::proportional(size * 0.75),
                color(colors.text_secondary),
            );
            right = shortcut.left() - 10.0;
        }
        let name_job = name_job(&row.name, &row.highlights, theme, size);
        let name_galley = ui.fonts_mut(|fonts| fonts.layout_job(name_job));
        match layout {
            Rows::Double => {
                let name_pos = egui::pos2(x, middle - name_galley.size().y * 0.55 - 1.0);
                painter.galley(name_pos, name_galley, color(colors.text_primary));
                painter.text(
                    egui::pos2(x, middle + 2.0),
                    egui::Align2::LEFT_TOP,
                    &row.location,
                    FontId::proportional(size * 0.8),
                    color(colors.text_secondary),
                );
            }
            Rows::Single | Rows::Details => {
                let secondary = if layout == Rows::Details && row.kind != EntryKind::App {
                    let (bytes, date) = crate::format::details(&row.path);
                    let mut parts: Vec<String> = Vec::new();
                    parts.push(row.location.clone());
                    parts.extend(bytes);
                    parts.extend(date);
                    parts.join("   ")
                } else {
                    row.location.clone()
                };
                let name_width = name_galley.size().x;
                let space = (right - x - name_width - 16.0).max(0.0);
                let side_galley = ui.fonts_mut(|fonts| {
                    let mut job = LayoutJob::simple_singleline(
                        secondary,
                        FontId::proportional(size * 0.85),
                        color(colors.text_secondary),
                    );
                    job.wrap.max_width = space;
                    job.wrap.max_rows = 1;
                    job.wrap.break_anywhere = true;
                    fonts.layout_job(job)
                });
                painter.galley(
                    egui::pos2(x, middle - name_galley.size().y / 2.0),
                    name_galley,
                    color(colors.text_primary),
                );
                let side_pos = egui::pos2(right - side_galley.size().x, middle - side_galley.size().y / 2.0);
                painter.galley(side_pos, side_galley, color(colors.text_secondary));
            }
        }
        response
    }

    fn footer(&self, ui: &mut Ui) {
        let colors = &self.settings.theme.colors;
        let muted = color(colors.text_secondary);
        ui.add_space(4.0);
        ui.separator();
        for problem in &self.problems {
            ui.horizontal(|ui| {
                let error = color(colors.error);
                ui.label(
                    RichText::new(icons::WARNING)
                        .family(FontFamily::Name(ICONS.into()))
                        .color(error),
                );
                ui.label(RichText::new(problem).small().color(error));
            });
        }
        if self.line.trim().is_empty() {
            return;
        }
        let strings = &self.strings;
        let outcome = &self.outcome;
        let status = if let Some(notice) = &self.notice {
            notice.clone()
        } else if outcome.unreachable {
            strings.unreachable.to_owned()
        } else if outcome.indexing {
            strings.indexing.to_owned()
        } else if outcome.outside_index {
            strings.outside_index.to_owned()
        } else if outcome.total == 0 {
            strings.no_result.to_owned()
        } else if outcome.is_fuzzy {
            format!(
                "{} · {}",
                strings.count(outcome.rows.len(), outcome.total),
                strings.fuzzy_note
            )
        } else {
            strings.count(outcome.rows.len(), outcome.total)
        };
        ui.horizontal(|ui| {
            ui.label(RichText::new(status).small().color(muted));
            if !outcome.rows.is_empty() {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new(strings.key_hints).small().color(muted));
                });
            }
        });
    }
}

/// The name with its matched letters highlighted (semi-bold, highlight color).
fn name_job(name: &str, highlights: &[usize], theme: &serafind_settings::Theme, size: f32) -> LayoutJob {
    let normal = TextFormat::simple(FontId::proportional(size), color(theme.colors.text_primary));
    let strong = TextFormat::simple(
        FontId::new(size, FontFamily::Name(BOLD.into())),
        color(theme.colors.accent),
    );
    let mut job = LayoutJob::default();
    let mut highlighted = highlights.iter().peekable();
    for (index, character) in name.chars().enumerate() {
        let is_highlighted = highlighted.next_if_eq(&&index).is_some();
        let mut buffer = [0; 4];
        job.append(
            character.encode_utf8(&mut buffer),
            0.0,
            if is_highlighted {
                strong.clone()
            } else {
                normal.clone()
            },
        );
    }
    job.wrap.max_rows = 1;
    job
}

/// The input text with its parts colored (`chips = "inline"`): filters in the accent color,
/// parts that cannot be used in the error color.
fn inline_job(
    text: &str,
    home: &std::path::Path,
    colors: &serafind_settings::Colors,
    size: f32,
) -> LayoutJob {
    let exists = |path: &std::path::Path| path.is_dir();
    let context = serafind_core::ParseContext {
        home,
        current_folder: home,
        folder_exists: &exists,
    };
    let query = serafind_core::parse(text, &context);
    let font = FontId::proportional(size * 1.2);
    let mut job = LayoutJob::default();
    let mut at = 0;
    for part in &query.parts {
        if part.span.start > at {
            job.append(
                &text[at..part.span.start],
                0.0,
                TextFormat::simple(font.clone(), color(colors.text_primary)),
            );
        }
        let mut format = TextFormat::simple(font.clone(), color(colors.text_primary));
        if !part.is_used() {
            format.color = color(colors.error);
            format.underline = Stroke::new(1.0, color(colors.error));
        } else if part.kind != PartKind::Name {
            format.color = color(colors.accent);
        }
        job.append(&text[part.span.clone()], 0.0, format);
        at = part.span.end;
    }
    if at < text.len() {
        job.append(
            &text[at..],
            0.0,
            TextFormat::simple(font, color(colors.text_primary)),
        );
    }
    job
}

/// Installs the fonts and visuals of the bar's theme; returns a warning when its font is missing.
pub fn apply_theme(ctx: &egui::Context, settings: &Settings) -> Option<String> {
    let (fonts, warning) = style::fonts(&settings.theme);
    ctx.set_fonts(fonts);
    style::apply(ctx, settings);
    warning
}

#[cfg(test)]
mod tests;
