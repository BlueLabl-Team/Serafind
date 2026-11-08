//! Starting Serafind with the session (`[startup] with_session`): an XDG autostart file on
//! Linux, a `Run` registry value on Windows. Both run `serafind start`.

#[cfg(not(windows))]
use std::path::{Path, PathBuf};

#[cfg(not(windows))]
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
#[cfg(not(windows))]
#[must_use]
pub fn autostart_file() -> Option<PathBuf> {
    Some(
        dirs::config_dir()?
            .join("autostart")
            .join(format!("{APP_ID}.desktop")),
    )
}

/// Writes or removes the autostart file.
#[cfg(not(windows))]
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
#[cfg(not(windows))]
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
    use std::io;
    use std::path::Path;

    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{
        HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
    };

    const KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const VALUE: &str = "Serafind";

    /// Sets or deletes the `Run` value of the current user, through the registry API.
    pub fn apply(with_session: bool, executable: &Path) -> io::Result<Option<String>> {
        let wanted = format!("\"{}\" start", executable.display());
        let current = read()?;
        if with_session && current.as_deref() == Some(wanted.as_str()) || !with_session && current.is_none() {
            return Ok(None);
        }
        if with_session {
            write(&wanted)?;
            Ok(Some(format!(r"registered in HKCU\{KEY}")))
        } else {
            delete()?;
            Ok(Some(format!(r"removed from HKCU\{KEY}")))
        }
    }

    /// The text as a null-terminated UTF-16 string.
    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn check(status: u32) -> io::Result<()> {
        if status == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(io::Error::from_raw_os_error(
                i32::try_from(status).unwrap_or(i32::MAX),
            ))
        }
    }

    /// The current value; `None` when there is none.
    fn read() -> io::Result<Option<String>> {
        let (key, value) = (wide(KEY), wide(VALUE));
        let mut buffer = vec![0u16; 2048];
        let mut size = u32::try_from(buffer.len() * 2).unwrap_or(u32::MAX);
        // SAFETY: the key and value names are null-terminated; the buffer and its size in bytes
        // describe the same memory, and RRF_RT_REG_SZ makes Windows end the text with a null.
        #[allow(unsafe_code)]
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buffer.as_mut_ptr().cast(),
                &raw mut size,
            )
        };
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        check(status)?;
        let length = buffer.iter().position(|&unit| unit == 0).unwrap_or(buffer.len());
        Ok(Some(String::from_utf16_lossy(&buffer[..length])))
    }

    fn write(text: &str) -> io::Result<()> {
        let (key, value, data) = (wide(KEY), wide(VALUE), wide(text));
        let bytes = u32::try_from(data.len() * 2).map_err(io::Error::other)?;
        // SAFETY: null-terminated names; the data pointer and its size in bytes match.
        #[allow(unsafe_code)]
        let status = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                value.as_ptr(),
                REG_SZ,
                data.as_ptr().cast(),
                bytes,
            )
        };
        check(status)
    }

    fn delete() -> io::Result<()> {
        let (key, value) = (wide(KEY), wide(VALUE));
        // SAFETY: null-terminated names.
        #[allow(unsafe_code)]
        let status = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr()) };
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(());
        }
        check(status)
    }
}

#[cfg(all(test, not(windows)))]
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
