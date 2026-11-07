//! Starting Serafind with the session (`[startup] with_session`): an XDG autostart file on
//! Linux, a `Run` registry value on Windows. Both run `serafind start`.

use std::path::{Path, PathBuf};

use crate::desktop::APP_ID;

/// Registers Serafind to start with the session, or removes the registration. Returns what
/// changed, to log (`None` when it already was as asked).
///
/// # Errors
/// When the registration cannot be written or removed.
pub fn apply(with_session: bool) -> std::io::Result<Option<String>> {
    let executable = std::env::current_exe()?;
    #[cfg(not(windows))]
    {
        let Some(file) = autostart_file() else {
            return Ok(None);
        };
        apply_file(&file, with_session, &executable)
    }
    #[cfg(windows)]
    {
        windows::apply(with_session, &executable)
    }
}

/// `~/.config/autostart/<APP_ID>.desktop`.
#[must_use]
pub fn autostart_file() -> Option<PathBuf> {
    Some(
        dirs::config_dir()?
            .join("autostart")
            .join(format!("{APP_ID}.desktop")),
    )
}

/// Writes or removes the autostart file.
fn apply_file(file: &Path, with_session: bool, executable: &Path) -> std::io::Result<Option<String>> {
    if !with_session {
        return match std::fs::remove_file(file) {
            Ok(()) => Ok(Some(format!("removed {}", file.display()))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        };
    }
    let content = autostart_entry(executable);
    if std::fs::read_to_string(file).is_ok_and(|existing| existing == content) {
        return Ok(None);
    }
    if let Some(folder) = file.parent() {
        std::fs::create_dir_all(folder)?;
    }
    std::fs::write(file, content)?;
    Ok(Some(format!("written {}", file.display())))
}

/// The autostart entry: runs `serafind start` when the session opens.
fn autostart_entry(executable: &Path) -> String {
    let exec = executable
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace(' ', "\\s");
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Serafind\n\
         Comment=Quick search bar, opened with a global shortcut\n\
         Exec={exec} start\n\
         Icon={APP_ID}\n\
         Terminal=false\n\
         NoDisplay=true\n\
         X-GNOME-Autostart-enabled=true\n"
    )
}

#[cfg(windows)]
mod windows {
    use std::os::windows::process::CommandExt;
    use std::path::Path;
    use std::process::Command;

    const KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
    const VALUE: &str = "Serafind";
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    /// Sets or deletes the `Run` value with `reg.exe`.
    pub fn apply(with_session: bool, executable: &Path) -> std::io::Result<Option<String>> {
        let wanted = format!("\"{}\" start", executable.display());
        let current = Command::new("reg")
            .args(["query", KEY, "/v", VALUE])
            .creation_flags(CREATE_NO_WINDOW)
            .output()?;
        let text = String::from_utf8_lossy(&current.stdout);
        let present = current.status.success();
        if with_session && present && text.contains(&wanted) || !with_session && !present {
            return Ok(None);
        }
        let mut command = Command::new("reg");
        if with_session {
            command.args(["add", KEY, "/v", VALUE, "/t", "REG_SZ", "/d", &wanted, "/f"]);
        } else {
            command.args(["delete", KEY, "/v", VALUE, "/f"]);
        }
        let status = command.creation_flags(CREATE_NO_WINDOW).status()?;
        if !status.success() {
            return Err(std::io::Error::other(format!("reg.exe failed ({status})")));
        }
        Ok(Some(if with_session {
            format!("registered in {KEY}")
        } else {
            format!("removed from {KEY}")
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_is_written_once_and_removed() {
        let folder = assert_fs::TempDir::new().unwrap();
        let file = folder.path().join("autostart/serafind.desktop");
        let executable = Path::new("/opt/my apps/serafind");
        assert!(apply_file(&file, true, executable).unwrap().is_some());
        let content = std::fs::read_to_string(&file).unwrap();
        assert!(
            content.contains("Exec=/opt/my\\sapps/serafind start\n"),
            "{content}"
        );
        assert!(content.contains("X-GNOME-Autostart-enabled=true\n"));
        assert_eq!(
            apply_file(&file, true, executable).unwrap(),
            None,
            "already there"
        );
        assert!(apply_file(&file, false, executable).unwrap().is_some());
        assert!(!file.exists());
        assert_eq!(
            apply_file(&file, false, executable).unwrap(),
            None,
            "already gone"
        );
    }

    #[test]
    fn a_moved_executable_is_written_again() {
        let folder = assert_fs::TempDir::new().unwrap();
        let file = folder.path().join("serafind.desktop");
        apply_file(&file, true, Path::new("/old/serafind")).unwrap();
        assert!(
            apply_file(&file, true, Path::new("/new/serafind"))
                .unwrap()
                .is_some()
        );
        assert!(
            std::fs::read_to_string(&file)
                .unwrap()
                .contains("Exec=/new/serafind start")
        );
    }
}
