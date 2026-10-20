//! The characters that ship with the app (design §9.2). Each one is a folder
//! under `public/characters/` that the webview loads; this list is what the
//! menu bar's Character menu offers and what a saved choice is checked against.

use tauri::State;

use crate::settings::SettingsStore;

/// The character used on first run and whenever a saved choice is unknown.
pub const DEFAULT_ID: &str = "shiba";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BundledCharacter {
    /// Folder name under `public/characters/`.
    pub id: &'static str,
    /// Name shown in the menu.
    pub label: &'static str,
}

pub const BUNDLED: &[BundledCharacter] = &[BundledCharacter {
    id: DEFAULT_ID,
    label: "Shiba Inu",
}];

/// `id` if it's a bundled character, otherwise the default, so a stale or
/// hand-edited settings.json can never point the pet at a missing folder.
pub fn resolve(id: &str) -> &'static str {
    BUNDLED
        .iter()
        .find(|character| character.id == id)
        .map_or(DEFAULT_ID, |character| character.id)
}

/// The frontend asks for this at startup to know which sprite to load.
#[tauri::command]
pub fn current_character(settings: State<'_, SettingsStore>) -> String {
    resolve(&settings.get().character).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::path::Path;

    #[test]
    fn the_default_is_bundled() {
        assert!(BUNDLED.iter().any(|c| c.id == DEFAULT_ID));
    }

    #[test]
    fn ids_are_unique() {
        let ids: HashSet<_> = BUNDLED.iter().map(|c| c.id).collect();

        assert_eq!(ids.len(), BUNDLED.len());
    }

    #[test]
    fn resolve_keeps_a_bundled_id() {
        assert_eq!(resolve("shiba"), "shiba");
    }

    #[test]
    fn resolve_falls_back_to_the_default_for_unknown_ids() {
        assert_eq!(resolve("no-such-pet"), DEFAULT_ID);
        assert_eq!(resolve(""), DEFAULT_ID);
    }

    #[test]
    fn every_bundled_character_has_its_files() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../public/characters");

        for character in BUNDLED {
            let manifest = root.join(character.id).join("character.json");
            assert!(manifest.exists(), "{} is missing", manifest.display());
        }
    }
}
