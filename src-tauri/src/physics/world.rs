//! The world: floor, wall and ceiling surfaces built from the displays'
//! visible frames (design §6.2). Rebuilt every
//! [`WORLD_REFRESH_SECONDS`](super::constants::WORLD_REFRESH_SECONDS).
//!
//! Ledges (other apps' window tops) are added in Phase 3
//! (`physics/world.rs` visible-ledge subtraction).

use crate::platform::{Display, Point};

/// A horizontal surface (a floor or a ceiling): `y` is where the pet's
/// anchor rests, valid for `x` in `[x0, x1]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HorizontalSurface {
    pub x0: f64,
    pub x1: f64,
    pub y: f64,
}

impl HorizontalSurface {
    pub fn contains_x(&self, x: f64) -> bool {
        x >= self.x0 && x <= self.x1
    }
}

/// A vertical surface (a wall, or the gap between two displays): `x` is
/// where the pet's anchor rests, valid for `y` in `[y0, y1]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VerticalSurface {
    pub x: f64,
    pub y0: f64,
    pub y1: f64,
}

impl VerticalSurface {
    pub fn contains_y(&self, y: f64) -> bool {
        y >= self.y0 && y <= self.y1
    }
}

/// The surfaces the pet can stand, hang or climb on, rebuilt from the
/// current display arrangement.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct World {
    pub floors: Vec<HorizontalSurface>,
    pub ceilings: Vec<HorizontalSurface>,
    pub walls: Vec<VerticalSurface>,
}

impl World {
    /// Builds the world from every connected display's visible frame. Walls
    /// come from each display's left/right edges, *except* where another
    /// display's visible frame directly abuts that edge — an open doorway
    /// to walk (or fall) through rather than a wall. A real gap between
    /// displays therefore naturally keeps walls on both sides of it (design
    /// §6.7).
    pub fn build(displays: &[Display]) -> World {
        let mut floors = Vec::with_capacity(displays.len());
        let mut ceilings = Vec::with_capacity(displays.len());
        let mut walls = Vec::new();

        for display in displays {
            let frame = display.visible_frame;
            floors.push(HorizontalSurface {
                x0: frame.left(),
                x1: frame.right(),
                y: frame.bottom(),
            });
            ceilings.push(HorizontalSurface {
                x0: frame.left(),
                x1: frame.right(),
                y: frame.top(),
            });

            if !has_neighbor_at(displays, display, frame.left()) {
                walls.push(VerticalSurface {
                    x: frame.left(),
                    y0: frame.top(),
                    y1: frame.bottom(),
                });
            }
            if !has_neighbor_at(displays, display, frame.right()) {
                walls.push(VerticalSurface {
                    x: frame.right(),
                    y0: frame.top(),
                    y1: frame.bottom(),
                });
            }
        }

        World {
            floors,
            ceilings,
            walls,
        }
    }

    /// The floor directly under `x`, if any.
    pub fn floor_at(&self, x: f64) -> Option<&HorizontalSurface> {
        self.floors.iter().find(|f| f.contains_x(x))
    }

    /// The ceiling directly above `x`, if any.
    pub fn ceiling_at(&self, x: f64) -> Option<&HorizontalSurface> {
        self.ceilings.iter().find(|c| c.contains_x(x))
    }

    /// A wall within `epsilon` of `x` that covers `y`, if any.
    pub fn wall_near(&self, x: f64, y: f64, epsilon: f64) -> Option<&VerticalSurface> {
        self.walls
            .iter()
            .find(|w| (w.x - x).abs() <= epsilon && w.contains_y(y))
    }
}

/// Whether `point` is within any display's full frame. `pet_window` uses
/// this to notice when the pet has drifted off every screen (e.g. a display
/// was unplugged) so it can respawn (design §9.1).
pub fn is_on_any_display(point: Point, displays: &[Display]) -> bool {
    displays.iter().any(|d| {
        let f = d.frame;
        point.x >= f.left() && point.x <= f.right() && point.y >= f.top() && point.y <= f.bottom()
    })
}

/// The top-center of the main display's visible frame (the first display,
/// matching `NSScreen`'s ordering), used to respawn a pet that's off every
/// screen. Falls back to the origin if there are no displays at all.
pub fn spawn_position(displays: &[Display]) -> Point {
    match displays.first() {
        Some(main) => Point {
            x: main.visible_frame.left() + main.visible_frame.width / 2.0,
            y: main.visible_frame.top(),
        },
        None => Point::default(),
    }
}

/// Whether some *other* display's visible frame touches `edge_x`, meaning
/// the pet can walk (or fall) across instead of hitting a wall there.
fn has_neighbor_at(displays: &[Display], this: &Display, edge_x: f64) -> bool {
    displays.iter().any(|other| {
        let same_display = std::ptr::eq(other, this);
        if same_display {
            return false;
        }
        let frame = other.visible_frame;
        frame.left() == edge_x || frame.right() == edge_x
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::Rect;

    fn display(frame: Rect, visible_frame: Rect) -> Display {
        Display {
            frame,
            visible_frame,
        }
    }

    fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn a_single_display_gets_a_floor_ceiling_and_two_walls() {
        let frame = rect(0.0, 0.0, 1920.0, 1080.0);
        let world = World::build(&[display(frame, frame)]);

        assert_eq!(
            world.floors,
            vec![HorizontalSurface {
                x0: 0.0,
                x1: 1920.0,
                y: 1080.0
            }]
        );
        assert_eq!(
            world.ceilings,
            vec![HorizontalSurface {
                x0: 0.0,
                x1: 1920.0,
                y: 0.0
            }]
        );
        assert_eq!(world.walls.len(), 2);
        assert!(world.walls.contains(&VerticalSurface {
            x: 0.0,
            y0: 0.0,
            y1: 1080.0
        }));
        assert!(world.walls.contains(&VerticalSurface {
            x: 1920.0,
            y0: 0.0,
            y1: 1080.0
        }));
    }

    #[test]
    fn a_dock_on_the_side_moves_the_wall_inward() {
        let frame = rect(0.0, 0.0, 1920.0, 1080.0);
        // A Dock on the right, 80pt wide: visibleFrame is inset.
        let visible = rect(0.0, 0.0, 1840.0, 1080.0);
        let world = World::build(&[display(frame, visible)]);

        assert!(world.walls.contains(&VerticalSurface {
            x: 1840.0,
            y0: 0.0,
            y1: 1080.0
        }));
        assert!(!world.walls.iter().any(|w| w.x == 1920.0));
    }

    #[test]
    fn two_abutting_displays_have_no_wall_at_the_shared_edge() {
        let left = rect(0.0, 0.0, 1920.0, 1080.0);
        let right = rect(1920.0, 0.0, 1920.0, 1080.0);
        let world = World::build(&[display(left, left), display(right, right)]);

        assert!(!world.walls.iter().any(|w| w.x == 1920.0));
        // The outer edges are still walls.
        assert!(world.walls.contains(&VerticalSurface {
            x: 0.0,
            y0: 0.0,
            y1: 1080.0
        }));
        assert!(world.walls.contains(&VerticalSurface {
            x: 3840.0,
            y0: 0.0,
            y1: 1080.0
        }));
        assert_eq!(world.walls.len(), 2);
    }

    #[test]
    fn a_gap_between_displays_keeps_walls_on_both_sides() {
        let left = rect(0.0, 0.0, 1920.0, 1080.0);
        // A 200pt gap before the second display starts.
        let right = rect(2120.0, 0.0, 1920.0, 1080.0);
        let world = World::build(&[display(left, left), display(right, right)]);

        assert!(world.walls.contains(&VerticalSurface {
            x: 1920.0,
            y0: 0.0,
            y1: 1080.0
        }));
        assert!(world.walls.contains(&VerticalSurface {
            x: 2120.0,
            y0: 0.0,
            y1: 1080.0
        }));
        assert_eq!(world.walls.len(), 4);
    }

    #[test]
    fn floor_at_and_ceiling_at_only_match_within_their_display() {
        let left = rect(0.0, 0.0, 1920.0, 1080.0);
        let right = rect(1920.0, 0.0, 1920.0, 900.0);
        let world = World::build(&[display(left, left), display(right, right)]);

        assert_eq!(world.floor_at(100.0).unwrap().y, 1080.0);
        assert_eq!(world.floor_at(2000.0).unwrap().y, 900.0);
        assert!(world.floor_at(5000.0).is_none());
    }

    #[test]
    fn is_on_any_display_true_inside_a_frame_false_outside_all() {
        let frame = rect(0.0, 0.0, 1920.0, 1080.0);
        let displays = [display(frame, frame)];

        assert!(is_on_any_display(Point { x: 500.0, y: 500.0 }, &displays));
        assert!(!is_on_any_display(
            Point {
                x: 5000.0,
                y: 500.0
            },
            &displays
        ));
        assert!(!is_on_any_display(Point { x: 500.0, y: -10.0 }, &displays));
    }

    #[test]
    fn spawn_position_is_top_center_of_the_first_display() {
        let main = rect(0.0, 0.0, 1920.0, 1000.0);
        let secondary = rect(1920.0, 0.0, 1280.0, 800.0);
        let displays = [display(main, main), display(secondary, secondary)];

        assert_eq!(spawn_position(&displays), Point { x: 960.0, y: 0.0 });
    }

    #[test]
    fn spawn_position_falls_back_to_the_origin_with_no_displays() {
        assert_eq!(spawn_position(&[]), Point::default());
    }

    #[test]
    fn wall_near_matches_within_epsilon_and_y_range() {
        let frame = rect(0.0, 0.0, 1920.0, 1080.0);
        let world = World::build(&[display(frame, frame)]);

        assert!(world.wall_near(0.2, 500.0, 0.5).is_some());
        assert!(world.wall_near(10.0, 500.0, 0.5).is_none());
        assert!(world.wall_near(0.0, 2000.0, 0.5).is_none());
    }
}
