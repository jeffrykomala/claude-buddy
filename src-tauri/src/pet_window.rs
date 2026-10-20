//! Wires the pure physics engine to the real pet window: a 60 Hz tick that
//! steps the physics and moves the window, with the world (`NSScreen`)
//! refreshed on the main thread every 250 ms (design §9.1).
//!
//! `NSScreen` only answers from the AppKit main thread; `CGEvent` (the
//! mouse) doesn't care which thread calls it. So the 60 Hz loop below reads
//! the mouse and moves the window from a background task (Tauri's window
//! dispatcher is thread-safe), while the display list is refreshed via
//! [`tauri::AppHandle::run_on_main_thread`] and cached for the tick loop to
//! read.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, LogicalPosition, Manager};

use crate::physics::constants::{PET_SIZE_PT, TICK_HZ, WORLD_REFRESH_SECONDS};
use crate::physics::drag::{Drag, DragEvent};
use crate::physics::world::{self, World};
use crate::physics::{behavior, engine};
use crate::platform::{self, Display, Mouse, Point, Screens};

/// The frontend listens for this every tick to redraw the sprite.
pub const POSE_EVENT: &str = "pet://pose";
/// Emitted when a grab resolves into a plain click (Phase 2 opens the bubble).
pub const CLICK_EVENT: &str = "pet://click";

struct Dragging {
    drag: Drag,
    /// Where the user pressed, relative to the window's top-left corner, so
    /// the pet doesn't jump to be centered under the cursor.
    offset: Point,
}

struct Inner {
    physics: engine::State,
    world: World,
    world_age: f64,
    behaviors: behavior::BehaviorConfig,
    drag: Option<Dragging>,
}

/// The pet's shared state, managed by Tauri and used by both the tick loop
/// and the `grab`/`click` commands.
pub struct PetWindow {
    inner: Mutex<Inner>,
    started_at: Instant,
}

impl PetWindow {
    /// `displays` should come from a call to `Screens::work_areas` made on
    /// the main thread (e.g. during `tauri::Builder::setup`).
    pub fn new(displays: Vec<Display>) -> Self {
        let world = World::build(&displays);
        let position = world::spawn_position(&displays);
        PetWindow {
            inner: Mutex::new(Inner {
                physics: engine::State::new(position, 0xDE5C_B0DD),
                world,
                world_age: 0.0,
                behaviors: behavior::default_config(),
                drag: None,
            }),
            started_at: Instant::now(),
        }
    }

    fn elapsed(&self) -> f64 {
        self.started_at.elapsed().as_secs_f64()
    }

    /// Called from the main thread every [`WORLD_REFRESH_SECONDS`] with a
    /// freshly read display list.
    fn set_displays(&self, displays: Vec<Display>) {
        let mut inner = self.inner.lock().unwrap();
        inner.world = World::build(&displays);
        if !world::is_on_any_display(inner.physics.position, &displays) {
            let seed = inner.physics.rng_state;
            inner.physics = engine::State::new(world::spawn_position(&displays), seed);
        }
    }
}

/// Starts a grab: the frontend calls this on `pointerdown` over the pet,
/// passing the press point relative to the window's top-left corner.
#[tauri::command]
pub fn grab(
    pet: tauri::State<'_, PetWindow>,
    platform: tauri::State<'_, platform::Platform>,
    offset_x: f64,
    offset_y: f64,
) {
    let cursor = platform.cursor();
    let now = pet.elapsed();
    let mut inner = pet.inner.lock().unwrap();
    inner.drag = Some(Dragging {
        drag: Drag::start(cursor, now),
        offset: Point {
            x: offset_x,
            y: offset_y,
        },
    });
}

/// A plain click outside of any grab (e.g. a hotkey-driven open), forwarded
/// to the frontend the same way a resolved grab-click is (design §6.5).
#[tauri::command]
pub fn click(app: AppHandle) {
    let _ = app.emit(CLICK_EVENT, ());
}

/// Spawns the 60 Hz physics/render tick as a background task.
pub fn spawn_tick_loop(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs_f64(1.0 / TICK_HZ));
        let mut last = Instant::now();
        loop {
            interval.tick().await;
            let now = Instant::now();
            let dt = now.duration_since(last).as_secs_f64();
            last = now;
            tick(&app, dt);
        }
    });
}

fn tick(app: &AppHandle, dt: f64) {
    let (Some(window), Some(pet), Some(platform)) = (
        app.get_webview_window("main"),
        app.try_state::<PetWindow>(),
        app.try_state::<platform::Platform>(),
    ) else {
        return;
    };

    let cursor = platform.cursor();
    let buttons = platform.buttons();
    let now = pet.elapsed();

    let (position, pose, clicked) = {
        let mut inner = pet.inner.lock().unwrap();

        maybe_refresh_world(app.clone(), &mut inner, dt);

        let (input, clicked) = resolve_input(&mut inner, cursor, buttons, now);
        let (next, _events) = engine::step(&inner.physics, &inner.world, input, dt);
        inner.physics = maybe_choose_behavior(next, &inner.behaviors);

        let position = window_top_left(
            inner.physics.position,
            inner.physics.surface,
            inner.physics.facing,
            PET_SIZE_PT,
        );
        (position, inner.physics.pose, clicked)
    };

    let _ = window.set_position(LogicalPosition::new(position.x, position.y));
    let _ = app.emit(POSE_EVENT, pose);
    if clicked {
        let _ = app.emit(CLICK_EVENT, ());
    }
}

/// Every [`WORLD_REFRESH_SECONDS`], asks the main thread to re-read the
/// displays (`NSScreen` only answers there) and cache the result.
fn maybe_refresh_world(app: AppHandle, inner: &mut Inner, dt: f64) {
    inner.world_age += dt;
    if inner.world_age < WORLD_REFRESH_SECONDS {
        return;
    }
    inner.world_age = 0.0;

    let _ = app.clone().run_on_main_thread(move || {
        if let (Some(pet), Some(platform)) = (
            app.try_state::<PetWindow>(),
            app.try_state::<platform::Platform>(),
        ) {
            pet.set_displays(platform.work_areas());
        }
    });
}

/// Resolves this tick's mouse state into a physics [`engine::Input`], plus
/// whether a grab just resolved into a plain click.
fn resolve_input(
    inner: &mut Inner,
    cursor: Point,
    buttons: platform::Buttons,
    now: f64,
) -> (engine::Input, bool) {
    let Some(dragging) = inner.drag.as_mut() else {
        return (engine::Input::None, false);
    };

    match dragging.drag.poll(cursor, buttons.left, now) {
        DragEvent::Continuing => (
            engine::Input::Dragged {
                position: Point {
                    x: cursor.x - dragging.offset.x,
                    y: cursor.y - dragging.offset.y,
                },
            },
            false,
        ),
        DragEvent::Clicked => {
            inner.drag = None;
            (engine::Input::None, true)
        }
        DragEvent::Thrown { velocity } => {
            inner.drag = None;
            (engine::Input::Thrown { velocity }, false)
        }
    }
}

/// Picks the next behavior once the current one's timer runs out, applying
/// any special transition it needs (design §6.4).
fn maybe_choose_behavior(state: engine::State, config: &behavior::BehaviorConfig) -> engine::State {
    if state.behavior_timer > 0.0 {
        return state;
    }
    match behavior::choose(state.surface, config, state.rng_state) {
        Some((behavior::Action::LetGo, _, rng_state)) => {
            engine::let_go_from_ceiling(&engine::State { rng_state, ..state }).0
        }
        Some((behavior::Action::JumpOffWall, _, rng_state)) => {
            engine::jump_off_wall(&engine::State { rng_state, ..state }).0
        }
        Some((action, duration, rng_state)) => behavior::apply(&state, action, duration, rng_state),
        None => state,
    }
}

/// Converts the physics anchor (design: "the point where the pet currently
/// touches its surface") into the window's top-left corner, for a
/// `size`×`size` sprite. Until real sprites land (1.4) every character uses
/// this one fixed size.
fn window_top_left(
    anchor: Point,
    surface: engine::Surface,
    facing: engine::Facing,
    size: f64,
) -> Point {
    use engine::{Facing, Surface};
    match surface {
        Surface::Floor => Point {
            x: anchor.x - size / 2.0,
            y: anchor.y - size,
        },
        Surface::Ceiling => Point {
            x: anchor.x - size / 2.0,
            y: anchor.y,
        },
        Surface::Wall => {
            // Climbing sprites face away from the wall, so facing Right
            // means the wall (and the pet's near edge) is on the left.
            let x = if facing == Facing::Right {
                anchor.x
            } else {
                anchor.x - size
            };
            Point {
                x,
                y: anchor.y - size / 2.0,
            }
        }
        Surface::Air | Surface::Dragged => Point {
            x: anchor.x - size / 2.0,
            y: anchor.y - size / 2.0,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::engine::{Facing, Surface};

    #[test]
    fn floor_anchor_is_bottom_center() {
        let top_left = window_top_left(
            Point { x: 100.0, y: 200.0 },
            Surface::Floor,
            Facing::Left,
            40.0,
        );
        assert_eq!(top_left, Point { x: 80.0, y: 160.0 });
    }

    #[test]
    fn ceiling_anchor_is_top_center() {
        let top_left = window_top_left(
            Point { x: 100.0, y: 200.0 },
            Surface::Ceiling,
            Facing::Left,
            40.0,
        );
        assert_eq!(top_left, Point { x: 80.0, y: 200.0 });
    }

    #[test]
    fn wall_anchor_sits_on_the_near_edge() {
        // Facing Right means climbing a wall on the left (the pet's left
        // edge touches it), so the window's left edge is at the anchor.
        let on_left_wall = window_top_left(
            Point { x: 0.0, y: 200.0 },
            Surface::Wall,
            Facing::Right,
            40.0,
        );
        assert_eq!(on_left_wall, Point { x: 0.0, y: 180.0 });

        let on_right_wall = window_top_left(
            Point {
                x: 1920.0,
                y: 200.0,
            },
            Surface::Wall,
            Facing::Left,
            40.0,
        );
        assert_eq!(
            on_right_wall,
            Point {
                x: 1880.0,
                y: 180.0
            }
        );
    }

    #[test]
    fn air_and_dragged_are_centered_on_the_anchor() {
        let falling = window_top_left(Point { x: 50.0, y: 50.0 }, Surface::Air, Facing::Left, 40.0);
        assert_eq!(falling, Point { x: 30.0, y: 30.0 });

        let dragged = window_top_left(
            Point { x: 50.0, y: 50.0 },
            Surface::Dragged,
            Facing::Left,
            40.0,
        );
        assert_eq!(dragged, falling);
    }

    #[test]
    fn maybe_choose_behavior_keeps_waiting_while_the_timer_runs() {
        let state = engine::State {
            behavior_timer: 1.5,
            ..engine::State::new(Point::default(), 1)
        };
        let config = behavior::default_config();

        let after = maybe_choose_behavior(state, &config);

        assert_eq!(after.behavior_timer, 1.5);
        assert_eq!(after.pose, state.pose);
    }

    #[test]
    fn maybe_choose_behavior_picks_something_new_once_the_timer_elapses() {
        let state = engine::State {
            surface: Surface::Floor,
            behavior_timer: 0.0,
            ..engine::State::new(Point::default(), 1)
        };
        let config = behavior::default_config();

        let after = maybe_choose_behavior(state, &config);

        assert_ne!(after.rng_state, state.rng_state);
    }

    #[test]
    fn maybe_choose_behavior_lets_go_from_the_ceiling_by_falling() {
        // Seed chosen so `LetGo` (the only ceiling action with weight 1 out
        // of the small default set) sometimes needs a few draws; loop until
        // we observe a transition to Air, which should only ever happen via
        // LetGo from the Ceiling surface.
        let config = behavior::default_config();
        let mut seed = 1u64;
        let mut saw_fall = false;
        for _ in 0..500 {
            let state = engine::State {
                surface: Surface::Ceiling,
                behavior_timer: 0.0,
                ..engine::State::new(Point::default(), seed)
            };
            let after = maybe_choose_behavior(state, &config);
            seed = after.rng_state;
            if after.surface == Surface::Air {
                assert_eq!(after.pose, engine::Pose::Fall);
                saw_fall = true;
                break;
            }
        }
        assert!(
            saw_fall,
            "expected LetGo to be chosen at least once in 500 draws"
        );
    }
}
