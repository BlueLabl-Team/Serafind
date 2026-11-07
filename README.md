# Serafind

*A BlueLabl project*

Serafind is a quick search bar for Linux and Windows. Press a global shortcut anywhere, type, and open what you were looking for: an application, a folder, a file, or text inside a file. It brings the quick search of [PowerHive](https://github.com/BlueLabl-Team/PowerHive) to the whole desktop, as a single small executable.

> **Status:** early development. Nothing is usable yet. See the [roadmap](#roadmap).

## Goals

- **Fast**: results while typing; under 100 ms from shortcut to bar, under 10 ms per query on about one million files.
- **Light**: a single executable of a few megabytes, low memory use when idle, no runtime to install.
- **Keyboard first**: every mode and action is reachable by a shortcut or a keyword.

## Features (planned)

### Modes

| Mode | Searches | Switch |
|---|---|---|
| **All** (default) | Apps first, then folders and files | Tab / Shift+Tab, Ctrl+1 |
| **Apps** | Installed applications (`.desktop` files, Start menu) | Ctrl+2 or keyword |
| **Folders** | Folders | Ctrl+3 or keyword |
| **Files** | Files | Ctrl+4 or keyword |
| **Content** | Text inside files | Ctrl+5 or keyword |

Mode keywords will be defined with the search syntax.

### Search syntax

The same syntax as PowerHive's quick search, where each part of the line is shown as a chip:

```
invoice ext:pdf,docx in:~/Documents type:file -draft "exact phrase" rep*rt
```

- words (or `"quoted phrases"`) the name must contain; `*` and `?` make a pattern found anywhere in the name (`rep*rt` finds `my-report.pdf`), `^` and `$` anchor it at the start or the end (`^inv`, `.pdf$`); case is ignored unless the word has an upper-case letter, accents are always ignored
- `ext:` extensions, `in:` starting folder (Tab completes it), `type:` `file` or `folder`
- `-` excludes a word or a filter

### Actions

| Key | Action |
|---|---|
| Enter | Open |
| Ctrl+Enter | Show in the file manager |
| Ctrl+C | Copy the path |

## How it works

- A small resident process keeps an **in-memory index** of the home directory, minus excluded folders (`.git`, `node_modules`, `target`, caches, hidden folders). The index is built by a parallel walk at start and kept up to date by watching the filesystem. The indexed folders and exclusions can be configured.
- **Content** search runs on demand, like ripgrep: the index narrows down the candidate files, then they are read in parallel.
- The **global shortcut** is registered directly on Windows and X11, and through the XDG `GlobalShortcuts` portal on Wayland (GNOME, KDE). On other compositors, bind `serafind toggle` to a shortcut in the compositor settings.

## Settings and themes

Everything is set in one folder: `~/.config/serafind/` on Linux, `%APPDATA%\Serafind\` on Windows. It is created on first launch, with every option commented:

```
serafind/
├── config.toml        behavior, layout, results, indexed folders, shortcut, theme choice
└── themes/
    ├── dark.toml      built-in themes (PowerHive's Default Dark and Default Light), written as examples
    ├── light.toml
    └── my-theme.toml  your own: extends = "dark", then only what changes
```

- Changes apply as soon as a file is saved.
- Every key is optional: a missing key keeps its default.
- An unknown key, a wrong value or a syntax error makes the file be refused. The previous settings stay, and the error is shown in the bar with its file and line.
- Themes use the same color roles as PowerHive (`primary`, `accent`, `surface_elevated`, `text_secondary`…), so both applications look alike.
- `theme = "system"` follows the dark or light mode of the desktop. Font and icons come from the theme, and `[appearance]` in `config.toml` can override them.
- The global shortcut is `Super+F` by default (`[shortcut] toggle`), with an optional per-system value.

## Built with

| | |
|---|---|
| [Rust](https://www.rust-lang.org/) (edition 2024, latest stable) | Language |
| [egui](https://github.com/emilk/egui) | User interface |
| [ignore](https://github.com/BurntSushi/ripgrep/tree/master/crates/ignore) | Parallel tree walking |
| [notify](https://github.com/notify-rs/notify) | Filesystem watching (index, settings) |
| [nucleo-matcher](https://github.com/helix-editor/nucleo) | Fuzzy matching, accent and case folding |
| [serde](https://serde.rs/) and [toml](https://github.com/toml-rs/toml) | Settings and theme files |

### Tests

| | |
|---|---|
| [cargo-nextest](https://nexte.st/) | Test runner |
| [rstest](https://github.com/la10736/rstest) | Fixtures and table-driven tests |
| [proptest](https://github.com/proptest-rs/proptest) | Property-based tests |
| [insta](https://insta.rs/) | Snapshot tests |
| [assert_fs](https://github.com/assert-rs/assert_fs) | Temporary file trees |

Later: [egui_kittest](https://github.com/emilk/egui/tree/main/crates/egui_kittest) for UI tests, [divan](https://github.com/nvzqz/divan) for benchmarks, [assert_cmd](https://github.com/assert-rs/assert_cmd) for command-line tests, [cargo-mutants](https://mutants.rs/) and [cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov) run by hand.

## Project layout

| Crate | Role |
|---|---|
| [`crates/serafind-core`](crates/serafind-core) | Search line syntax, modes, matching and ranking. Pure: no disk, no window. |
| [`crates/serafind-index`](crates/serafind-index) | In-memory index: tree walking, filesystem watching, exclusions. |
| [`crates/serafind-settings`](crates/serafind-settings) | `config.toml` and themes: defaults, validation, live reload. No user interface. |
| [`crates/serafind`](crates/serafind) | The `serafind` executable: search bar, resident process, global shortcut, command line. |

## Build from source

You need Rust (latest stable) and Git.

```bash
git clone git@github.com:BlueLabl-Team/Serafind.git
cd Serafind
cargo build --release          # target/release/serafind
cargo run --release -p serafind  # opens the bar (the global shortcut comes later)
cargo test --workspace         # or: cargo nextest run --workspace
cargo insta review             # review changed snapshots
```

## Roadmap

| # | Step | Status |
|---|---|---|
| 0 | Workspace, test tooling, roadmap | ✅ |
| 1 | `core`: search syntax (PowerHive grammar + mode keywords), matching, ranking (fuzzy / prefix) | ✅ |
| 2 | `index`: parallel first walk, filesystem watching, exclusions, benchmarks | ✅ |
| 3 | Settings and themes (TOML, defaults, validation, live reload) | ✅ |
| 3 | Search bar (egui): input, chips, results, keyboard navigation, `in:` completion | ✅ |
| 4 | Resident process: single instance, `serafind toggle`, global shortcut (X11, Windows, Wayland portal), tray icon | ⬜ |
| 4b | Choose where the bar appears on Wayland (layer-shell, focus on X11, or another way: to be decided) | ⬜ |
| 5 | Actions: open, show in the file manager, copy the path | ⬜ |
| 6 | Apps mode: `.desktop` files (Linux), Start menu shortcuts (Windows) | ⬜ |
| 7 | Content mode: on-demand parallel search | ⬜ |
| 8 | Start with the session; settings for indexed and excluded folders applied live | ⬜ |
| 9 | GitHub Actions: build, tests, releases (Linux and Windows executables) | ⬜ |
| 10 | Integration in third-party software through their plugin / extension systems (library, IPC protocol or C ABI: to be decided) | ⬜ |

## License

[Apache License 2.0](LICENSE)
