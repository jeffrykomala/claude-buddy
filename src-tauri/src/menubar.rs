//! The menu bar icon and its menu (design §9.1): Talk, New conversation,
//! Recent ▸, Pause wandering, Character ▸, Notifications and Quit. The buddy
//! has no Dock icon or app menu, so this is where its settings live.

use tauri::image::Image;
use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::buddy;
use crate::characters::{self, BUNDLED};
use crate::conversations::{self, ConversationStore};
use crate::pet_window::PetWindow;
use crate::settings::SettingsStore;

/// Emitted with the new character id when the user picks one, so the webview
/// can swap the sprite.
pub const CHARACTER_EVENT: &str = "pet://character";

const TALK_ID: &str = "talk";
const NEW_ID: &str = "new_conversation";
const NOTIFICATIONS_ID: &str = "notifications";
const PAUSE_ID: &str = "pause_wandering";
const QUIT_ID: &str = "quit";
const CHARACTER_ID_PREFIX: &str = "character:";
const RECENT_ID_PREFIX: &str = "recent:";
const NO_RECENT_ID: &str = "no_recent";

#[derive(Debug, PartialEq, Eq)]
enum MenuAction {
    Talk,
    NewConversation,
    Resume(String),
    TogglePause,
    ToggleNotifications,
    SelectCharacter(String),
    Quit,
}

/// The Recent ▸ submenu, kept so it can be rebuilt as conversations change.
struct RecentMenu(Submenu<Wry>);

fn character_menu_id(id: &str) -> String {
    format!("{CHARACTER_ID_PREFIX}{id}")
}

fn recent_menu_id(id: &str) -> String {
    format!("{RECENT_ID_PREFIX}{id}")
}

fn parse_menu_id(id: &str) -> Option<MenuAction> {
    match id {
        TALK_ID => Some(MenuAction::Talk),
        NEW_ID => Some(MenuAction::NewConversation),
        NOTIFICATIONS_ID => Some(MenuAction::ToggleNotifications),
        PAUSE_ID => Some(MenuAction::TogglePause),
        QUIT_ID => Some(MenuAction::Quit),
        _ => id
            .strip_prefix(CHARACTER_ID_PREFIX)
            .filter(|name| BUNDLED.iter().any(|c| c.id == *name))
            .map(|name| MenuAction::SelectCharacter(name.to_string()))
            .or_else(|| {
                id.strip_prefix(RECENT_ID_PREFIX)
                    .filter(|session| !session.is_empty())
                    .map(|session| MenuAction::Resume(session.to_string()))
            }),
    }
}

/// Rebuilds Recent ▸ from the saved conversations. Call after any change to
/// the list; does nothing before the menu is installed.
pub fn refresh_recent(app: &AppHandle) {
    let Some(menu) = app.try_state::<RecentMenu>() else {
        return;
    };
    let Some(store) = app.try_state::<ConversationStore>() else {
        return;
    };
    let submenu = &menu.0;
    if let Ok(old_items) = submenu.items() {
        for item in old_items {
            log_on_error(submenu.remove(&item));
        }
    }
    let now = conversations::now_secs();
    let entries = store.list();
    if entries.is_empty() {
        let none = MenuItem::with_id(
            app,
            NO_RECENT_ID,
            "No recent conversations",
            false,
            None::<&str>,
        );
        log_on_error(none.and_then(|item| submenu.append(&item)));
        return;
    }
    for entry in entries {
        let label = format!(
            "{} ({})",
            entry.title,
            conversations::relative_time(now, entry.last_used)
        );
        let item = MenuItem::with_id(app, recent_menu_id(&entry.id), label, true, None::<&str>);
        log_on_error(item.and_then(|item| submenu.append(&item)));
    }
}

/// Adds the menu bar icon. Call from `setup`, after `SettingsStore` and
/// `PetWindow` are managed.
pub fn install(app: &AppHandle) -> tauri::Result<()> {
    let settings = app.state::<SettingsStore>().get();
    let active = characters::resolve(&settings.character);

    let pause = CheckMenuItem::with_id(
        app,
        PAUSE_ID,
        "Pause wandering",
        true,
        !settings.wander,
        None::<&str>,
    )?;
    let character_items = BUNDLED
        .iter()
        .map(|c| {
            CheckMenuItem::with_id(
                app,
                character_menu_id(c.id),
                c.label,
                true,
                c.id == active,
                None::<&str>,
            )
        })
        .collect::<tauri::Result<Vec<_>>>()?;
    let item_refs: Vec<&dyn IsMenuItem<Wry>> = character_items
        .iter()
        .map(|item| item as &dyn IsMenuItem<Wry>)
        .collect();
    let character_menu = Submenu::with_items(app, "Character", true, &item_refs)?;
    let quit = MenuItem::with_id(
        app,
        QUIT_ID,
        "Quit Desktop Buddy",
        true,
        Some("CmdOrCtrl+Q"),
    )?;
    let talk = MenuItem::with_id(app, TALK_ID, "Talk", true, Some("CmdOrCtrl+Shift+Space"))?;
    let new_conversation = MenuItem::with_id(app, NEW_ID, "New conversation", true, None::<&str>)?;
    let recent = Submenu::with_id(app, "recent", "Recent", true)?;
    let notifications = CheckMenuItem::with_id(
        app,
        NOTIFICATIONS_ID,
        "Notifications",
        true,
        settings.notifications,
        None::<&str>,
    )?;
    let separator = PredefinedMenuItem::separator(app)?;
    let separator_two = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(
        app,
        &[
            &talk,
            &new_conversation,
            &recent,
            &separator,
            &pause,
            &character_menu,
            &notifications,
            &separator_two,
            &quit,
        ],
    )?;
    app.manage(RecentMenu(recent));
    refresh_recent(app);

    TrayIconBuilder::with_id("buddy")
        .icon(Image::from_bytes(include_bytes!("../icons/tray.png"))?)
        .icon_as_template(true)
        .tooltip("Desktop Buddy")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(move |app, event| {
            if let Some(action) = parse_menu_id(event.id().as_ref()) {
                handle(app, action, &pause, &notifications, &character_items);
            }
        })
        .build(app)?;
    Ok(())
}

fn handle(
    app: &AppHandle,
    action: MenuAction,
    pause: &CheckMenuItem<Wry>,
    notifications: &CheckMenuItem<Wry>,
    character_items: &[CheckMenuItem<Wry>],
) {
    match action {
        MenuAction::Talk => buddy::talk(app),
        MenuAction::NewConversation => buddy::new_conversation(app),
        MenuAction::Resume(id) => log_on_error(buddy::resume_conversation(app, &id)),
        MenuAction::ToggleNotifications => {
            let enabled = !app.state::<SettingsStore>().get().notifications;
            app.state::<SettingsStore>()
                .update(|settings| settings.with_notifications(enabled));
            log_on_error(notifications.set_checked(enabled));
        }
        MenuAction::TogglePause => {
            // Decide from our own state, not the item's: the OS may already
            // have flipped the check mark by the time this runs.
            let wander = !app.state::<SettingsStore>().get().wander;
            set_wander(app, wander);
            log_on_error(pause.set_checked(!wander));
        }
        MenuAction::SelectCharacter(id) => {
            app.state::<SettingsStore>()
                .update(|settings| settings.with_character(&id));
            for item in character_items {
                let is_chosen = item.id().as_ref() == character_menu_id(&id);
                log_on_error(item.set_checked(is_chosen));
            }
            log_on_error(app.emit(CHARACTER_EVENT, id));
        }
        MenuAction::Quit => buddy::quit(app),
    }
}

fn set_wander(app: &AppHandle, wander: bool) {
    app.state::<SettingsStore>()
        .update(|settings| settings.with_wander(wander));
    app.state::<PetWindow>().set_wander(wander);
}

fn log_on_error<T, E: std::fmt::Display>(result: Result<T, E>) {
    if let Err(error) = result {
        eprintln!("menubar: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_the_fixed_items() {
        assert_eq!(
            parse_menu_id("pause_wandering"),
            Some(MenuAction::TogglePause)
        );
        assert_eq!(parse_menu_id("quit"), Some(MenuAction::Quit));
    }

    #[test]
    fn recognises_every_bundled_character() {
        for character in BUNDLED {
            assert_eq!(
                parse_menu_id(&character_menu_id(character.id)),
                Some(MenuAction::SelectCharacter(character.id.to_string()))
            );
        }
    }

    #[test]
    fn recognises_the_conversation_items() {
        assert_eq!(parse_menu_id("talk"), Some(MenuAction::Talk));
        assert_eq!(
            parse_menu_id("new_conversation"),
            Some(MenuAction::NewConversation)
        );
        assert_eq!(
            parse_menu_id("notifications"),
            Some(MenuAction::ToggleNotifications)
        );
    }

    #[test]
    fn a_recent_item_carries_its_session_id() {
        assert_eq!(
            parse_menu_id(&recent_menu_id("550e8400")),
            Some(MenuAction::Resume("550e8400".to_string()))
        );
    }

    #[test]
    fn an_empty_recent_id_and_the_placeholder_do_nothing() {
        assert_eq!(parse_menu_id("recent:"), None);
        assert_eq!(parse_menu_id("no_recent"), None);
    }

    #[test]
    fn rejects_unknown_ids_and_unbundled_characters() {
        assert_eq!(parse_menu_id(""), None);
        assert_eq!(parse_menu_id("something-else"), None);
        assert_eq!(parse_menu_id("character:no-such-pet"), None);
        assert_eq!(parse_menu_id("character:"), None);
    }

    #[test]
    fn the_tray_icon_is_a_valid_png() {
        let icon = Image::from_bytes(include_bytes!("../icons/tray.png"));

        let icon = icon.expect("tray.png should decode");
        assert!(icon.width() > 0 && icon.height() > 0);
    }
}
