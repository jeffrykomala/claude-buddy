//! Grab and throw (design §6.5). Pure logic only: `pet_window` polls the
//! real mouse (`platform::Mouse`) each tick and drives this state machine;
//! it never calls the OS itself.

use std::collections::VecDeque;

use crate::physics::constants::*;
use crate::platform::Point;

/// What happened to the mouse this tick, once a drag is in progress.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DragEvent {
    /// Still held down; keep following the cursor.
    Continuing,
    /// Released quickly without much movement: opens the chat bubble.
    Clicked,
    /// Released after moving (or after the click window passed): flings
    /// the pet with this velocity.
    Thrown { velocity: Point },
}

/// A sample of the cursor position at a point in time, used to compute the
/// throw velocity over a trailing window.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Sample {
    position: Point,
    t: f64,
}

/// Tracks one grab-to-release gesture.
#[derive(Debug, Clone, PartialEq)]
pub struct Drag {
    start: Point,
    start_t: f64,
    samples: VecDeque<Sample>,
}

impl Drag {
    /// Starts tracking a grab at `cursor`, at time `t` (seconds, from any
    /// monotonic clock the caller likes — only differences matter).
    pub fn start(cursor: Point, t: f64) -> Self {
        let mut samples = VecDeque::new();
        samples.push_back(Sample {
            position: cursor,
            t,
        });
        Drag {
            start: cursor,
            start_t: t,
            samples,
        }
    }

    /// Records a new cursor position while the mouse is still held down,
    /// keeping only the samples needed for the throw-velocity window.
    pub fn track(&mut self, cursor: Point, t: f64) {
        self.samples.push_back(Sample {
            position: cursor,
            t,
        });
        while let Some(oldest) = self.samples.front() {
            if t - oldest.t > THROW_VELOCITY_WINDOW_SECONDS {
                self.samples.pop_front();
            } else {
                break;
            }
        }
    }

    /// Call every tick with the latest cursor position and left-button
    /// state. Returns what the caller should do.
    pub fn poll(&mut self, cursor: Point, left_button_down: bool, t: f64) -> DragEvent {
        if left_button_down {
            self.track(cursor, t);
            return DragEvent::Continuing;
        }
        self.release(cursor, t)
    }

    fn release(&self, cursor: Point, t: f64) -> DragEvent {
        let elapsed = t - self.start_t;
        let distance = distance(self.start, cursor);

        if distance < CLICK_MAX_DISTANCE_PT && elapsed <= CLICK_MAX_DURATION_SECONDS {
            return DragEvent::Clicked;
        }

        DragEvent::Thrown {
            velocity: self.throw_velocity(t),
        }
    }

    /// The average cursor velocity over the trailing
    /// [`THROW_VELOCITY_WINDOW_SECONDS`], capped at
    /// [`THROW_VELOCITY_MAX_PT_S`] (design §6.5).
    fn throw_velocity(&self, now: f64) -> Point {
        let Some(oldest) = self.samples.front() else {
            return Point::default();
        };
        let Some(newest) = self.samples.back() else {
            return Point::default();
        };

        let dt = (now - oldest.t).max(1.0 / TICK_HZ);
        let raw = Point {
            x: (newest.position.x - oldest.position.x) / dt,
            y: (newest.position.y - oldest.position.y) / dt,
        };
        cap_speed(raw, THROW_VELOCITY_MAX_PT_S)
    }
}

/// Right-click (or Control-click) opens the quick-action menu instead of
/// starting a drag (design §6.5, §8.4). The quick-action menu itself is
/// built in Phase 3.
pub fn is_right_click(buttons: crate::platform::Buttons) -> bool {
    buttons.right
}

fn distance(a: Point, b: Point) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    (dx * dx + dy * dy).sqrt()
}

fn cap_speed(velocity: Point, max: f64) -> Point {
    let speed = (velocity.x * velocity.x + velocity.y * velocity.y).sqrt();
    if speed <= max || speed == 0.0 {
        return velocity;
    }
    let scale = max / speed;
    Point {
        x: velocity.x * scale,
        y: velocity.y * scale,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::Buttons;

    #[test]
    fn a_quick_small_movement_is_a_click() {
        let mut drag = Drag::start(Point { x: 100.0, y: 100.0 }, 0.0);
        drag.track(Point { x: 102.0, y: 101.0 }, 0.05);

        let event = drag.poll(Point { x: 102.0, y: 101.0 }, false, 0.1);

        assert_eq!(event, DragEvent::Clicked);
    }

    #[test]
    fn moving_more_than_the_click_threshold_is_a_throw() {
        let mut drag = Drag::start(Point { x: 100.0, y: 100.0 }, 0.0);
        drag.track(Point { x: 200.0, y: 100.0 }, 0.05);

        let event = drag.poll(Point { x: 200.0, y: 100.0 }, false, 0.1);

        match event {
            DragEvent::Thrown { velocity } => assert!(velocity.x > 0.0),
            other => panic!("expected Thrown, got {other:?}"),
        }
    }

    #[test]
    fn holding_past_the_click_duration_is_a_throw_even_without_much_movement() {
        let mut drag = Drag::start(Point { x: 100.0, y: 100.0 }, 0.0);
        drag.track(Point { x: 101.0, y: 100.0 }, 0.3);

        let event = drag.poll(Point { x: 101.0, y: 100.0 }, false, 0.3);

        assert!(matches!(event, DragEvent::Thrown { .. }));
    }

    #[test]
    fn polling_while_still_held_down_just_continues() {
        let mut drag = Drag::start(Point { x: 0.0, y: 0.0 }, 0.0);

        let event = drag.poll(Point { x: 5.0, y: 5.0 }, true, 0.02);

        assert_eq!(event, DragEvent::Continuing);
    }

    #[test]
    fn throw_velocity_only_considers_the_trailing_window() {
        let mut drag = Drag::start(Point { x: 0.0, y: 0.0 }, 0.0);
        // A big early jump that should fall outside the 100ms window...
        drag.track(Point { x: 1000.0, y: 0.0 }, 0.05);
        // ...followed by a much smaller, recent movement.
        drag.track(Point { x: 1010.0, y: 0.0 }, 0.2);

        let event = drag.poll(Point { x: 1010.0, y: 0.0 }, false, 0.2);

        match event {
            DragEvent::Thrown { velocity } => {
                // Only the last ~100ms (1000 -> 1010 over 0.15s) should count,
                // not the full 0 -> 1010 over 0.2s.
                assert!(velocity.x < 1010.0 / 0.2);
            }
            other => panic!("expected Thrown, got {other:?}"),
        }
    }

    #[test]
    fn throw_velocity_is_capped() {
        let mut drag = Drag::start(Point { x: 0.0, y: 0.0 }, 0.0);
        drag.track(
            Point {
                x: 10_000.0,
                y: 0.0,
            },
            0.05,
        );

        let event = drag.poll(
            Point {
                x: 10_000.0,
                y: 0.0,
            },
            false,
            0.05,
        );

        match event {
            DragEvent::Thrown { velocity } => {
                let speed = (velocity.x * velocity.x + velocity.y * velocity.y).sqrt();
                assert!(speed <= THROW_VELOCITY_MAX_PT_S + 0.001);
            }
            other => panic!("expected Thrown, got {other:?}"),
        }
    }

    #[test]
    fn is_right_click_checks_the_right_button_only() {
        assert!(is_right_click(Buttons {
            left: false,
            right: true
        }));
        assert!(!is_right_click(Buttons {
            left: true,
            right: false
        }));
    }
}
