//! Serafind's place in the desktop: its application id, its `.desktop` file (Linux) and
//! notifications.

use std::path::{Path, PathBuf};

/// The application id: name of the `.desktop` file, identity with the desktop portals and
/// window class. Changing it would lose the shortcut the user accepted in their desktop.
pub const APP_ID: &str = "io.github.bluelabl_team.Serafind";

/// Writes `~/.local/share/applications/<APP_ID>.desktop` when it is missing or points to
/// another executable, so that the desktop knows Serafind (the global shortcut portal needs it,
/// and it appears among the applications). Returns the file when it was written.
///
/// # Errors
/// When the file cannot be written.
pub fn install_desktop_file() -> std::io::Result<Option<PathBuf>> {
    let Some(path) = desktop_file() else {
        return Ok(None);
    };
    let executable = std::env::current_exe()?;
    let content = desktop_entry(&executable);
    if std::fs::read_to_string(&path).is_ok_and(|existing| existing == content) {
        return Ok(None);
    }
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder)?;
    }
    std::fs::write(&path, content)?;
    Ok(Some(path))
}

/// Removes the `.desktop` file. Returns it when it existed.
///
/// # Errors
/// When the file exists but cannot be removed.
pub fn uninstall_desktop_file() -> std::io::Result<Option<PathBuf>> {
    let Some(path) = desktop_file() else {
        return Ok(None);
    };
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(Some(path)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// Where the `.desktop` file goes; `None` where there is no such thing (Windows).
fn desktop_file() -> Option<PathBuf> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    Some(
        dirs::data_dir()?
            .join("applications")
            .join(format!("{APP_ID}.desktop")),
    )
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
         Icon=system-search\n\
         Terminal=false\n\
         Categories=Utility;\n\
         Keywords=search;find;files;launcher;\n\
         StartupNotify=false\n"
    )
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
