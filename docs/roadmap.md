# Serafind: roadmap

| # | Step | Status |
|---|---|---|
| 0 | Workspace, test tooling, roadmap | ✅ |
| 1 | `core`: search syntax (filters + mode keywords), matching, ranking (fuzzy / prefix) | ✅ |
| 2 | `index`: parallel first walk, filesystem watching, exclusions, benchmarks | ✅ |
| 3 | Settings and themes (TOML, defaults, validation, live reload) | ✅ |
| 3 | Search bar (egui): input, chips, results, keyboard navigation, `in:` completion | ✅ |
| 4 | Resident process: single instance, `serafind toggle`, global shortcut (X11, Windows, Wayland portal), tray icon | ✅ |
| 4b | Choose where the bar appears on Wayland (layer-shell, focus on X11, or another way: to be decided) | ⬜ |
| 5 | Actions: open, show in the file manager, copy the path or name, terminal; configurable keys | ✅ |
| 6 | Apps mode: `.desktop` files with Flatpak and Snap (Linux), Start menu and Store apps (Windows), real icons | ✅ |
| 7 | Content mode: on-demand parallel search, `re:` expressions, open at the line | ✅ |
| 8 | Start with the session; settings for indexed and excluded folders applied live; low-priority indexing | ✅ |
| 9 | GitHub Actions: checks on Linux and Windows, dependency audit, releases (archives, .deb, .rpm, Windows installer) | ✅ |
| 9b | Package managers, after the first release: dnf (COPR), Chocolatey, winget… | ⬜ |
| 10 | Integration in third-party software through their plugin / extension systems (library, IPC protocol or C ABI: to be decided) | ⬜ |
| Later | After the first release: text of PDF and Office documents in the Content mode | ⬜ |
| Later | After the first release: actions of applications (e.g. Firefox's "New Private Window") as results | ⬜ |
