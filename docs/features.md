# Serafind: features in detail

The [README](../README.md) gives the essentials; this page goes into each feature.

## Search syntax

Each part of the line is understood and shown as a chip:

```
invoice ext:pdf,docx in:~/Documents type:file -draft "exact phrase" rep*rt
```

- words (or `"quoted phrases"`) the name must contain; `*` and `?` make a pattern found anywhere in the name (`rep*rt` finds `my-report.pdf`), `^` and `$` anchor it at the start or the end (`^inv`, `.pdf$`); case is ignored unless the word has an upper-case letter, accents are always ignored
- `ext:` extensions, `in:` starting folder (Tab completes it), `type:` `file` or `folder`, `c:` text inside the files and `re:` a regular expression inside the files (Content mode)
- `-` excludes a word or a filter

## Actions

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

## Text inside files

The **Content** mode (its tab, or `c:"text"` anywhere in the line) finds text inside the indexed files, like ripgrep. Other modes only look at names.

- The words typed are one phrase, found as written: case is ignored unless the phrase has an upper-case letter. `c:"other text"` adds a text that must be there too, `-word` or `-c:` one that must not be. `re:"fix(me)?"` is a regular expression.
- `ext:` and `in:` choose the files read. Binary files and files over 10 MB (`[content] max_file_size`) are skipped.
- The search starts after a short pause in typing (`[content] typing_delay`); results come as the files are read, with the progress in the footer.
- Each file shows the first line found, with its number, and how many lines match.
- Enter opens the file at that line when its default application is an editor that can (VS Code, Zed, Sublime Text, Kate, gedit, vim, Neovim, Emacs, JetBrains IDEs), else opens it as usual. `[actions] open_at_line = "code -g {file}:{line}"` chooses the command.

## Applications

The **Apps** mode, and the top of the **All** mode, list the installed applications, by their name in the desktop's language, with their own icon.

- **Linux:** the `.desktop` files of the system, the user, Flatpak and Snap, as the applications menu shows them (hidden ones and those of other desktops are left out). Serafind starts them itself from their `Exec` line; those that run in a terminal (`Terminal=true`) open in the terminal of `[actions] terminal`.
- **Windows:** the Start menu shortcuts and the Microsoft Store apps.
- The list follows installs and removals.
- On an application, Ctrl+Enter, Ctrl+C and Ctrl+T act on its program (e.g. `/usr/bin/firefox`).
- `[behavior] learn_from_usage = true` puts the applications you start most often first.
- `[appearance] app_icons = "generic"` shows the same icon for every application.

## Command line

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

## Start with the session

Serafind starts when the session opens (`[startup] with_session = true`, the default): an autostart entry in `~/.config/autostart` on Linux, a `Run` registry value on Windows, both running `serafind start`. Setting it to `false` removes it at once; the desktop's own settings (GNOME Tweaks, the Task Manager on Windows) can turn it off too. The index is built at low priority (CPU and disk), so that it does not slow the session down.

## Global shortcut

`Super+F` (the Windows / logo key and F) opens and closes the bar. Change it with `[shortcut] toggle` in `config.toml`; it applies without restarting.

- **Wayland (GNOME, KDE):** Serafind asks the desktop through the `GlobalShortcuts` portal. The first time, the desktop asks you to accept the shortcut, and may let you pick another key. The desktop has the last word: the key it gave is shown by `serafind status`, and can be changed in its keyboard settings.
- **X11 and Windows:** Serafind registers the shortcut itself.
- **When it fails** (key taken, no portal), the reason is shown by `serafind status`, at the bottom of the bar and in a notification. Then bind `serafind toggle` to a key in your desktop's keyboard settings, and set `[shortcut] enabled = false`.

On Linux, Serafind writes `~/.local/share/applications/io.github.bluelabl_team.Serafind.desktop` and its logo in `~/.local/share/icons/hicolor/`: the portal needs them to know the application.

## Notification area icon

Off by default; turn it on with `[tray] enabled = true` in `config.toml`. A click on the icon opens or closes the bar; its menu opens the bar, indexes again, opens the settings folder or quits Serafind. `serafind status` tells whether it is shown.

- **Linux:** KDE and most desktops show it. **GNOME** needs the [AppIndicator and KStatusNotifierItem Support](https://extensions.gnome.org/extension/615/appindicator-support/) extension (`gnome-shell-extension-appindicator` on Fedora). When the notification area is not there yet (at login, or before the extension is turned on), the icon waits for it and appears as soon as it comes.
- **Windows:** in the notification area of the taskbar.

Messages of the background process go to `~/.local/state/serafind/serafind.log` (Linux) or the local application data folder (Windows).

## Settings and themes

Everything is set in one folder: `~/.config/serafind/` on Linux, `%APPDATA%\Serafind\` on Windows. It is created on first launch, with every option commented:

```
serafind/
├── config.toml        bar size and text size, layout, behavior, indexed folders, shortcut, theme choice
└── themes/
    ├── dark.toml      built-in themes (Default Dark and Default Light), written as examples
    ├── light.toml
    └── my-theme.toml  your own: extends = "dark", then only what changes
```

- Changes apply as soon as a file is saved.
- Every key is optional: a missing key keeps its default.
- An unknown key, a wrong value or a syntax error makes the file be refused. The previous settings stay, and the error is shown in the bar with its file and line.
- Every dimension of the bar (width, padding, row height, text size, number of results) is in `[bar]` of `config.toml`; themes only hold the look (colors, font, corners, shadow, icons).
- Themes are made of color roles (`primary`, `accent`, `surface_elevated`, `text_secondary`…).
- `theme = "system"` follows the dark or light mode of the desktop. Font and icons come from the theme, and `[appearance]` in `config.toml` can override them.
- The global shortcut is `Super+F` by default (`[shortcut] toggle`), with an optional per-system value.

## Known issues

- **GNOME 48 says the shortcut failed when it worked.** When an application asks again for a shortcut it already holds, `xdg-desktop-portal-gnome` 48 binds it but answers `BindShortcuts` with a failure. Serafind then asks the portal which shortcuts it really holds (`ListShortcuts`) before reporting a problem. If `serafind status` shows an error while the key works, the workaround did not apply: please open an issue with the output of `serafind status`.
