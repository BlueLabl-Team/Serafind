//! Files only the user can read (0600 on Unix): the log and the usage counts tell what was
//! searched and opened.

use std::fs::File;
use std::io;
use std::path::Path;

/// Creates (or empties) `path`, readable and writable by the user only.
///
/// # Errors
/// When the file cannot be created.
pub fn create(path: &Path) -> io::Result<File> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        options.mode(0o600);
        let file = options.open(path)?;
        // A file made by an earlier version keeps its permissions: set them again.
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        Ok(file)
    }
    #[cfg(not(unix))]
    options.open(path)
}

/// Writes `content` to a file only the user can read.
///
/// # Errors
/// When the file cannot be written.
pub fn write(path: &Path, content: impl AsRef<[u8]>) -> io::Result<()> {
    io::Write::write_all(&mut create(path)?, content.as_ref())
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn only_the_user_can_read_them() {
        let folder = assert_fs::TempDir::new().unwrap();
        let path = folder.path().join("log");
        std::fs::write(&path, "old").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        super::write(&path, "new").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
