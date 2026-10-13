//! What the updater leaves behind. On Windows, Tauri's updater writes each installer it downloads
//! into a folder of its own in the temporary folder, named `<product>-<version>-updater-<random>`,
//! and never removes it (tauri-apps/plugins-workspace#2132), so the app removes those folders once
//! it runs again.

use std::fs;
use std::path::Path;

/// Removes the updater's folders for `product` from `temp`, leaving anything else alone, and says
/// how many it removed. A folder still in use stays until a later start.
pub fn remove_downloads(temp: &Path, product: &str) -> usize {
    let prefix = format!("{product}-");
    let Ok(entries) = fs::read_dir(temp) else {
        return 0;
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let is_download = name
            .to_str()
            .and_then(|name| name.strip_prefix(&prefix))
            .is_some_and(|rest| rest.contains("-updater-"));
        // The entry's own type, so that a link is never followed.
        let is_folder = entry.file_type().is_ok_and(|kind| kind.is_dir());
        if is_download && is_folder && fs::remove_dir_all(entry.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::remove_downloads;
    use std::fs;

    #[test]
    fn removes_only_the_updaters_folders_for_this_app() {
        let temp = tempfile::tempdir().unwrap();
        let download = temp
            .path()
            .join("Dashboard for Claude Code-0.4.0-updater-StW6Ih");
        fs::create_dir(&download).unwrap();
        fs::write(
            download.join("Dashboard for Claude Code-0.4.0-installer.exe"),
            b"MZ",
        )
        .unwrap();
        let kept = [
            "Dashboard for Claude Code-notes",
            "Another app-1.0.0-updater-AbC123",
            "Dashboard for Claude Codes-0.4.0-updater-AbC123",
        ];
        for name in kept {
            fs::create_dir(temp.path().join(name)).unwrap();
        }
        let file = temp
            .path()
            .join("Dashboard for Claude Code-0.4.0-updater-file");
        fs::write(&file, b"").unwrap();

        assert_eq!(
            remove_downloads(temp.path(), "Dashboard for Claude Code"),
            1
        );
        assert!(!download.exists());
        for name in kept {
            assert!(temp.path().join(name).exists(), "{name} stays");
        }
        assert!(file.exists(), "a file of that name stays");
    }

    #[test]
    fn a_missing_temporary_folder_removes_nothing() {
        let temp = tempfile::tempdir().unwrap();
        assert_eq!(
            remove_downloads(&temp.path().join("gone"), "Dashboard for Claude Code"),
            0
        );
    }
}
