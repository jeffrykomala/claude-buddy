use crate::platform::{Display, Screens};

/// A fixed list of displays, set up once by a test.
pub struct FakeScreens {
    displays: Vec<Display>,
}

impl FakeScreens {
    pub fn new(displays: Vec<Display>) -> Self {
        Self { displays }
    }

    /// A single display with no Dock/menu-bar insets, for tests that don't
    /// care about the floor/ceiling distinction.
    pub fn single(frame: crate::platform::Rect) -> Self {
        Self::new(vec![Display {
            frame,
            visible_frame: frame,
        }])
    }
}

impl Screens for FakeScreens {
    fn work_areas(&self) -> Vec<Display> {
        self.displays.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::Rect;

    #[test]
    fn returns_the_scripted_displays() {
        let displays = vec![Display {
            frame: Rect {
                x: 0.0,
                y: 0.0,
                width: 1920.0,
                height: 1080.0,
            },
            visible_frame: Rect {
                x: 0.0,
                y: 0.0,
                width: 1920.0,
                height: 1055.0,
            },
        }];
        let screens = FakeScreens::new(displays.clone());

        assert_eq!(screens.work_areas(), displays);
    }

    #[test]
    fn single_uses_the_same_rect_for_frame_and_visible_frame() {
        let rect = Rect {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 600.0,
        };
        let screens = FakeScreens::single(rect);

        let areas = screens.work_areas();
        assert_eq!(areas.len(), 1);
        assert_eq!(areas[0].frame, rect);
        assert_eq!(areas[0].visible_frame, rect);
    }
}
