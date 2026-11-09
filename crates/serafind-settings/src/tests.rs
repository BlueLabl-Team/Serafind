use assert_fs::TempDir;
use assert_fs::prelude::*;
use rstest::rstest;

use super::*;

fn folder_with(config: &str) -> TempDir {
    let dir = TempDir::new().unwrap();
    dir.child(CONFIG_FILE).write_str(config).unwrap();
    dir
}

#[test]
fn the_commented_file_gives_the_defaults() {
    let parsed: Config = toml::from_str(DEFAULT_CONFIG).unwrap();
    assert_eq!(parsed, Config::default());
    assert_eq!(Config::default().problems(), Vec::<(&str, String)>::new());
}

#[test]
fn no_files_means_defaults() {
    let dir = TempDir::new().unwrap();
    assert_eq!(
        load(dir.path(), SystemMode::Dark).unwrap(),
        Settings::defaults(SystemMode::Dark)
    );
}

#[test]
fn written_defaults_load_as_defaults() {
    let dir = TempDir::new().unwrap();
    let written = write_defaults(dir.path()).unwrap();
    assert_eq!(written.len(), 3);
    for mode in [SystemMode::Dark, SystemMode::Light] {
        assert_eq!(load(dir.path(), mode).unwrap(), Settings::defaults(mode));
    }
}

#[test]
fn existing_files_are_never_replaced() {
    let dir = folder_with("[bar]\nvisible_results = 3\n");
    let written = write_defaults(dir.path()).unwrap();
    assert_eq!(written.len(), 2, "only the two themes");
    assert_eq!(
        load(dir.path(), SystemMode::Dark)
            .unwrap()
            .config
            .bar
            .visible_results,
        3
    );
}

#[test]
fn a_few_keys_are_enough() {
    let dir = folder_with("[behavior]\nescape = \"clear-then-close\"\n[layout]\nrows = \"double\"\n");
    let config = load(dir.path(), SystemMode::Dark).unwrap().config;
    assert_eq!(config.behavior.escape, Escape::ClearThenClose);
    assert_eq!(config.layout.rows, Rows::Double);
    assert_eq!(config.bar, BarSettings::default());
}

#[rstest]
#[case(SystemMode::Dark, "Default Dark")]
#[case(SystemMode::Light, "Default Light")]
fn system_theme_follows_the_desktop(#[case] mode: SystemMode, #[case] name: &str) {
    let dir = TempDir::new().unwrap();
    assert_eq!(load(dir.path(), mode).unwrap().theme.name, name);
}

#[test]
fn a_named_theme_ignores_the_desktop() {
    let dir = folder_with("[appearance]\ntheme = \"light\"\n");
    assert_eq!(
        load(dir.path(), SystemMode::Dark).unwrap().theme.name,
        "Default Light"
    );
}

#[test]
fn appearance_overrides_the_theme() {
    let dir = folder_with("[appearance]\nfont_family = \"Cantarell\"\nicons = \"none\"\n");
    let theme = load(dir.path(), SystemMode::Dark).unwrap().theme;
    assert_eq!(theme.font.family, "Cantarell");
    assert_eq!(theme.icons.set, IconSet::None);
}

#[rstest]
#[case("[bar]\nwidht = 700\n", 2, "widht")]
#[case("[barr]\nwidth = 700\n", 1, "barr")]
#[case("[layout]\nrows = \"triple\"\n", 2, "triple")]
#[case("[bar]\nvisible_results = \"eight\"\n", 2, "eight")]
#[case("[bar\nvisible_results = 8\n", 1, "")]
#[case("\n\n[bar]\nvisible_results = 0\n", 4, "bar.visible_results")]
#[case("[bar]\nvisible_results = 20\nmax_results = 10\n", 3, "bar.max_results")]
#[case("[bar]\nfont_size = 100\n", 2, "bar.font_size")]
#[case("[window]\ntop = 2.0\n", 2, "window.top")]
#[case("[shortcut]\ntoggle = \"Super+Banana\"\n", 2, "unknown key")]
#[case("[shortcut]\ntoggle = \"F\"\n", 2, "normal typing")]
#[case("[keys]\nopen = \"Enter\"\ncopy_name = \"N\"\n", 3, "type in the search")]
#[case("[keys]\nmode_with_digit = \"Super\"\n", 2, "cannot use Super")]
#[case("[behavior]\nclick = \"triple\"\n", 2, "triple")]
#[case("[actions]\nterminal = \" \"\n", 2, "actions.terminal")]
fn problems_are_errors_with_their_line(#[case] config: &str, #[case] line: usize, #[case] mentions: &str) {
    let dir = folder_with(config);
    let error = load(dir.path(), SystemMode::Dark).unwrap_err();
    assert_eq!(error.line, Some(line), "{error}");
    assert!(error.message.contains(mentions), "{error}");
    assert!(error.file.ends_with(CONFIG_FILE));
}

#[test]
fn a_missing_theme_is_an_error() {
    let dir = folder_with("[appearance]\ntheme = \"solarized\"\n");
    let error = load(dir.path(), SystemMode::Dark).unwrap_err();
    assert!(error.message.contains("solarized"), "{error}");
}

#[test]
fn shortcut_per_system() {
    let shortcut = Shortcut {
        enabled: true,
        toggle: "Super+F".to_owned(),
        windows: Some("Ctrl+Alt+Space".to_owned()),
        linux: None,
    };
    let expected = if cfg!(windows) {
        "Ctrl+Alt+Space"
    } else {
        "Super+F"
    };
    assert_eq!(shortcut.for_this_system(), expected);
}

#[test]
fn settings_folder_name() {
    if let Some(dir) = default_dir() {
        let name = dir.file_name().unwrap().to_string_lossy().to_lowercase();
        assert_eq!(name, "serafind");
    }
}
