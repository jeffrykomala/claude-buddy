use objc2::MainThreadMarker;
use objc2_app_kit::NSScreen;
use objc2_foundation::NSRect;

use crate::platform::{Display, Rect, Screens};

/// Converts an `NSScreen` rectangle (bottom-left origin, `y` grows upward)
/// into the app's top-left-origin, `y`-grows-downward space (design §6.1).
/// `x` is unchanged; only `y` needs flipping, relative to the main
/// display's height.
pub fn flip_rect(rect: Rect, main_height: f64) -> Rect {
    Rect {
        x: rect.x,
        y: main_height - (rect.y + rect.height),
        ..rect
    }
}

fn ns_rect_to_rect(rect: NSRect) -> Rect {
    Rect {
        x: rect.origin.x,
        y: rect.origin.y,
        width: rect.size.width,
        height: rect.size.height,
    }
}

/// Reads every connected display's frame and visible area from `NSScreen`.
#[derive(Debug, Default)]
pub struct MacScreens;

impl MacScreens {
    pub fn new() -> Self {
        Self
    }
}

impl Screens for MacScreens {
    fn work_areas(&self) -> Vec<Display> {
        // NSScreen can only be read from the main thread. Callers that need
        // this off the main thread (the physics tick) must hop back onto it
        // first; returning an empty list here is a safe no-op rather than a
        // panic.
        let Some(mtm) = MainThreadMarker::new() else {
            return Vec::new();
        };

        let main_height = NSScreen::mainScreen(mtm)
            .map(|screen| screen.frame().size.height)
            .unwrap_or(0.0);

        NSScreen::screens(mtm)
            .iter()
            .map(|screen| Display {
                frame: flip_rect(ns_rect_to_rect(screen.frame()), main_height),
                visible_frame: flip_rect(ns_rect_to_rect(screen.visibleFrame()), main_height),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn the_main_screen_itself_flips_to_a_top_left_origin() {
        let main = rect(0.0, 0.0, 1920.0, 1080.0);

        assert_eq!(flip_rect(main, 1080.0), rect(0.0, 0.0, 1920.0, 1080.0));
    }

    #[test]
    fn a_secondary_display_above_the_main_one_gets_a_negative_top() {
        // In NSScreen space the secondary's bottom edge (y = 1080) sits on
        // the main display's top edge.
        let secondary = rect(0.0, 1080.0, 1920.0, 1200.0);

        let flipped = flip_rect(secondary, 1080.0);

        // Its top-left-space bottom (y + height) should touch the main
        // display's top (y = 0), and its top should be above that (negative).
        assert_eq!(flipped.y, -1200.0);
        assert_eq!(flipped.bottom(), 0.0);
    }

    #[test]
    fn a_secondary_display_to_the_left_keeps_its_vertical_alignment() {
        // Same height as main, placed directly to the left in NSScreen space.
        let secondary = rect(-1920.0, 0.0, 1920.0, 1080.0);

        let flipped = flip_rect(secondary, 1080.0);

        assert_eq!(flipped.x, -1920.0);
        assert_eq!(flipped.y, 0.0);
        assert_eq!(flipped.bottom(), 1080.0);
    }

    #[test]
    fn a_secondary_display_above_and_to_the_left_flips_both_independently() {
        let secondary = rect(-1920.0, 1080.0, 1920.0, 1080.0);

        let flipped = flip_rect(secondary, 1080.0);

        assert_eq!(flipped.x, -1920.0);
        assert_eq!(flipped.y, -1080.0);
    }

    #[test]
    fn width_and_height_are_never_changed() {
        let secondary = rect(100.0, 200.0, 640.0, 480.0);

        let flipped = flip_rect(secondary, 1080.0);

        assert_eq!(flipped.width, 640.0);
        assert_eq!(flipped.height, 480.0);
    }
}
