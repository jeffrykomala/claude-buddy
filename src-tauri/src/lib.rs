mod pet_window;
mod physics;
mod platform;

use platform::{PetWindowOs, Screens};
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            pet_window::grab,
            pet_window::click
        ])
        .setup(|app| {
            // No Dock icon, no app menu bar: the buddy lives in the menu
            // bar icon (added in 1.5) and the pet itself (design §11).
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let platform = platform::current();
            if let Some(window) = app.get_webview_window("main") {
                platform.setup(&window);
            }

            // Safe to read NSScreen here: `setup` runs on the main thread.
            let displays = platform.work_areas();
            app.manage(platform);
            app.manage(pet_window::PetWindow::new(displays));
            pet_window::spawn_tick_loop(app.handle().clone());

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
