//! How often each application was started from Serafind, so that the ones used most come first.
//! Only kept when `[behavior] learn_from_usage = true`. Stored in `usage.json`, next to the log.

use std::path::PathBuf;

use crate::search::Launches;

/// The launch counts and where they are saved.
#[derive(Debug, Default)]
pub struct Usage {
    file: Option<PathBuf>,
    launches: Launches,
}

impl Usage {
    /// Reads the counts saved in `file` (none when it is missing or unreadable).
    #[must_use]
    pub fn load(file: Option<PathBuf>) -> Self {
        let launches = file
            .as_ref()
            .and_then(|file| std::fs::read_to_string(file).ok())
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Self { file, launches }
    }

    /// The usual file: `~/.local/state/serafind/usage.json` on Linux.
    #[must_use]
    pub fn default_file() -> Option<PathBuf> {
        Some(
            dirs::state_dir()
                .or_else(dirs::data_local_dir)?
                .join("serafind")
                .join("usage.json"),
        )
    }

    /// The counts.
    #[must_use]
    pub fn launches(&self) -> &Launches {
        &self.launches
    }

    /// Counts one more launch of `app` and saves.
    ///
    /// # Errors
    /// When the file cannot be written.
    pub fn record(&mut self, app: &str) -> std::io::Result<()> {
        *self.launches.entry(app.to_owned()).or_insert(0) += 1;
        let Some(file) = &self.file else {
            return Ok(());
        };
        if let Some(folder) = file.parent() {
            std::fs::create_dir_all(folder)?;
        }
        let text = serde_json::to_string_pretty(&self.launches).map_err(std::io::Error::other)?;
        crate::private_file::write(file, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_are_saved_and_read_again() {
        let folder = assert_fs::TempDir::new().unwrap();
        let file = folder.path().join("state/usage.json");
        let mut usage = Usage::load(Some(file.clone()));
        assert!(usage.launches().is_empty());
        usage.record("firefox.desktop").unwrap();
        usage.record("firefox.desktop").unwrap();
        usage.record("org.gnome.Nautilus.desktop").unwrap();
        let again = Usage::load(Some(file));
        assert_eq!(again.launches().get("firefox.desktop"), Some(&2));
        assert_eq!(again.launches().get("org.gnome.Nautilus.desktop"), Some(&1));
    }

    #[test]
    fn a_broken_file_is_ignored() {
        let folder = assert_fs::TempDir::new().unwrap();
        let file = folder.path().join("usage.json");
        std::fs::write(&file, "not json").unwrap();
        assert!(Usage::load(Some(file)).launches().is_empty());
    }
}
