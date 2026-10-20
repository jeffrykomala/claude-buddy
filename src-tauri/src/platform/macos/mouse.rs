use objc2_core_graphics::{CGEvent, CGEventSource, CGEventSourceStateID, CGMouseButton};

use crate::platform::{Buttons, Mouse, Point};

/// Reads the global cursor location and button state via `CGEvent`. No
/// Accessibility permission is needed for either (design §6.5).
#[derive(Debug, Default)]
pub struct MacMouse;

impl MacMouse {
    pub fn new() -> Self {
        Self
    }
}

impl Mouse for MacMouse {
    fn cursor(&self) -> Point {
        let location = CGEvent::new(None)
            .map(|event| CGEvent::location(Some(&event)))
            .unwrap_or_default();
        Point {
            x: location.x,
            y: location.y,
        }
    }

    fn buttons(&self) -> Buttons {
        Buttons {
            left: CGEventSource::button_state(
                CGEventSourceStateID::CombinedSessionState,
                CGMouseButton::Left,
            ),
            right: CGEventSource::button_state(
                CGEventSourceStateID::CombinedSessionState,
                CGMouseButton::Right,
            ),
        }
    }
}
