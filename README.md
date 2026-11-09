# Serafind

*A BlueLabL project*

Serafind is a quick search bar for Linux and Windows. Press **Super+F** anywhere, type, and open what you were looking for: an application, a folder, a file, or a line of text inside a file. It is a single small program; a light resident process keeps an index of your home folder up to date.

- **Fast**: results while you type, in a few milliseconds over hundreds of thousands of files.
- **Keyboard first**: every mode and action has a key, and every key can be changed.
- **Configurable**: one commented settings file, applied as soon as it is saved.

## Install

On Fedora, the simplest is the COPR repository (first line below). Otherwise, download the file for your system from the [latest release](https://github.com/BlueLabl-Team/Serafind/releases/latest).

| System | File | How |
|---|---|---|
| Fedora 43 and later | [COPR repository](https://copr.fedorainfracloud.org/coprs/archeopsy/Serafind/) | `sudo dnf copr enable archeopsy/Serafind`, then `sudo dnf install serafind`: updates come with the system's |
| Ubuntu, Debian | `serafind_X.Y.Z_amd64.deb` | `sudo apt install ./serafind_X.Y.Z_amd64.deb` |
| Fedora, openSUSE | `serafind-X.Y.Z-1.x86_64.rpm` | `sudo dnf install ./serafind-X.Y.Z-1.x86_64.rpm` |
| Other Linux | `serafind-X.Y.Z-linux-x86_64.tar.gz` | Extract it and put `serafind` in a folder of your `PATH` (e.g. `~/.local/bin`) |
| Windows 10 / 11 | `serafind-X.Y.Z-windows-x86_64-setup.exe` | Run it: it installs for your account only, without administrator rights |
| Windows, no installer | `serafind-X.Y.Z-windows-x86_64.zip` | Extract it anywhere and run `serafind.exe` |

Linux builds need glibc 2.35 or newer (Ubuntu 22.04, Debian 12, Fedora 36 and later). You can check a download against `SHA256SUMS.txt`.

> **Windows:** Serafind is not signed yet, so SmartScreen may warn that it does not know it: choose *More info*, then *Run anyway*.

### From source

With [rustup](https://rustup.rs/) and Git (on Windows, also the *Desktop development with C++* tools of Visual Studio):

```bash
git clone https://github.com/BlueLabl-Team/Serafind.git
cd Serafind
cargo build --release      # the program is target/release/serafind
```

## Getting started

1. **Start Serafind once**: run `serafind` (the Windows installer does it for you). It then starts by itself with every session.
2. **Press Super+F** (the Windows key and F) to open the bar, type, and press **Enter**.
   - On GNOME and KDE (Wayland), the desktop asks once to accept the shortcut.
   - On Windows, Win+F may already be taken by the system: pick another key in the settings (see below).
3. **Something wrong?** `serafind status` tells whether Serafind runs, how big its index is, and which shortcut it got.

## Using the bar

| Key | Action |
|---|---|
| Enter, double click | Open (or start the application) |
| Ctrl+Enter | Show in the file manager |
| Ctrl+C / Ctrl+Shift+C | Copy the path / the name |
| Ctrl+T | Open a terminal in the folder |
| Alt+1 … Alt+9 | Open that result |
| Tab, Ctrl+1 … Ctrl+5 | Mode: **All**, **Apps**, **Folders**, **Files**, **Content** (text inside files) |
| Escape | Close |

Narrow a search with filters: `invoice ext:pdf in:~/Documents -draft`. `c:"text"` searches inside files, `re:"regex"` with a regular expression.

## Settings

Everything is in one folder, created at the first start with every option commented:

- Linux: `~/.config/serafind/config.toml` (and `themes/`)
- Windows: `%APPDATA%\Serafind\config.toml`

Changes apply as soon as the file is saved; a mistake is shown in the bar with its line. For example, another shortcut, only on Windows:

```toml
[shortcut]
windows = "Ctrl+Alt+Space"
```

## Uninstall

Run `serafind uninstall` first (it stops Serafind and removes its start with the session), then remove the package: `sudo apt remove serafind`, `sudo dnf remove serafind`, or *Settings → Apps* on Windows. Your settings folder is kept.

## More

- [Features in detail](docs/features.md): search syntax, every action, applications, text search, shortcut, tray icon, all the settings.
- [How it works](docs/architecture.md), and the [roadmap](docs/roadmap.md).
- [Contributing](CONTRIBUTING.md) and [security](SECURITY.md).

## License

[Apache License 2.0](LICENSE)
