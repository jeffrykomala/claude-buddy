use objc2_app_kit::{NSFloatingWindowLevel, NSWindow, NSWindowCollectionBehavior};

use crate::platform::PetWindowOs;

use super::MacosPlatform;

impl PetWindowOs for MacosPlatform {
    /// Makes the pet window follow the user everywhere: every Space
    /// (`CanJoinAllSpaces`), staying put during Mission Control
    /// (`Stationary`), and sitting above full-screen apps
    /// (`FullScreenAuxiliary`) — plus a floating window level so it stays
    /// above normal windows (design §11).
    ///
    /// `ActivationPolicy::Accessory` (no Dock icon) is set separately via
    /// `tauri::App::set_activation_policy`, since Tauri exposes that
    /// directly.
    fn setup(&self, window: &tauri::WebviewWindow) {
        let Ok(ptr) = window.ns_window() else {
            return;
        };
        if ptr.is_null() {
            return;
        }

        // SAFETY: `ns_window()` returns a valid, already-retained `NSWindow*`
        // owned by the webview for as long as `window` is alive; we only
        // borrow it here to call two setters.
        let ns_window = unsafe { &*ptr.cast::<NSWindow>() };

        ns_window.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::Stationary
                | NSWindowCollectionBehavior::FullScreenAuxiliary,
        );
        ns_window.setLevel(NSFloatingWindowLevel);
    }
}
