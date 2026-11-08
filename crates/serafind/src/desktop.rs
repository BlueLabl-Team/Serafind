//! Serafind's place in the desktop: its application id, its `.desktop` file (Linux) and
//! notifications.

use std::path::{Path, PathBuf};

/// The application id: name of the `.desktop` file, identity with the desktop portals and
/// window class. Changing it would lose the shortcut the user accepted in their desktop.
pub const APP_ID: &str = "io.github.bluelabl_team.Serafind";

/// Writes `~/.local/share/applications/<APP_ID>.desktop` and the logo in
/// `~/.local/share/icons/hicolor/256x256/apps/` when they are missing or different, so that the
/// desktop knows Serafind (the global shortcut portal needs it, and it appears among the
/// applications). Returns the files written.
///
/// # Errors
/// When a file cannot be written.
pub fn install_desktop_file() -> std::io::Result<Vec<PathBuf>> {
    let Some((entry, icon)) = desktop_files() else {
        return Ok(Vec::new());
    };
    let executable = std::env::current_exe()?;
    let mut written = Vec::new();
    for (path, content) in [
        (entry, desktop_entry(&executable).into_bytes()),
        (icon, crate::logo::PNG_256.to_vec()),
    ] {
        if std::fs::read(&path).is_ok_and(|existing| existing == content) {
            continue;
        }
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder)?;
        }
        std::fs::write(&path, content)?;
        written.push(path);
    }
    Ok(written)
}

/// Removes the `.desktop` file and the logo. Returns those that existed.
///
/// # Errors
/// When a file exists but cannot be removed.
pub fn uninstall_desktop_file() -> std::io::Result<Vec<PathBuf>> {
    let Some((entry, icon)) = desktop_files() else {
        return Ok(Vec::new());
    };
    let mut removed = Vec::new();
    for path in [entry, icon] {
        match std::fs::remove_file(&path) {
            Ok(()) => removed.push(path),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(removed)
}

/// Where the `.desktop` file and the logo go; `None` where there is no such thing (Windows).
fn desktop_files() -> Option<(PathBuf, PathBuf)> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let data = dirs::data_dir()?;
    Some((
        data.join("applications").join(format!("{APP_ID}.desktop")),
        data.join("icons/hicolor/256x256/apps")
            .join(format!("{APP_ID}.png")),
    ))
}

/// The `.desktop` entry: opening it from the applications toggles the bar.
fn desktop_entry(executable: &Path) -> String {
    let exec = executable
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace(' ', "\\s");
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Serafind\n\
         GenericName=Search\n\
         Comment=Search your files and applications\n\
         Exec={exec} toggle\n\
         Icon={APP_ID}\n\
         Terminal=false\n\
         Categories=Utility;\n\
         Keywords=search;find;files;launcher;\n\
         StartupNotify=false\n"
    )
}

/// Opens a file or folder with the desktop's default application (the file manager for a
/// folder).
///
/// # Errors
/// When the opener cannot be started.
pub fn open(path: &Path, activation_token: Option<&str>) -> std::io::Result<()> {
    let mut command = if cfg!(windows) {
        let mut command = std::process::Command::new("explorer");
        command.arg(path);
        command
    } else {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(path);
        command
    };
    if let Some(token) = activation_token {
        // Lets the opened window take the focus on Wayland.
        command.env("XDG_ACTIVATION_TOKEN", token);
    }
    let mut child = command.spawn()?;
    std::thread::spawn(move || child.wait());
    Ok(())
}

/// Shows a desktop notification (Linux, through the notification portal). Failures are only
/// logged: a notification is a courtesy.
pub fn notify(title: &str, body: &str) {
    #[cfg(target_os = "linux")]
    {
        use ashpd::desktop::notification::{Notification, NotificationProxy};
        let result = futures_lite::future::block_on(async {
            if let Ok(app_id) = ashpd::AppID::try_from(APP_ID) {
                let _ = ashpd::register_host_app(app_id).await;
            }
            let proxy = NotificationProxy::new().await?;
            proxy
                .add_notification("serafind", Notification::new(title).body(body))
                .await
        });
        if let Err(error) = result {
            eprintln!("serafind: cannot show a notification: {error}");
        }
    }
    #[cfg(not(target_os = "linux"))]
    eprintln!("serafind: {title}: {body}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_entry_runs_toggle() {
        let entry = desktop_entry(Path::new("/opt/my apps/serafind"));
        assert!(entry.contains("Exec=/opt/my\\sapps/serafind toggle\n"), "{entry}");
        assert!(entry.starts_with("[Desktop Entry]\n"));
        assert!(entry.contains("Name=Serafind\n"));
    }

    #[test]
    fn app_id_is_valid() {
        assert!(APP_ID.split('.').count() >= 3);
        assert!(
            APP_ID
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_')
        );
    }
}
