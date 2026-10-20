use std::cell::Cell;

use crate::platform::{Buttons, Mouse, Point};

/// A mouse whose cursor and buttons a test sets directly, one tick at a
/// time, to script a drag-and-throw sequence.
pub struct FakeMouse {
    cursor: Cell<Point>,
    buttons: Cell<Buttons>,
}

impl FakeMouse {
    pub fn new(cursor: Point, buttons: Buttons) -> Self {
        Self {
            cursor: Cell::new(cursor),
            buttons: Cell::new(buttons),
        }
    }

    pub fn set_cursor(&self, cursor: Point) {
        self.cursor.set(cursor);
    }

    pub fn set_buttons(&self, buttons: Buttons) {
        self.buttons.set(buttons);
    }
}

impl Default for FakeMouse {
    fn default() -> Self {
        Self::new(Point::default(), Buttons::default())
    }
}

impl Mouse for FakeMouse {
    fn cursor(&self) -> Point {
        self.cursor.get()
    }

    fn buttons(&self) -> Buttons {
        self.buttons.get()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_with_the_scripted_state() {
        let mouse = FakeMouse::new(
            Point { x: 10.0, y: 20.0 },
            Buttons {
                left: true,
                right: false,
            },
        );

        assert_eq!(mouse.cursor(), Point { x: 10.0, y: 20.0 });
        assert_eq!(
            mouse.buttons(),
            Buttons {
                left: true,
                right: false
            }
        );
    }

    #[test]
    fn set_cursor_and_set_buttons_update_what_is_reported_next() {
        let mouse = FakeMouse::default();

        mouse.set_cursor(Point { x: 5.0, y: 6.0 });
        mouse.set_buttons(Buttons {
            left: false,
            right: true,
        });

        assert_eq!(mouse.cursor(), Point { x: 5.0, y: 6.0 });
        assert_eq!(
            mouse.buttons(),
            Buttons {
                left: false,
                right: true
            }
        );
    }
}
