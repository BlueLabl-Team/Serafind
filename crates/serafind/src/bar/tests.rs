//! The bar in a test harness: keys and text go through egui as in the real window.

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use eframe::egui::{self, Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serafind_index::{Index, Kind};
use serafind_settings::{Escape, SystemMode};

use super::*;
use crate::search::SharedIndex;

const HOME: &str = "/home/me";

const FILES: [&str; 6] = [
    "Documents/facture-2026.pdf",
    "Documents/Factures/",
    "Downloads/facture-edf.pdf",
    "Projects/serafind/README.md",
    "Music/",
    "notes.txt",
];

fn index_of(paths: &[&str]) -> SharedIndex {
    let mut index = Index::new();
    index.add_root(Path::new(HOME));
    for path in paths {
        let kind = if path.ends_with('/') {
            Kind::Folder
        } else {
            Kind::File
        };
        index.insert_path(&Path::new(HOME).join(path.trim_end_matches('/')), kind);
    }
    Arc::new(RwLock::new(Some(Arc::new(RwLock::new(index)))))
}

fn bar_with(settings: Settings, index: SharedIndex, home: &Path) -> Bar {
    Bar::new(settings, Searcher::new(index, home.to_path_buf()))
}

fn harness(bar: Bar) -> Harness<'static, Bar> {
    let settings = bar.settings().clone();
    Harness::builder()
        .with_size(egui::vec2(700.0, 520.0))
        .with_context_setup(move |ctx| {
            let _ = apply_theme(ctx, &settings);
        })
        .build_ui_state(|ui, bar: &mut Bar| bar.ui(ui), bar)
}

fn default_harness() -> Harness<'static, Bar> {
    harness(bar_with(
        Settings::defaults(SystemMode::Dark),
        index_of(&FILES),
        Path::new(HOME),
    ))
}

fn type_text(harness: &mut Harness<'_, Bar>, text: &str) {
    harness.get_by_role(egui::Role::TextInput).type_text(text);
    harness.run();
}

fn press(harness: &mut Harness<'_, Bar>, modifiers: Modifiers, key: Key) {
    harness.key_press_modifiers(modifiers, key);
    harness.run();
}

fn row_names(harness: &Harness<'_, Bar>) -> Vec<String> {
    harness
        .state()
        .outcome()
        .rows
        .iter()
        .map(|row| row.name.clone())
        .collect()
}

#[test]
fn typing_shows_results_and_count() {
    let mut harness = default_harness();
    harness.run();
    assert!(harness.query_by_label("facture-2026.pdf").is_none());
    type_text(&mut harness, "fact");
    assert_eq!(harness.state().line(), "fact");
    let names = row_names(&harness);
    assert_eq!(names.len(), 3, "{names:?}");
    harness.get_by_label("facture-2026.pdf");
    harness.get_by_label_contains("3 results");
}

#[test]
fn empty_line_shows_the_filters_hint() {
    let mut harness = default_harness();
    harness.run();
    harness.get_by_label_contains("a: apps");
}

#[test]
fn arrows_move_the_selection_and_enter_opens() {
    let mut harness = default_harness();
    type_text(&mut harness, "fact");
    press(&mut harness, Modifiers::NONE, Key::ArrowDown);
    press(&mut harness, Modifiers::NONE, Key::ArrowDown);
    press(&mut harness, Modifiers::NONE, Key::ArrowDown);
    assert_eq!(harness.state().selected(), 2, "stops at the last row");
    press(&mut harness, Modifiers::NONE, Key::ArrowUp);
    assert_eq!(harness.state().selected(), 1);
    press(&mut harness, Modifiers::NONE, Key::Enter);
    let expected = harness.state().outcome().rows[1].path.clone();
    assert_eq!(harness.state_mut().take_requests(), [Request::Open(expected)]);
}

#[test]
fn ctrl_enter_reveals_and_alt_number_opens() {
    let mut harness = default_harness();
    type_text(&mut harness, "fact");
    press(&mut harness, Modifiers::COMMAND, Key::Enter);
    press(&mut harness, Modifiers::ALT, Key::Num2);
    let rows = harness.state().outcome().rows.clone();
    assert_eq!(
        harness.state_mut().take_requests(),
        [
            Request::Reveal(rows[0].path.clone()),
            Request::Open(rows[1].path.clone())
        ]
    );
}

#[test]
fn clicking_a_row_opens_it() {
    let mut harness = default_harness();
    type_text(&mut harness, "readme");
    harness.get_by_label("README.md").click();
    harness.run();
    assert_eq!(
        harness.state_mut().take_requests(),
        [Request::Open(PathBuf::from(
            "/home/me/Projects/serafind/README.md"
        ))]
    );
}

#[test]
fn escape_closes() {
    let mut harness = default_harness();
    type_text(&mut harness, "fact");
    press(&mut harness, Modifiers::NONE, Key::Escape);
    assert_eq!(harness.state_mut().take_requests(), [Request::Close]);
}

#[test]
fn escape_can_clear_first() {
    let mut settings = Settings::defaults(SystemMode::Dark);
    settings.config.behavior.escape = Escape::ClearThenClose;
    let mut harness = harness(bar_with(settings, index_of(&FILES), Path::new(HOME)));
    type_text(&mut harness, "fact");
    press(&mut harness, Modifiers::NONE, Key::Escape);
    assert_eq!(harness.state().line(), "");
    assert!(harness.state_mut().take_requests().is_empty());
    press(&mut harness, Modifiers::NONE, Key::Escape);
    assert_eq!(harness.state_mut().take_requests(), [Request::Close]);
}

#[test]
fn tab_and_ctrl_numbers_choose_the_mode() {
    let mut harness = default_harness();
    type_text(&mut harness, "fact");
    press(&mut harness, Modifiers::NONE, Key::Tab);
    assert_eq!(harness.state().tab(), SearchMode::Apps);
    press(&mut harness, Modifiers::SHIFT, Key::Tab);
    assert_eq!(harness.state().tab(), SearchMode::All);
    press(&mut harness, Modifiers::COMMAND, Key::Num3);
    assert_eq!(harness.state().tab(), SearchMode::Folders);
    assert_eq!(row_names(&harness), ["Factures"]);
    press(&mut harness, Modifiers::COMMAND, Key::Num4);
    assert_eq!(row_names(&harness).len(), 2);
}

#[test]
fn a_mode_filter_wins_over_the_tab() {
    let mut harness = default_harness();
    press(&mut harness, Modifiers::COMMAND, Key::Num4);
    type_text(&mut harness, "d: fact");
    assert_eq!(harness.state().tab(), SearchMode::Files);
    assert_eq!(harness.state().outcome().mode, SearchMode::Folders);
    assert_eq!(row_names(&harness), ["Factures"]);
    // The selector shows the mode in use.
    harness.get_by_label_contains("Folders");
}

#[test]
fn chips_show_what_was_understood() {
    let mut harness = default_harness();
    type_text(&mut harness, "fact ext:pdf -edf in:~/Nowhere");
    harness.get_by_label("Ext: pdf");
    harness.get_by_label("Not edf");
    harness.get_by_label("Not edf");
    assert!(
        !harness.state().outcome().query.parts[3].is_used(),
        "the missing folder is reported"
    );
    assert_eq!(row_names(&harness), ["facture-2026.pdf"]);
}

#[test]
fn indexing_is_shown_until_the_index_is_ready() {
    let index: SharedIndex = Arc::new(RwLock::new(None));
    let mut harness = harness(bar_with(
        Settings::defaults(SystemMode::Dark),
        Arc::clone(&index),
        Path::new(HOME),
    ));
    type_text(&mut harness, "fact");
    harness.get_by_label_contains("Indexing");
    *index.write().unwrap() = index_of(&FILES).read().unwrap().clone();
    harness.run();
    assert_eq!(row_names(&harness).len(), 3);
}

#[test]
fn problems_are_shown_in_the_footer() {
    let mut harness = default_harness();
    harness
        .state_mut()
        .set_problems(vec!["config.toml:2:1: unknown field `widht`".to_owned()]);
    harness.run();
    harness.get_by_label_contains("unknown field `widht`");
}

#[test]
fn tab_completes_the_in_folder() {
    let home = assert_fs::TempDir::new().unwrap();
    std::fs::create_dir_all(home.path().join("Documents/Taxes")).unwrap();
    std::fs::create_dir_all(home.path().join("Downloads")).unwrap();
    let mut harness = harness(bar_with(
        Settings::defaults(SystemMode::Dark),
        index_of(&[]),
        home.path(),
    ));
    type_text(&mut harness, "in:~/Doc");
    press(&mut harness, Modifiers::NONE, Key::Tab);
    assert_eq!(harness.state().line(), "in:~/Documents/");
    assert_eq!(
        harness.state().tab(),
        SearchMode::All,
        "Tab completed instead of changing the mode"
    );
    press(&mut harness, Modifiers::NONE, Key::Tab);
    assert_eq!(harness.state().line(), "in:~/Documents/Taxes/");
}

#[test]
fn every_layout_draws() {
    for (chips, rows, selector) in [
        (Chips::Below, Rows::Single, ModeSelector::Dropdown),
        (Chips::Inline, Rows::Double, ModeSelector::Tabs),
        (Chips::Hidden, Rows::Details, ModeSelector::Dropdown),
    ] {
        let mut settings = Settings::defaults(SystemMode::Light);
        settings.config.layout.chips = chips;
        settings.config.layout.rows = rows;
        settings.config.layout.mode_selector = selector;
        let mut harness = harness(bar_with(settings, index_of(&FILES), Path::new(HOME)));
        type_text(&mut harness, "fact ext:pdf");
        assert_eq!(row_names(&harness).len(), 2, "{chips:?} {rows:?} {selector:?}");
        harness.get_by_label("facture-2026.pdf");
    }
}

/// Renders the bar to PNG files to look at it: `SERAFIND_PREVIEW=/some/folder cargo test -p
/// serafind preview -- --ignored`.
#[test]
#[ignore = "writes images for a visual check"]
fn preview() {
    let Some(folder) = std::env::var_os("SERAFIND_PREVIEW") else {
        return;
    };
    let folder = PathBuf::from(folder);
    for (name, mode, line) in [
        ("dark-empty", SystemMode::Dark, ""),
        ("dark-results", SystemMode::Dark, "fact ext:pdf -edf"),
        ("light-results", SystemMode::Light, "fact"),
        ("dark-fuzzy", SystemMode::Dark, "fctr"),
    ] {
        let mut harness = harness(bar_with(
            Settings::defaults(mode),
            index_of(&FILES),
            Path::new(HOME),
        ));
        harness.run();
        if !line.is_empty() {
            type_text(&mut harness, line);
        }
        press(&mut harness, Modifiers::NONE, Key::ArrowDown);
        let image = harness.render().expect("render");
        image.save(folder.join(format!("{name}.png"))).expect("save");
    }
}
