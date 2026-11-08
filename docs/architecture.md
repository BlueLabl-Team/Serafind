# Serafind: how it works

## Processes and index

- **Two processes.** A small resident process (a few MB, no window) keeps the index, listens for the shortcut and answers on a local socket. The bar is a separate process, started on demand, that searches through that socket. Between openings the bar is hidden and kept (instant), or closed to free its memory (`[behavior] keep_window`). On Wayland, a window cannot be hidden, so the bar is always closed and started again (about 50 ms).
- **Local protocol.** JSON messages, one per line, on a Unix socket in the user's private runtime folder (Linux) or a named pipe (Windows). Third-party tools can use it, for example `{"type":"search","line":"invoice ext:pdf","mode":"all","max":10}`.
- The resident process keeps an **in-memory index** of the home directory, minus excluded folders (`.git`, `node_modules`, `target`, caches, hidden folders). The index is built by a parallel walk at start and kept up to date by watching the filesystem. The indexed folders and exclusions can be configured.
- **Content** search runs on demand, like ripgrep: the index narrows down the candidate files, then they are read in parallel.
- The **global shortcut** is registered directly on Windows and X11, and through the XDG `GlobalShortcuts` portal on Wayland (GNOME, KDE). On other compositors, bind `serafind toggle` to a shortcut in the compositor settings.

## Project layout

| Crate | Role |
|---|---|
| [`crates/serafind-core`](../crates/serafind-core) | Search line syntax, modes, matching and ranking. Pure: no disk, no window. |
| [`crates/serafind-index`](../crates/serafind-index) | In-memory index: tree walking, filesystem watching, exclusions. |
| [`crates/serafind-ipc`](../crates/serafind-ipc) | The local protocol: JSON messages over a local socket, client and listener. |
| [`crates/serafind-settings`](../crates/serafind-settings) | `config.toml` and themes: defaults, validation, live reload. No user interface. |
| [`crates/serafind`](../crates/serafind) | The `serafind` executable: command line, resident process, search bar. |

## Built with

| | |
|---|---|
| [Rust](https://www.rust-lang.org/) (edition 2024; version fixed in `rust-toolchain.toml`) | Language |
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

## Logo

A winged magnifying glass: Serafind is *seraphim* + *find*. The current logo is a placeholder; its source is [`crates/serafind/assets/logo/serafind.svg`](../crates/serafind/assets/logo/serafind.svg), and the PNG sizes next to it are rendered from it.
