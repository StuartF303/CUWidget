//! User settings persisted as JSON in the app config dir.
//!
//! Loading never fails: a missing, unreadable or corrupt file gives defaults,
//! and unknown fields (for example from a newer version) are ignored.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const FILE_NAME: &str = "settings.json";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Keep the widget above other windows. Off unless the user turns it on.
    pub always_on_top: bool,
}

/// Where the settings file lives inside the app config directory.
pub fn path_in(config_dir: &Path) -> PathBuf {
    config_dir.join(FILE_NAME)
}

/// Read settings from `path`; any problem yields the defaults.
pub fn load(path: &Path) -> Settings {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Write settings to `path`, creating the parent directory if needed. The file
/// is written to a sibling temp file first so a crash cannot leave it half-written.
pub fn save(path: &Path, settings: &Settings) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(settings).map_err(io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json)?;
    fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_not_always_on_top() {
        assert!(!Settings::default().always_on_top);
    }

    #[test]
    fn missing_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load(&path_in(dir.path())), Settings::default());
    }

    #[test]
    fn corrupt_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = path_in(dir.path());
        fs::write(&path, "{ not json").unwrap();
        assert_eq!(load(&path), Settings::default());
        fs::write(&path, r#"{"always_on_top": "yes"}"#).unwrap();
        assert_eq!(load(&path), Settings::default());
        fs::write(&path, [0xff, 0xfe, 0x00]).unwrap();
        assert_eq!(load(&path), Settings::default());
    }

    #[test]
    fn unreadable_path_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        // A directory where a file is expected cannot be read as text.
        assert_eq!(load(dir.path()), Settings::default());
    }

    #[test]
    fn round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = path_in(dir.path());
        let on = Settings { always_on_top: true };
        save(&path, &on).unwrap();
        assert_eq!(load(&path), on);
        save(&path, &Settings::default()).unwrap();
        assert_eq!(load(&path), Settings::default());
    }

    #[test]
    fn save_creates_missing_parent_directory() {
        let dir = tempfile::tempdir().unwrap();
        let path = path_in(&dir.path().join("nested").join("config"));
        save(&path, &Settings { always_on_top: true }).unwrap();
        assert!(load(&path).always_on_top);
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let path = path_in(dir.path());
        fs::write(&path, r#"{"always_on_top": true, "poll_scale": 2, "theme": {"x": 1}}"#).unwrap();
        assert_eq!(load(&path), Settings { always_on_top: true });
    }

    #[test]
    fn missing_fields_take_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = path_in(dir.path());
        fs::write(&path, "{}").unwrap();
        assert_eq!(load(&path), Settings::default());
    }
}
