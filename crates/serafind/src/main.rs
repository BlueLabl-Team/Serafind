//! Serafind: a quick search bar opened by a global shortcut.
//!
//! `serafind` starts the resident process in the background, or opens / closes the bar when it
//! already runs. The resident process starts the bar process (`serafind bar`) when needed.

mod actions;
mod app;
mod bar;
mod completion;
mod daemon;
mod desktop;
mod format;
mod icons;
mod logo;
mod search;
mod shortcut;
mod strings;
mod style;
mod tray;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use eframe::egui;
use serafind_ipc::{Client, Request, Response};
use serafind_settings::{Backend, Settings, SystemMode};

const HELP: &str = "\
Serafind: a quick search bar.

Usage: serafind [command] [--settings <folder>]

Commands:
  (none)    Start Serafind in the background, or open / close the bar if it runs
  toggle    Open the bar, or close it
  show      Open the bar
  hide      Close the bar
  status    Show whether Serafind runs, and its index
  quit      Stop Serafind
  daemon    Run Serafind in this terminal (for debugging)
  uninstall Stop Serafind and remove its desktop entry (settings are kept)

Options:
  --settings <folder>  Use another settings folder (default: ~/.config/serafind)
  -V, --version        Show the version
  -h, --help           Show this help";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Command {
    Start,
    Toggle,
    Show,
    Hide,
    Status,
    Quit,
    Daemon,
    Bar,
    Uninstall,
}

fn main() -> ExitCode {
    let mut command = Command::Start;
    let mut settings_dir = serafind_settings::default_dir();
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        command = match argument.as_str() {
            "--version" | "-V" => {
                println!("serafind {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            "--help" | "-h" => {
                println!("{HELP}");
                return ExitCode::SUCCESS;
            }
            "--settings" => match arguments.next() {
                Some(dir) => {
                    settings_dir = Some(PathBuf::from(dir));
                    continue;
                }
                None => return usage_error("--settings needs a folder"),
            },
            "toggle" => Command::Toggle,
            "show" => Command::Show,
            "hide" => Command::Hide,
            "status" => Command::Status,
            "quit" => Command::Quit,
            "daemon" => Command::Daemon,
            "bar" => Command::Bar,
            "uninstall" => Command::Uninstall,
            other => return usage_error(&format!("unknown argument {other:?}")),
        };
    }

    let Some(home) = dirs::home_dir() else {
        eprintln!("serafind: no home folder found");
        return ExitCode::FAILURE;
    };
    if let Some(dir) = &settings_dir
        && let Err(error) = serafind_settings::write_defaults(dir)
    {
        eprintln!(
            "serafind: cannot write the default settings in {}: {error}",
            dir.display()
        );
    }

    run(command, settings_dir, home)
}

/// The Wayland activation token this command was started with (a desktop shortcut gives one):
/// passed on to the bar so that its window gets the keyboard focus.
fn activation_token() -> Option<String> {
    std::env::var("XDG_ACTIVATION_TOKEN")
        .ok()
        .filter(|token| !token.is_empty())
}

/// Runs a command once the arguments are read.
fn run(command: Command, settings_dir: Option<PathBuf>, home: PathBuf) -> ExitCode {
    match command {
        Command::Daemon => match daemon::run(settings_dir, home) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("serafind: {error}");
                ExitCode::FAILURE
            }
        },
        Command::Bar => run_bar(settings_dir, home),
        Command::Uninstall => {
            if let Ok(mut client) = Client::connect() {
                let _ = client.call(&Request::Quit);
            }
            match desktop::uninstall_desktop_file() {
                Ok(removed) if removed.is_empty() => println!("No desktop entry to remove."),
                Ok(removed) => {
                    for path in removed {
                        println!("Removed {}.", path.display());
                    }
                }
                Err(error) => {
                    eprintln!("serafind: {error}");
                    return ExitCode::FAILURE;
                }
            }
            if let Some(dir) = &settings_dir {
                println!("Settings are kept in {}.", dir.display());
            }
            ExitCode::SUCCESS
        }
        Command::Start => match Client::connect() {
            Ok(mut client) => send(
                &mut client,
                &Request::Toggle {
                    activation_token: activation_token(),
                },
            ),
            Err(_) => start_in_background(settings_dir.as_deref(), false),
        },
        Command::Toggle | Command::Show => {
            let request = if command == Command::Toggle {
                Request::Toggle {
                    activation_token: activation_token(),
                }
            } else {
                Request::Show {
                    activation_token: activation_token(),
                }
            };
            match Client::connect() {
                Ok(mut client) => send(&mut client, &request),
                // Not running: start it and open the bar.
                Err(_) => start_in_background(settings_dir.as_deref(), true),
            }
        }
        Command::Hide | Command::Quit | Command::Status => {
            let Ok(mut client) = Client::connect() else {
                println!("Serafind is not running.");
                return if command == Command::Status {
                    ExitCode::from(3)
                } else {
                    ExitCode::SUCCESS
                };
            };
            let request = match command {
                Command::Hide => Request::Hide,
                Command::Quit => Request::Quit,
                _ => Request::Status,
            };
            send(&mut client, &request)
        }
    }
}

fn usage_error(message: &str) -> ExitCode {
    eprintln!("serafind: {message}\n\n{HELP}");
    ExitCode::from(2)
}

/// Sends one request to the resident process and prints its answer when there is one to print.
fn send(client: &mut Client, request: &Request) -> ExitCode {
    match client.call(request) {
        Ok(Response::Ok) => {
            if *request == Request::Quit {
                // Wait until it has let the socket go, so that a new start right after works.
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
                while Client::connect().is_ok() && std::time::Instant::now() < deadline {
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                println!("Serafind stopped.");
            }
            ExitCode::SUCCESS
        }
        Ok(Response::Status(status)) => {
            println!("Serafind {} is running (process {}).", status.version, status.pid);
            if status.index_ready {
                println!("Index: {} entries.", status.entries);
            } else {
                println!("Index: being built.");
            }
            let bar = match (status.bar_running, status.bar_visible) {
                (_, true) => "open",
                (true, false) => "hidden",
                (false, false) => "closed",
            };
            println!("Bar: {bar}.");
            match (&status.shortcut, &status.shortcut_problem) {
                (_, Some(problem)) => println!("Shortcut: none: {problem}."),
                (Some(shortcut), None) => println!("Shortcut: {shortcut}."),
                (None, None) => println!("Shortcut: being set up."),
            }
            match (status.tray_enabled, &status.tray_problem) {
                (false, _) => println!("Tray icon: off ([tray] enabled = false)."),
                (true, Some(problem)) => println!("Tray icon: not shown: {problem}."),
                (true, None) => println!("Tray icon: shown."),
            }
            ExitCode::SUCCESS
        }
        Ok(other) => {
            eprintln!("serafind: unexpected answer: {other:?}");
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("serafind: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Starts `serafind daemon` detached from this terminal, waits until it answers, and opens the
/// bar when asked. Its messages go to a log file.
fn start_in_background(settings_dir: Option<&Path>, show: bool) -> ExitCode {
    let log_path = log_file();
    let result = (|| -> std::io::Result<()> {
        let mut command = std::process::Command::new(std::env::current_exe()?);
        command.arg("daemon");
        if let Some(dir) = settings_dir {
            command.arg("--settings").arg(dir);
        }
        command
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null());
        match log_path
            .as_ref()
            .and_then(|path| std::fs::File::create(path).ok())
        {
            Some(log) => command.stderr(log),
            None => command.stderr(std::process::Stdio::null()),
        };
        detach(&mut command);
        command.spawn().map(drop)
    })();
    if let Err(error) = result {
        eprintln!("serafind: cannot start: {error}");
        return ExitCode::FAILURE;
    }

    let started = Instant::now();
    let mut client = loop {
        if let Ok(client) = Client::connect() {
            break client;
        }
        if started.elapsed() > Duration::from_secs(5) {
            eprintln!("serafind: started, but it does not answer");
            if let Some(log) = log_path
                .as_ref()
                .and_then(|path| std::fs::read_to_string(path).ok())
            {
                for line in log.lines().rev().take(5).collect::<Vec<_>>().into_iter().rev() {
                    eprintln!("  {line}");
                }
            }
            return ExitCode::FAILURE;
        }
        std::thread::sleep(Duration::from_millis(30));
    };
    if show {
        return send(
            &mut client,
            &Request::Show {
                activation_token: activation_token(),
            },
        );
    }
    println!(
        "Serafind started. Open the bar with the global shortcut (see `serafind status`) or `serafind toggle`."
    );
    if let Some(path) = log_path {
        println!("Log: {}", path.display());
    }
    ExitCode::SUCCESS
}

/// Where the resident process writes its messages: `~/.local/state/serafind/serafind.log` on
/// Linux, the local application data folder on Windows.
fn log_file() -> Option<PathBuf> {
    let folder = dirs::state_dir().or_else(dirs::data_local_dir)?.join("serafind");
    std::fs::create_dir_all(&folder).ok()?;
    Some(folder.join("serafind.log"))
}

/// Separates the started process from this terminal, so that closing it does not stop Serafind.
#[cfg(unix)]
fn detach(command: &mut std::process::Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(windows)]
fn detach(command: &mut std::process::Command) {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
}

#[cfg(not(any(unix, windows)))]
fn detach(_command: &mut std::process::Command) {}

/// The bar process: started by the resident process, searches through it.
fn run_bar(settings_dir: Option<PathBuf>, home: PathBuf) -> ExitCode {
    // The desktop mode is only known once the window exists: the app reloads if it differs.
    let (settings, settings_error) = match settings_dir
        .as_deref()
        .map(|dir| serafind_settings::load(dir, SystemMode::Dark))
    {
        Some(Ok(settings)) => (settings, None),
        Some(Err(error)) => (Settings::defaults(SystemMode::Dark), Some(error.to_string())),
        None => (Settings::defaults(SystemMode::Dark), None),
    };
    let mut client = match Client::connect() {
        Ok(client) => client,
        Err(error) => {
            eprintln!("serafind: the bar needs Serafind running ({error}); start it with `serafind`");
            return ExitCode::FAILURE;
        }
    };

    // Shown in the bar's footer when the global shortcut could not be set up.
    let shortcut_problem = match client.call(&Request::Status) {
        Ok(Response::Status(status)) => status.shortcut_problem,
        _ => None,
    };

    let window = &settings.config.window;
    let width = f32::from(settings.config.bar.width) + 2.0 * f32::from(app::SHADOW_MARGIN);
    let viewport = egui::ViewportBuilder::default()
        .with_title("Serafind")
        .with_app_id(desktop::APP_ID)
        .with_icon(window_icon())
        .with_decorations(false)
        .with_transparent(true)
        // Not "false": on Wayland that freezes the size at creation and the bar could not grow.
        .with_resizable(true)
        .with_always_on_top()
        .with_taskbar(false)
        .with_visible(true)
        .with_inner_size([width, 80.0]);
    let mut options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Glow,
        ..eframe::NativeOptions::default()
    };
    let wayland = uses_wayland(window.backend);
    force_x11_if_asked(&mut options, window.backend);

    let result = eframe::run_native(
        "Serafind",
        options,
        Box::new(move |creation| {
            let link = match app::Link::subscribe(&creation.egui_ctx) {
                Ok(link) => Some(link),
                Err(error) => {
                    eprintln!("serafind: cannot follow Serafind: {error}");
                    None
                }
            };
            Ok(Box::new(app::App::new(
                &creation.egui_ctx,
                app::Startup {
                    settings,
                    settings_error,
                    settings_dir,
                    searcher: search::Searcher::remote(client, home),
                    link,
                    can_hide: !wayland,
                    shortcut_problem,
                },
            )))
        }),
    );
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("serafind: {error}");
            ExitCode::FAILURE
        }
    }
}

/// True when the bar's window will be a native Wayland window (which cannot be hidden).
fn uses_wayland(backend: Backend) -> bool {
    cfg!(target_os = "linux")
        && std::env::var_os("WAYLAND_DISPLAY").is_some()
        && (backend == Backend::Wayland || std::env::var_os("DISPLAY").is_none())
}

/// On Linux, shows the bar through X11 (`XWayland` on Wayland desktops) when the settings ask for
/// it and an X server is there.
#[cfg(target_os = "linux")]
fn force_x11_if_asked(options: &mut eframe::NativeOptions, backend: Backend) {
    use winit::platform::x11::EventLoopBuilderExtX11;
    if backend == Backend::X11 && std::env::var_os("DISPLAY").is_some() {
        options.event_loop_builder = Some(Box::new(|builder| {
            builder.with_x11();
        }));
    }
}

#[cfg(not(target_os = "linux"))]
fn force_x11_if_asked(_options: &mut eframe::NativeOptions, _backend: Backend) {}

/// The logo as the window icon (X11 and Windows; on Wayland the desktop takes it from the
/// `.desktop` file).
fn window_icon() -> egui::IconData {
    let logo = logo::decode(logo::PNG_64);
    egui::IconData {
        rgba: logo.rgba,
        width: logo.width,
        height: logo.height,
    }
}
