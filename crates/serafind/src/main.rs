//! Serafind: a quick search bar opened by a global shortcut.
//!
//! For now `serafind` opens the bar directly; the resident process and the global shortcut come
//! with step 4 of the roadmap.

mod app;
mod bar;
mod completion;
mod format;
mod icons;
mod search;
mod strings;
mod style;

use std::path::PathBuf;
use std::process::ExitCode;

use eframe::egui;
use serafind_settings::{Backend, Settings, SystemMode};

fn main() -> ExitCode {
    let mut arguments = std::env::args().skip(1);
    let mut settings_dir = serafind_settings::default_dir();
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--version" | "-V" => {
                println!("serafind {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            "--help" | "-h" => {
                println!("{HELP}");
                return ExitCode::SUCCESS;
            }
            "--settings" => match arguments.next() {
                Some(dir) => settings_dir = Some(PathBuf::from(dir)),
                None => return usage_error("--settings needs a folder"),
            },
            other => return usage_error(&format!("unknown argument {other:?}")),
        }
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
    // The desktop mode is only known once the window exists: the app reloads if it differs.
    let (settings, settings_error) = match settings_dir
        .as_deref()
        .map(|dir| serafind_settings::load(dir, SystemMode::Dark))
    {
        Some(Ok(settings)) => (settings, None),
        Some(Err(error)) => (Settings::defaults(SystemMode::Dark), Some(error.to_string())),
        None => (Settings::defaults(SystemMode::Dark), None),
    };

    let window = &settings.config.window;
    let width = f32::from(window.width) + 2.0 * f32::from(app::SHADOW_MARGIN);
    let viewport = egui::ViewportBuilder::default()
        .with_title("Serafind")
        .with_app_id("serafind")
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(false)
        .with_always_on_top()
        .with_taskbar(false)
        .with_visible(true)
        .with_inner_size([width, 80.0]);
    let mut options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Glow,
        ..eframe::NativeOptions::default()
    };
    force_x11_if_asked(&mut options, window.backend);

    let result = eframe::run_native(
        "Serafind",
        options,
        Box::new(move |creation| {
            Ok(Box::new(app::App::new(
                &creation.egui_ctx,
                settings,
                settings_error,
                settings_dir,
                home,
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

const HELP: &str = "\
Serafind: a quick search bar.

Usage: serafind [--settings <folder>]

Options:
  --settings <folder>  Use another settings folder (default: ~/.config/serafind)
  -V, --version        Show the version
  -h, --help           Show this help";

fn usage_error(message: &str) -> ExitCode {
    eprintln!("serafind: {message}\n\n{HELP}");
    ExitCode::from(2)
}

/// On Linux, shows the bar through X11 (`XWayland` on Wayland desktops) unless the settings ask
/// for native Wayland, or no X server is there.
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
