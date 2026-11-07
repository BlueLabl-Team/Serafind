# Serafind

*A BlueLabl project*

Serafind is a quick search bar for Linux and Windows. Press a global shortcut anywhere, type, and open what you were looking for: an application, a folder, a file, or text inside a file. It brings the quick search of [PowerHive](https://github.com/BlueLabl-Team/PowerHive) to the whole desktop, as a single small executable.

> **Status:** early development. Nothing is usable yet. See the [roadmap](#roadmap).

## Goals

- **Fast**: results while typing; under 100 ms from shortcut to bar, under 10 ms per query on about one million files.
- **Light**: a single executable of a few megabytes, low memory use when idle, no runtime to install.
- **Keyboard first**: every mode and action is reachable by a shortcut or a keyword.

## Features

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
- `ext:` extensions, `in:` starting folder (Tab completes it), `type:` `file` or `folder`, `c:` text inside the files and `re:` a regular expression inside the files (Content mode)
- `-` excludes a word or a filter

### Actions

| Key | Action |
|---|---|
| Enter, double click | Open with the default application |
| Ctrl+Enter | Show in the file manager, selected in its folder |
| Ctrl+C | Copy the full path (when no text of the search is selected) |
| Ctrl+Shift+C | Copy the name |
| Ctrl+T | Open a terminal in the folder (the folder of a file) |
| Alt+1 … Alt+9 | Open that result |
| ↑ ↓, Page Up / Down | Move the selection |
| Tab / Shift+Tab, Ctrl+1 … Ctrl+5 | Change the mode (Tab accepts the proposed `in:` folder first) |
| Escape | Close |

Every key can be changed in `[keys]` of `config.toml`. Opening, showing in the folder and opening a terminal close the bar; copying keeps it open (`[behavior] close_after_*`). A single click can open instead of a double click (`[behavior] click = "single"`).

The resident process does the actions. On Linux, it opens through the desktop portal (`OpenURI`), so that the desktop chooses the application, and falls back on `xdg-open`. The terminal is found automatically (`xdg-terminal-exec`, Ptyxis, GNOME Terminal, Konsole, kitty, Alacritty, WezTerm, foot, xterm; Windows Terminal or PowerShell), or set with `[actions] terminal`.

### Text inside files

The **Content** mode (its tab, or `c:"text"` anywhere in the line) finds text inside the indexed files, like ripgrep. Other modes only look at names.

- The words typed are one phrase, found as written: case is ignored unless the phrase has an upper-case letter. `c:"other text"` adds a text that must be there too, `-word` or `-c:` one that must not be. `re:"fix(me)?"` is a regular expression.
- `ext:` and `in:` choose the files read. Binary files and files over 10 MB (`[content] max_file_size`) are skipped.
- The search starts after a short pause in typing (`[content] typing_delay`); results come as the files are read, with the progress in the footer.
- Each file shows the first line found, with its number, and how many lines match.
- Enter opens the file at that line when its default application is an editor that can (VS Code, Zed, Sublime Text, Kate, gedit, vim, Neovim, Emacs, JetBrains IDEs), else opens it as usual. `[actions] open_at_line = "code -g {file}:{line}"` chooses the command.

### Applications

The **Apps** mode, and the top of the **All** mode, list the installed applications, by their name in the desktop's language, with their own icon.

- **Linux:** the `.desktop` files of the system, the user, Flatpak and Snap, as the applications menu shows them (hidden ones and those of other desktops are left out). Serafind starts them itself from their `Exec` line; those that run in a terminal (`Terminal=true`) open in the terminal of `[actions] terminal`.
- **Windows:** the Start menu shortcuts and the Microsoft Store apps.
- The list follows installs and removals.
- On an application, Ctrl+Enter, Ctrl+C and Ctrl+T act on its program (e.g. `/usr/bin/firefox`).
- `[behavior] learn_from_usage = true` puts the applications you start most often first.
- `[appearance] app_icons = "generic"` shows the same icon for every application.

## Usage

```sh
serafind            # start Serafind in the background (or open / close the bar if it runs)
serafind start      # start Serafind in the background if it does not run (used with the session)
serafind toggle     # open the bar, or close it
serafind show       # open the bar
serafind hide       # close the bar
serafind status     # is it running? how big is the index?
serafind quit       # stop Serafind
serafind daemon     # run in this terminal, to see its messages
serafind uninstall  # remove what Serafind added to the desktop (.desktop file, logo, start with the session)
```

### Start with the session

Serafind starts when the session opens (`[startup] with_session = true`, the default): an autostart entry in `~/.config/autostart` on Linux, a `Run` registry value on Windows, both running `serafind start`. Setting it to `false` removes it at once; the desktop's own settings (GNOME Tweaks, the Task Manager on Windows) can turn it off too. The index is built at low priority (CPU and disk), so that it does not slow the session down.

### Global shortcut

`Super+F` (the Windows / logo key and F) opens and closes the bar. Change it with `[shortcut] toggle` in `config.toml`; it applies without restarting.

- **Wayland (GNOME, KDE):** Serafind asks the desktop through the `GlobalShortcuts` portal. The first time, the desktop asks you to accept the shortcut, and may let you pick another key. The desktop has the last word: the key it gave is shown by `serafind status`, and can be changed in its keyboard settings.
- **X11 and Windows:** Serafind registers the shortcut itself.
- **When it fails** (key taken, no portal), the reason is shown by `serafind status`, at the bottom of the bar and in a notification. Then bind `serafind toggle` to a key in your desktop's keyboard settings, and set `[shortcut] enabled = false`.

On Linux, Serafind writes `~/.local/share/applications/io.github.bluelabl_team.Serafind.desktop` and its logo in `~/.local/share/icons/hicolor/`: the portal needs them to know the application.

### Notification area icon

Off by default; turn it on with `[tray] enabled = true` in `config.toml`. A click on the icon opens or closes the bar; its menu opens the bar, indexes again, opens the settings folder or quits Serafind. `serafind status` tells whether it is shown.

- **Linux:** KDE and most desktops show it. **GNOME** needs the [AppIndicator and KStatusNotifierItem Support](https://extensions.gnome.org/extension/615/appindicator-support/) extension (`gnome-shell-extension-appindicator` on Fedora); restart Serafind after installing it.
- **Windows:** in the notification area of the taskbar.

Messages of the background process go to `~/.local/state/serafind/serafind.log` (Linux) or the local application data folder (Windows).

## How it works

- **Two processes.** A small resident process (a few MB, no window) keeps the index, listens for the shortcut and answers on a local socket. The bar is a separate process, started on demand, that searches through that socket. Between openings the bar is hidden and kept (instant), or closed to free its memory (`[behavior] keep_window`). On Wayland, a window cannot be hidden, so the bar is always closed and started again (about 50 ms).
- **Local protocol.** JSON messages, one per line, on a Unix socket in the user's private runtime folder (Linux) or a named pipe (Windows). Third-party tools can use it, for example `{"type":"search","line":"invoice ext:pdf","mode":"all","max":10}`.
- The resident process keeps an **in-memory index** of the home directory, minus excluded folders (`.git`, `node_modules`, `target`, caches, hidden folders). The index is built by a parallel walk at start and kept up to date by watching the filesystem. The indexed folders and exclusions can be configured.
- **Content** search runs on demand, like ripgrep: the index narrows down the candidate files, then they are read in parallel.
- The **global shortcut** is registered directly on Windows and X11, and through the XDG `GlobalShortcuts` portal on Wayland (GNOME, KDE). On other compositors, bind `serafind toggle` to a shortcut in the compositor settings.

## Settings and themes

Everything is set in one folder: `~/.config/serafind/` on Linux, `%APPDATA%\Serafind\` on Windows. It is created on first launch, with every option commented:

```
serafind/
├── config.toml        bar size and text size, layout, behavior, indexed folders, shortcut, theme choice
└── themes/
    ├── dark.toml      built-in themes (PowerHive's Default Dark and Default Light), written as examples
    ├── light.toml
    └── my-theme.toml  your own: extends = "dark", then only what changes
```

- Changes apply as soon as a file is saved.
- Every key is optional: a missing key keeps its default.
- An unknown key, a wrong value or a syntax error makes the file be refused. The previous settings stay, and the error is shown in the bar with its file and line.
- Every dimension of the bar (width, padding, row height, text size, number of results) is in `[bar]` of `config.toml`; themes only hold the look (colors, font, corners, shadow, icons).
- Themes use the same color roles as PowerHive (`primary`, `accent`, `surface_elevated`, `text_secondary`…), so both applications look alike.
- `theme = "system"` follows the dark or light mode of the desktop. Font and icons come from the theme, and `[appearance]` in `config.toml` can override them.
- The global shortcut is `Super+F` by default (`[shortcut] toggle`), with an optional per-system value.

## Logo

A winged magnifying glass: Serafind is *seraphim* + *find*. The current logo is a placeholder; its source is [`crates/serafind/assets/logo/serafind.svg`](crates/serafind/assets/logo/serafind.svg), and the PNG sizes next to it are rendered from it.

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
| [`crates/serafind-ipc`](crates/serafind-ipc) | The local protocol: JSON messages over a local socket, client and listener. |
| [`crates/serafind-settings`](crates/serafind-settings) | `config.toml` and themes: defaults, validation, live reload. No user interface. |
| [`crates/serafind`](crates/serafind) | The `serafind` executable: command line, resident process, search bar. |

## Build from source

You need Rust (latest stable) and Git.

```bash
git clone git@github.com:BlueLabl-Team/Serafind.git
cd Serafind
cargo run --release -p serafind  # starts Serafind; then press Super+F
cargo build --profile dist       # smallest executable, for publishing: target/dist/serafind
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
| 4 | Resident process: single instance, `serafind toggle`, global shortcut (X11, Windows, Wayland portal), tray icon | ✅ |
| 4b | Choose where the bar appears on Wayland (layer-shell, focus on X11, or another way: to be decided) | ⬜ |
| 5 | Actions: open, show in the file manager, copy the path or name, terminal; configurable keys | ✅ |
| 6 | Apps mode: `.desktop` files with Flatpak and Snap (Linux), Start menu and Store apps (Windows), real icons | ✅ |
| 7 | Content mode: on-demand parallel search, `re:` expressions, open at the line | ✅ |
| 8 | Start with the session; settings for indexed and excluded folders applied live; low-priority indexing | ✅ |
| 9 | GitHub Actions: build, tests, releases (Linux and Windows executables) | ⬜ |
| 10 | Integration in third-party software through their plugin / extension systems (library, IPC protocol or C ABI: to be decided) | ⬜ |
| Later | After the first release: text of PDF and Office documents in the Content mode | ⬜ |
| Later | After the first release: actions of applications (e.g. Firefox's "New Private Window") as results | ⬜ |

## Known issues

- **GNOME 48 says the shortcut failed when it worked.** When an application asks again for a shortcut it already holds, `xdg-desktop-portal-gnome` 48 binds it but answers `BindShortcuts` with a failure. Serafind then asks the portal which shortcuts it really holds (`ListShortcuts`) before reporting a problem. If `serafind status` shows an error while the key works, the workaround did not apply: please open an issue with the output of `serafind status`.

## License

[Apache License 2.0](LICENSE)
