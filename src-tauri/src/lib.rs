mod activity;
mod buddy;
mod characters;
mod claude_runner;
mod conversations;
#[cfg(test)]
mod e2e_tests;
mod layout;
mod menubar;
mod notify;
mod perm_broker;
mod perm_server;
mod pet_window;
mod physics;
mod platform;
mod settings;
mod slash;
mod stop;
mod stream_parser;
mod ui_event;

use conversations::ConversationStore;
use platform::{PetWindowOs, Screens};
use settings::SettingsStore;
use tauri::{Emitter, Manager, RunEvent, WindowEvent};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

/// The global hotkey that opens the input bubble (design §9.1).
const HOTKEY_MODIFIERS: Modifiers = Modifiers::SUPER.union(Modifiers::SHIFT);
const HOTKEY_CODE: Code = Code::Space;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        buddy::talk(app);
                    }
                })
                .build(),
        )
        .on_window_event(|window, event| {
            // Clicking a notification activates the app: reopen the reply.
            if let WindowEvent::Focused(true) = event {
                let _ = window.app_handle().emit(buddy::REOPEN_EVENT, ());
            }
        })
        .invoke_handler(tauri::generate_handler![
            pet_window::grab,
            pet_window::click,
            pet_window::set_bubble,
            pet_window::set_paused,
            pet_window::take_focus,
            pet_window::release_focus,
            characters::current_character,
            buddy::send_message,
            buddy::answer_approval,
            buddy::cancel_run,
            buddy::set_minimized,
            buddy::start_new_conversation,
            buddy::resume,
            buddy::list_recent
        ])
        .setup(|app| {
            // No Dock icon, no app menu bar: the buddy lives in the menu
            // bar icon and the pet itself (design §11).
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let platform = platform::current();
            if let Some(window) = app.get_webview_window("main") {
                platform.setup(&window);
            }

            // Safe to read NSScreen here: `setup` runs on the main thread.
            let displays = platform.work_areas();
            let data_dir = app.path().app_data_dir()?;
            let settings = SettingsStore::open(&data_dir);
            let pet = pet_window::PetWindow::new(displays);
            pet.set_wander(settings.get().wander);
            app.manage(platform);
            app.manage(settings);
            app.manage(ConversationStore::open(&data_dir));
            app.manage(pet);
            app.manage(buddy::Buddy::start(app.handle()));
            menubar::install(app.handle())?;
            register_hotkey(app.handle());
            pet_window::spawn_tick_loop(app.handle().clone());

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app, event| match event {
        // Quitting must not leave `claude` running (design §13): stop it
        // first, then let the app exit.
        RunEvent::ExitRequested { api, .. } => {
            if let Some(buddy) = app.try_state::<buddy::Buddy>()
                && !buddy.is_quitting()
                && buddy.is_running()
            {
                api.prevent_exit();
                buddy::quit(app);
            }
        }
        RunEvent::Exit => {
            if let Some(buddy) = app.try_state::<buddy::Buddy>() {
                buddy.shutdown();
            }
        }
        _ => {}
    });
}

/// ⌘⇧Space opens the input bubble. If another app already owns the hotkey
/// the buddy still works from the pet and the menu bar.
fn register_hotkey(app: &tauri::AppHandle) {
    let shortcut = Shortcut::new(Some(HOTKEY_MODIFIERS), HOTKEY_CODE);
    if let Err(error) = app.global_shortcut().register(shortcut) {
        eprintln!("hotkey: cannot register ⌘⇧Space: {error}");
    }
}
