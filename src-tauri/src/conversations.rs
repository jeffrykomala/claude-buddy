//! The list of recent conversations (design §8.6, §9.1). A conversation is
//! just a Claude Code session id, so this file only remembers which ids the
//! buddy has used, with a title to show in **Recent** and the folders to
//! pass again on resume.
//!
//! Everything here is forgiving: a missing or corrupt file is an empty list,
//! and a folder that can't be written only means no history, never a crash.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

const CONVERSATIONS_FILE: &str = "conversations.json";
/// The oldest conversations beyond this many drop off the list.
pub const MAX_CONVERSATIONS: usize = 20;
const MAX_TITLE_CHARS: usize = 40;
const UNTITLED: &str = "New conversation";

const MINUTE: u64 = 60;
const HOUR: u64 = 60 * MINUTE;
const DAY: u64 = 24 * HOUR;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conversation {
    /// The Claude Code session id (`--resume <id>`).
    pub id: String,
    pub title: String,
    /// Seconds since the Unix epoch.
    pub last_used: u64,
    pub model: Option<String>,
    pub add_dirs: Vec<String>,
}

/// The first prompt with whitespace collapsed, cut to 40 characters.
pub fn title_from(prompt: &str) -> String {
    let collapsed = prompt.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return UNTITLED.to_string();
    }
    if collapsed.chars().count() <= MAX_TITLE_CHARS {
        return collapsed;
    }
    let kept: String = collapsed.chars().take(MAX_TITLE_CHARS - 1).collect();
    format!("{kept}…")
}

/// "just now", "5 min ago", "2 h ago", "3 d ago".
pub fn relative_time(now: u64, then: u64) -> String {
    let elapsed = now.saturating_sub(then);
    match elapsed {
        e if e < MINUTE => "just now".to_string(),
        e if e < HOUR => format!("{} min ago", e / MINUTE),
        e if e < DAY => format!("{} h ago", e / HOUR),
        e => format!("{} d ago", e / DAY),
    }
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

fn load(path: &Path) -> Vec<Conversation> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Vec::new(),
        Err(error) => {
            eprintln!("conversations: cannot read {}: {error}", path.display());
            return Vec::new();
        }
    };
    let mut list: Vec<Conversation> = serde_json::from_str(&text).unwrap_or_else(|error| {
        eprintln!(
            "conversations: ignoring corrupt {}: {error}",
            path.display()
        );
        Vec::new()
    });
    list.sort_by_key(|c| std::cmp::Reverse(c.last_used));
    list.truncate(MAX_CONVERSATIONS);
    list
}

fn save(path: &Path, list: &[Conversation]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_string_pretty(list).map_err(io::Error::other)?;
    let temp = path.with_extension("json.tmp");
    fs::write(&temp, json)?;
    fs::rename(&temp, path)
}

/// The recent conversations plus where they're saved; managed by Tauri.
pub struct ConversationStore {
    path: PathBuf,
    list: Mutex<Vec<Conversation>>,
}

impl ConversationStore {
    /// Loads `conversations.json` from `dir` (the app data folder).
    pub fn open(dir: &Path) -> Self {
        let path = dir.join(CONVERSATIONS_FILE);
        let list = Mutex::new(load(&path));
        ConversationStore { path, list }
    }

    /// Newest first, at most [`MAX_CONVERSATIONS`].
    pub fn list(&self) -> Vec<Conversation> {
        self.list.lock().unwrap().clone()
    }

    pub fn get(&self, id: &str) -> Option<Conversation> {
        self.list
            .lock()
            .unwrap()
            .iter()
            .find(|c| c.id == id)
            .cloned()
    }

    /// A session started. A new id is added at the top, titled from its
    /// first prompt; a resumed one just moves to the top.
    pub fn record_started(
        &self,
        id: &str,
        first_prompt: &str,
        model: Option<&str>,
        add_dirs: &[String],
        now: u64,
    ) {
        self.change(|list| {
            let existing = list
                .iter()
                .position(|c| c.id == id)
                .map(|at| list.remove(at));
            let entry = match existing {
                Some(old) => refreshed(old, model, now),
                None => Conversation {
                    id: id.to_string(),
                    title: title_from(first_prompt),
                    last_used: now,
                    model: model.map(str::to_string),
                    add_dirs: add_dirs.to_vec(),
                },
            };
            list.insert(0, entry);
        });
    }

    /// A run finished: refresh the time and model, and move it to the top.
    pub fn touch(&self, id: &str, model: Option<&str>, now: u64) {
        self.change(|list| {
            if let Some(at) = list.iter().position(|c| c.id == id) {
                let old = list.remove(at);
                list.insert(0, refreshed(old, model, now));
            }
        });
    }

    pub fn remove(&self, id: &str) {
        self.change(|list| list.retain(|c| c.id != id));
    }

    /// Applies `edit`, trims to the limit and saves. A failed save is logged:
    /// the in-memory list still works for this run.
    fn change(&self, edit: impl FnOnce(&mut Vec<Conversation>)) {
        let mut list = self.list.lock().unwrap();
        edit(&mut list);
        list.truncate(MAX_CONVERSATIONS);
        if let Err(error) = save(&self.path, &list) {
            eprintln!(
                "conversations: cannot save {}: {error}",
                self.path.display()
            );
        }
    }
}

/// `old` used again at `now`, keeping its previous model if none is given.
fn refreshed(old: Conversation, model: Option<&str>, now: u64) -> Conversation {
    Conversation {
        last_used: now,
        model: model.map(str::to_string).or_else(|| old.model.clone()),
        ..old
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, ConversationStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = ConversationStore::open(dir.path());
        (dir, store)
    }

    #[test]
    fn title_collapses_whitespace() {
        assert_eq!(title_from("  hello \n\n  big\tworld "), "hello big world");
    }

    #[test]
    fn title_is_cut_to_forty_characters() {
        let title = title_from(&"a".repeat(100));

        assert_eq!(title.chars().count(), 40);
        assert!(title.ends_with('…'));
    }

    #[test]
    fn a_title_of_exactly_forty_characters_is_kept() {
        let text = "b".repeat(40);

        assert_eq!(title_from(&text), text);
    }

    #[test]
    fn an_empty_prompt_gets_a_placeholder_title() {
        assert_eq!(title_from("  \n "), "New conversation");
    }

    #[test]
    fn title_cuts_on_characters_not_bytes() {
        let title = title_from(&"é".repeat(60));

        assert_eq!(title.chars().count(), 40);
    }

    #[test]
    fn relative_times_pick_the_largest_unit() {
        assert_eq!(relative_time(1000, 1000), "just now");
        assert_eq!(relative_time(1000, 1000 - 59), "just now");
        assert_eq!(relative_time(10_000, 10_000 - 5 * 60), "5 min ago");
        assert_eq!(relative_time(100_000, 100_000 - 2 * 3600 - 50), "2 h ago");
        assert_eq!(relative_time(1_000_000, 1_000_000 - 3 * 86_400), "3 d ago");
    }

    #[test]
    fn relative_time_never_goes_negative() {
        assert_eq!(relative_time(10, 500), "just now");
    }

    #[test]
    fn a_new_session_is_recorded_at_the_top() {
        let (_dir, store) = store();

        store.record_started("a", "first thing", Some("opus"), &[], 100);
        store.record_started("b", "second thing", None, &["/x".to_string()], 200);

        let list = store.list();
        assert_eq!(list[0].id, "b");
        assert_eq!(list[0].title, "second thing");
        assert_eq!(list[0].add_dirs, vec!["/x".to_string()]);
        assert_eq!(list[1].id, "a");
        assert_eq!(list[1].model.as_deref(), Some("opus"));
    }

    #[test]
    fn a_resumed_session_keeps_its_title_and_moves_up() {
        let (_dir, store) = store();
        store.record_started("a", "original title", Some("opus"), &[], 100);
        store.record_started("b", "other", None, &[], 200);

        store.record_started("a", "a later prompt", None, &[], 300);

        let list = store.list();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, "a");
        assert_eq!(list[0].title, "original title");
        assert_eq!(list[0].last_used, 300);
        assert_eq!(list[0].model.as_deref(), Some("opus"));
    }

    #[test]
    fn touch_refreshes_time_and_model_and_moves_to_the_top() {
        let (_dir, store) = store();
        store.record_started("a", "one", Some("old"), &[], 100);
        store.record_started("b", "two", None, &[], 200);

        store.touch("a", Some("new"), 500);

        let list = store.list();
        assert_eq!(list[0].id, "a");
        assert_eq!(list[0].last_used, 500);
        assert_eq!(list[0].model.as_deref(), Some("new"));
    }

    #[test]
    fn touching_an_unknown_id_does_nothing() {
        let (_dir, store) = store();

        store.touch("ghost", None, 1);

        assert!(store.list().is_empty());
    }

    #[test]
    fn the_list_is_trimmed_to_twenty_dropping_the_oldest() {
        let (_dir, store) = store();

        for n in 0..25u64 {
            store.record_started(&format!("id{n}"), "p", None, &[], n);
        }

        let list = store.list();
        assert_eq!(list.len(), MAX_CONVERSATIONS);
        assert_eq!(list[0].id, "id24");
        assert_eq!(list.last().unwrap().id, "id5");
    }

    #[test]
    fn remove_deletes_an_entry() {
        let (_dir, store) = store();
        store.record_started("a", "one", None, &[], 1);
        store.record_started("b", "two", None, &[], 2);

        store.remove("a");

        assert!(store.get("a").is_none());
        assert!(store.get("b").is_some());
    }

    #[test]
    fn changes_survive_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        ConversationStore::open(dir.path()).record_started("a", "kept", None, &[], 7);

        let reopened = ConversationStore::open(dir.path());

        assert_eq!(reopened.get("a").unwrap().title, "kept");
    }

    #[test]
    fn a_corrupt_file_is_an_empty_list_that_can_be_rewritten() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(CONVERSATIONS_FILE), "{ not json").unwrap();

        let store = ConversationStore::open(dir.path());
        assert!(store.list().is_empty());
        store.record_started("a", "fresh", None, &[], 1);

        assert_eq!(ConversationStore::open(dir.path()).list().len(), 1);
    }

    #[test]
    fn an_unwritable_folder_still_works_in_memory() {
        let dir = tempfile::tempdir().unwrap();
        // A file where the folder should be: every save fails.
        let blocker = dir.path().join("blocked");
        fs::write(&blocker, "x").unwrap();
        let store = ConversationStore::open(&blocker);

        store.record_started("a", "still here", None, &[], 1);

        assert_eq!(store.list().len(), 1);
    }
}
