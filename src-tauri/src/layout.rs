//! Where the pet window goes when a bubble opens (design §9.1, §11): it
//! grows toward the side of the pet that has room, and stays on the screen.
//! Pure geometry in the top-left-origin space of §6.1.

use serde::Serialize;

use crate::platform::{Point, Rect};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    /// The bubble is above the pet (the normal case).
    Above,
    /// The bubble is below the pet (the pet is near the top of the screen).
    Below,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BubbleSize {
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layout {
    /// The window's frame.
    pub window: Rect,
    /// Where the pet's top-left corner sits inside the window.
    pub pet_offset: Point,
    pub side: Side,
}

/// Lays out the window for a pet whose top-left corner is at `pet` and is
/// `pet_size` wide and tall, with an optional bubble, inside `area` (the
/// visible part of the pet's display).
pub fn compute(pet: Point, pet_size: f64, bubble: Option<BubbleSize>, area: Rect) -> Layout {
    let Some(bubble) = bubble else {
        return Layout {
            window: Rect {
                x: pet.x,
                y: pet.y,
                width: pet_size,
                height: pet_size,
            },
            pet_offset: Point { x: 0.0, y: 0.0 },
            side: Side::Above,
        };
    };

    let room_above = pet.y - area.top();
    let room_below = area.bottom() - (pet.y + pet_size);
    let side = if room_above >= bubble.height || room_above >= room_below {
        Side::Above
    } else {
        Side::Below
    };

    let width = bubble.width.max(pet_size);
    let height = bubble.height + pet_size;
    let x = clamp_start(
        pet.x + pet_size / 2.0 - width / 2.0,
        width,
        area.left(),
        area.right(),
    );
    let y = match side {
        Side::Above => pet.y - bubble.height,
        Side::Below => pet.y,
    };
    let y = clamp_start(y, height, area.top(), area.bottom());

    Layout {
        window: Rect {
            x,
            y,
            width,
            height,
        },
        pet_offset: Point {
            x: pet.x - x,
            y: pet.y - y,
        },
        side,
    }
}

/// Moves a span starting at `start` and `length` long so it lies within
/// `[low, high]`; if it is longer than the range it starts at `low`.
fn clamp_start(start: f64, length: f64, low: f64, high: f64) -> f64 {
    if length >= high - low {
        return low;
    }
    start.clamp(low, high - length)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PET: f64 = 128.0;
    const AREA: Rect = Rect {
        x: 0.0,
        y: 25.0,
        width: 1440.0,
        height: 800.0,
    };
    const BUBBLE: BubbleSize = BubbleSize {
        width: 340.0,
        height: 260.0,
    };

    #[test]
    fn without_a_bubble_the_window_is_just_the_pet() {
        let layout = compute(Point { x: 500.0, y: 600.0 }, PET, None, AREA);

        assert_eq!(layout.window.width, PET);
        assert_eq!(layout.window.height, PET);
        assert_eq!(layout.window.x, 500.0);
        assert_eq!(layout.window.y, 600.0);
        assert_eq!(layout.pet_offset, Point { x: 0.0, y: 0.0 });
    }

    #[test]
    fn the_bubble_goes_above_a_pet_on_the_floor() {
        let layout = compute(Point { x: 600.0, y: 697.0 }, PET, Some(BUBBLE), AREA);

        assert_eq!(layout.side, Side::Above);
        assert_eq!(layout.window.y, 697.0 - 260.0);
        assert_eq!(layout.window.height, 260.0 + PET);
        assert_eq!(layout.pet_offset.y, 260.0);
    }

    #[test]
    fn the_window_is_centered_on_the_pet_horizontally() {
        let layout = compute(Point { x: 600.0, y: 697.0 }, PET, Some(BUBBLE), AREA);

        assert_eq!(layout.window.x, 600.0 + 64.0 - 170.0);
        assert_eq!(layout.pet_offset.x, 106.0);
    }

    #[test]
    fn the_bubble_goes_below_a_pet_near_the_top() {
        let layout = compute(Point { x: 600.0, y: 40.0 }, PET, Some(BUBBLE), AREA);

        assert_eq!(layout.side, Side::Below);
        assert_eq!(layout.window.y, 40.0);
        assert_eq!(layout.pet_offset.y, 0.0);
    }

    #[test]
    fn a_pet_at_the_left_edge_keeps_the_window_on_screen() {
        let layout = compute(Point { x: 0.0, y: 697.0 }, PET, Some(BUBBLE), AREA);

        assert_eq!(layout.window.x, 0.0);
        assert_eq!(layout.pet_offset.x, 0.0);
    }

    #[test]
    fn a_pet_at_the_right_edge_keeps_the_window_on_screen() {
        let layout = compute(
            Point {
                x: 1312.0,
                y: 697.0,
            },
            PET,
            Some(BUBBLE),
            AREA,
        );

        assert_eq!(layout.window.right(), 1440.0);
        assert_eq!(layout.pet_offset.x, 1312.0 - (1440.0 - 340.0));
    }

    #[test]
    fn the_pet_is_always_inside_the_window() {
        for (x, y) in [(0.0, 25.0), (1312.0, 25.0), (700.0, 400.0), (1312.0, 697.0)] {
            let layout = compute(Point { x, y }, PET, Some(BUBBLE), AREA);

            assert!(layout.pet_offset.x >= 0.0);
            assert!(layout.pet_offset.x + PET <= layout.window.width + 0.001);
            assert!(layout.pet_offset.y >= 0.0);
            assert!(layout.pet_offset.y + PET <= layout.window.height + 0.001);
        }
    }

    #[test]
    fn a_bubble_taller_than_the_screen_starts_at_the_top() {
        let small = Rect {
            x: 0.0,
            y: 0.0,
            width: 1000.0,
            height: 300.0,
        };

        let layout = compute(Point { x: 400.0, y: 172.0 }, PET, Some(BUBBLE), small);

        assert_eq!(layout.window.y, 0.0);
    }

    #[test]
    fn a_narrow_screen_pins_the_window_to_the_left() {
        let narrow = Rect {
            x: 10.0,
            y: 0.0,
            width: 300.0,
            height: 900.0,
        };

        let layout = compute(Point { x: 100.0, y: 700.0 }, PET, Some(BUBBLE), narrow);

        assert_eq!(layout.window.x, 10.0);
    }
}
