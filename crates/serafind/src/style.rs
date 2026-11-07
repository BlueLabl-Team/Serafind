//! Turns a Serafind theme into egui fonts and visuals.

use std::path::Path;
use std::sync::Arc;

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle,
};
use serafind_settings::{Color, Settings, Theme, ThemeMode};

/// Family name of the semi-bold text font (names, matched letters).
pub const BOLD: &str = "serafind-bold";
/// Family name of the icon font.
pub const ICONS: &str = "serafind-icons";

const INTER_REGULAR: &[u8] = include_bytes!("../assets/fonts/Inter-Regular.ttf");
const INTER_SEMI_BOLD: &[u8] = include_bytes!("../assets/fonts/Inter-SemiBold.ttf");
const PHOSPHOR: &[u8] = include_bytes!("../assets/fonts/Phosphor.ttf");

/// An egui color from a theme color.
#[must_use]
pub fn color(color: Color) -> Color32 {
    let [red, green, blue, alpha] = color.rgba;
    Color32::from_rgba_unmultiplied(red, green, blue, alpha)
}

/// Fonts for a theme: its text font (Inter, an installed font or a font file) with egui's fonts
/// as fallback for missing characters, a semi-bold family, and the icon font. Returns a warning
/// when the theme's font cannot be found (Inter is used instead).
#[must_use]
pub fn fonts(theme: &Theme) -> (FontDefinitions, Option<String>) {
    let mut fonts = FontDefinitions::default();
    let (regular, bold, warning): (Arc<FontData>, Arc<FontData>, Option<String>) =
        match find_font(&theme.font.family) {
            FontSource::Inter => (static_font(INTER_REGULAR), static_font(INTER_SEMI_BOLD), None),
            FontSource::Bytes(bytes) => {
                let font = Arc::new(FontData::from_owned(bytes));
                (Arc::clone(&font), font, None)
            }
            FontSource::NotFound => (
                static_font(INTER_REGULAR),
                static_font(INTER_SEMI_BOLD),
                Some(format!("font {:?} not found, using Inter", theme.font.family)),
            ),
        };
    fonts.font_data.insert("serafind-regular".to_owned(), regular);
    fonts.font_data.insert(BOLD.to_owned(), bold);
    fonts.font_data.insert(ICONS.to_owned(), static_font(PHOSPHOR));

    let fallbacks = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    let family = |first: &str| {
        let mut list = vec![first.to_owned(), ICONS.to_owned()];
        list.extend(fallbacks.iter().cloned());
        list
    };
    fonts
        .families
        .insert(FontFamily::Proportional, family("serafind-regular"));
    fonts.families.insert(FontFamily::Name(BOLD.into()), family(BOLD));
    // Icons first; text fonts behind it for anything else.
    let mut icons = vec![ICONS.to_owned(), "serafind-regular".to_owned()];
    icons.extend(fallbacks);
    fonts.families.insert(FontFamily::Name(ICONS.into()), icons);
    (fonts, warning)
}

fn static_font(bytes: &'static [u8]) -> Arc<FontData> {
    Arc::new(FontData::from_static(bytes))
}

enum FontSource {
    Inter,
    Bytes(Vec<u8>),
    NotFound,
}

/// `Inter`, a `.ttf` / `.otf` path, or the name of an installed font.
fn find_font(family: &str) -> FontSource {
    if family.eq_ignore_ascii_case("inter") {
        return FontSource::Inter;
    }
    let path = Path::new(family);
    let is_file = path.extension().is_some_and(|extension| {
        extension.eq_ignore_ascii_case("ttf") || extension.eq_ignore_ascii_case("otf")
    });
    let file = if is_file {
        Some(path.to_path_buf())
    } else {
        installed_font_file(family)
    };
    file.and_then(|file| std::fs::read(file).ok())
        .map_or(FontSource::NotFound, FontSource::Bytes)
}

/// Asks fontconfig for the file of an installed font.
#[cfg(target_os = "linux")]
fn installed_font_file(family: &str) -> Option<std::path::PathBuf> {
    let output = std::process::Command::new("fc-match")
        .args(["--format=%{file}\n%{family}", family])
        .output()
        .ok()?;
    let text = String::from_utf8(output.stdout).ok()?;
    let (file, found) = text.split_once('\n')?;
    // fc-match always answers something: accept it only when it is the asked family.
    let asked = family.to_lowercase();
    found
        .split(',')
        .any(|name| name.trim().to_lowercase() == asked)
        .then(|| file.into())
}

/// Looks in the system and user font folders for a file named after the font.
#[cfg(windows)]
fn installed_font_file(family: &str) -> Option<std::path::PathBuf> {
    let wanted: String = family
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_lowercase();
    let folders = [
        std::env::var_os("WINDIR").map(|dir| Path::new(&dir).join("Fonts")),
        std::env::var_os("LOCALAPPDATA").map(|dir| Path::new(&dir).join("Microsoft\\Windows\\Fonts")),
    ];
    folders.into_iter().flatten().find_map(|folder| {
        std::fs::read_dir(folder)
            .ok()?
            .flatten()
            .map(|entry| entry.path())
            .find(|path| {
                let stem = path
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                let is_font = path.extension().is_some_and(|extension| {
                    extension.eq_ignore_ascii_case("ttf") || extension.eq_ignore_ascii_case("otf")
                });
                is_font
                    && (stem == wanted
                        || stem == format!("{wanted}-regular")
                        || stem == format!("{wanted}regular"))
            })
    })
}

#[cfg(not(any(target_os = "linux", windows)))]
fn installed_font_file(_family: &str) -> Option<std::path::PathBuf> {
    None
}

/// Applies the theme's colors and shapes, and the bar's text size, to egui.
pub fn apply(ctx: &egui::Context, settings: &Settings) {
    let theme = &settings.theme;
    let colors = &theme.colors;
    let dark = theme.mode == ThemeMode::Dark;
    let size = f32::from(settings.config.bar.font_size);
    let radius = u8::try_from(theme.shape.radius).unwrap_or(u8::MAX);

    let mut visuals = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    visuals.override_text_color = Some(color(colors.text_primary));
    visuals.panel_fill = color(colors.surface_elevated);
    visuals.window_fill = color(colors.surface_elevated);
    visuals.extreme_bg_color = color(colors.surface);
    visuals.faint_bg_color = color(colors.surface);
    visuals.hyperlink_color = color(colors.accent);
    visuals.selection.bg_fill = color(colors.selection);
    visuals.selection.stroke = Stroke::new(1.0, color(colors.text_primary));
    visuals.text_cursor.stroke = Stroke::new(2.0, color(colors.focus));
    visuals.window_corner_radius = CornerRadius::same(radius);
    visuals.menu_corner_radius = CornerRadius::same(radius);
    visuals.window_stroke = Stroke::new(1.0, color(colors.border));
    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.corner_radius = CornerRadius::same(radius / 2);
        widget.fg_stroke.color = color(colors.text_primary);
    }
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, color(colors.border));
    visuals.widgets.inactive.weak_bg_fill = color(colors.surface);
    visuals.widgets.hovered.weak_bg_fill = color(colors.surface_hover);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, color(colors.border));

    ctx.set_visuals(visuals);
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Heading, FontId::proportional(size * 1.3)),
            (TextStyle::Body, FontId::proportional(size)),
            (TextStyle::Button, FontId::proportional(size)),
            (TextStyle::Monospace, FontId::monospace(size * 0.9)),
            // PowerHive: body 14, small 12.
            (TextStyle::Small, FontId::proportional(size * 12.0 / 14.0)),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(6.0, 4.0);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inter_is_built_in_and_unknown_fonts_fall_back() {
        let mut theme = Theme::built_in("dark").unwrap();
        let (definitions, warning) = fonts(&theme);
        assert!(warning.is_none());
        assert!(definitions.font_data.contains_key(ICONS));
        theme.font.family = "Definitely Not A Font 1234".to_owned();
        let (_, warning) = fonts(&theme);
        assert!(warning.is_some());
    }
}
