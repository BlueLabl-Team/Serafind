//! In-memory file index of Serafind.
//!
//! Will hold every path under the indexed roots (home directory by default, minus the excluded
//! folders), built by a parallel walk at start and kept up to date by watching the filesystem.

#[cfg(test)]
mod tests {
    use assert_fs::prelude::*;

    /// Wiring check for the filesystem test fixtures the index tests will rely on.
    #[test]
    fn temporary_trees_can_be_built() {
        let root = assert_fs::TempDir::new().unwrap();
        root.child("Documents/invoice-2026-03.pdf").touch().unwrap();
        root.child("node_modules/left-pad/index.js").touch().unwrap();

        assert!(root.child("Documents/invoice-2026-03.pdf").path().is_file());
        assert!(root.child("node_modules/left-pad").path().is_dir());
    }
}
