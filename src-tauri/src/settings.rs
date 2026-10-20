//! User settings, persisted as JSON in the app data folder (design §9.1).
//!
//! Only the keys the app uses so far live here; Phase 3 adds the rest. Unknown
//! keys in the file are ignored and missing ones take their default, so an
//! older or newer settings.json never stops the app from starting.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::characters::DEFAULT_ID;

const SETTINGS_FILE: &str = "settings.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Folder name of the active character under `characters/`.
    pub character: String,
    /// False while "Pause wandering" is on: the pet stays put (design §9.1).
    pub wander: bool,
    /// Whether finished runs and approvals may notify (design §8.3).
    pub notifications: bool,
    /// The Claude Code session the next message continues, if any.
    pub last_session_id: Option<String>,
    /// Folders passed again as `--add-dir` when that session is resumed.
    pub session_add_dirs: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            character: DEFAULT_ID.to_string(),
            wander: true,
            notifications: true,
            last_session_id: None,
            session_add_dirs: Vec::new(),
        }
    }
}

impl Settings {
    /// Reads settings from `path`. A missing file is normal (first run); an
    /// unreadable or corrupt one is reported and replaced by the defaults so
    /// the buddy still starts.
    pub fn load(path: &Path) -> Settings {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Settings::default(),
            Err(error) => {
                eprintln!("settings: cannot read {}: {error}", path.display());
                return Settings::default();
            }
        };
        serde_json::from_str(&text).unwrap_or_else(|error| {
            eprintln!("settings: ignoring corrupt {}: {error}", path.display());
            Settings::default()
        })
    }

    /// Writes settings to `path` via a temp file and rename, so a crash can
    /// never leave a half-written settings.json behind.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        let temp = path.with_extension("json.tmp");
        fs::write(&temp, json)?;
        fs::rename(&temp, path)
    }

    pub fn with_character(&self, id: &str) -> Settings {
        Settings {
            character: id.to_string(),
            ..self.clone()
        }
    }

    pub fn with_wander(&self, wander: bool) -> Settings {
        Settings {
            wander,
            ..self.clone()
        }
    }

    pub fn with_notifications(&self, notifications: bool) -> Settings {
        Settings {
            notifications,
            ..self.clone()
        }
    }

    /// Continues `session_id` (or starts fresh when `None`) with these
    /// folders on the next message.
    pub fn with_session(&self, session_id: Option<String>, add_dirs: Vec<String>) -> Settings {
        Settings {
            last_session_id: session_id,
            session_add_dirs: add_dirs,
            ..self.clone()
        }
    }
}

/// The live settings plus where they're saved; managed by Tauri so the menu
/// bar and commands share one copy.
pub struct SettingsStore {
    path: PathBuf,
    current: Mutex<Settings>,
}

impl SettingsStore {
    /// Loads `settings.json` from `dir` (the app data folder).
    pub fn open(dir: &Path) -> Self {
        let path = dir.join(SETTINGS_FILE);
        let current = Mutex::new(Settings::load(&path));
        SettingsStore { path, current }
    }

    pub fn get(&self) -> Settings {
        self.current.lock().unwrap().clone()
    }

    /// Applies `change` and saves. If saving fails the change still applies
    /// for this run and the error is logged, rather than losing the user's
    /// choice (or crashing) over a read-only folder.
    pub fn update(&self, change: impl FnOnce(&Settings) -> Settings) -> Settings {
        let mut current = self.current.lock().unwrap();
        let next = change(&current);
        if let Err(error) = next.save(&self.path) {
            eprintln!("settings: cannot save {}: {error}", self.path.display());
        }
        *current = next.clone();
        next
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings_path(dir: &tempfile::TempDir) -> PathBuf {
        dir.path().join(SETTINGS_FILE)
    }

    #[test]
    fn defaults_are_the_bundled_character_and_wandering_on() {
        let settings = Settings::default();

        assert_eq!(settings.character, DEFAULT_ID);
        assert!(settings.wander);
    }

    #[test]
    fn a_missing_file_gives_the_defaults() {
        let dir = tempfile::tempdir().unwrap();

        assert_eq!(Settings::load(&settings_path(&dir)), Settings::default());
    }

    #[test]
    fn a_corrupt_file_gives_the_defaults() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(settings_path(&dir), "{ not json").unwrap();

        assert_eq!(Settings::load(&settings_path(&dir)), Settings::default());
    }

    #[test]
    fn missing_keys_take_their_defaults() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(settings_path(&dir), r#"{ "wander": false }"#).unwrap();

        let settings = Settings::load(&settings_path(&dir));

        assert!(!settings.wander);
        assert_eq!(settings.character, DEFAULT_ID);
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            settings_path(&dir),
            r#"{ "character": "cat", "hotkey": "Cmd+Shift+Space" }"#,
        )
        .unwrap();

        assert_eq!(Settings::load(&settings_path(&dir)).character, "cat");
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let settings = Settings::default().with_character("cat").with_wander(false);

        settings.save(&settings_path(&dir)).unwrap();

        assert_eq!(Settings::load(&settings_path(&dir)), settings);
    }

    #[test]
    fn save_creates_missing_folders_and_leaves_no_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join(SETTINGS_FILE);

        Settings::default().save(&path).unwrap();

        assert!(path.exists());
        assert!(!path.with_extension("json.tmp").exists());
    }

    #[test]
    fn with_helpers_return_a_copy_and_leave_the_original_alone() {
        let original = Settings::default();

        let changed = original.with_character("cat").with_wander(false);

        assert_eq!(original, Settings::default());
        assert_eq!(changed.character, "cat");
        assert!(!changed.wander);
    }

    #[test]
    fn notifications_are_on_and_no_session_is_active_by_default() {
        let settings = Settings::default();

        assert!(settings.notifications);
        assert_eq!(settings.last_session_id, None);
        assert!(settings.session_add_dirs.is_empty());
    }

    #[test]
    fn the_session_and_notification_choices_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let settings = Settings::default()
            .with_notifications(false)
            .with_session(Some("abc".to_string()), vec!["/tmp/x".to_string()]);

        settings.save(&settings_path(&dir)).unwrap();
        let loaded = Settings::load(&settings_path(&dir));

        assert!(!loaded.notifications);
        assert_eq!(loaded.last_session_id.as_deref(), Some("abc"));
        assert_eq!(loaded.session_add_dirs, vec!["/tmp/x".to_string()]);
    }

    #[test]
    fn with_session_none_clears_the_conversation() {
        let active = Settings::default().with_session(Some("abc".to_string()), vec!["/x".into()]);

        let cleared = active.with_session(None, Vec::new());

        assert_eq!(cleared.last_session_id, None);
        assert!(cleared.session_add_dirs.is_empty());
    }

    #[test]
    fn the_store_persists_updates() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::open(dir.path());

        let updated = store.update(|s| s.with_wander(false));

        assert!(!updated.wander);
        assert!(!store.get().wander);
        assert!(!SettingsStore::open(dir.path()).get().wander);
    }

    #[test]
    fn the_store_keeps_the_change_when_saving_fails() {
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("blocker");
        fs::write(&blocker, "a file, not a folder").unwrap();
        // The settings folder would have to live inside a regular file.
        let store = SettingsStore::open(&blocker.join("inside"));

        let updated = store.update(|s| s.with_character("cat"));

        assert_eq!(updated.character, "cat");
        assert_eq!(store.get().character, "cat");
    }
}
