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

use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager};

use crate::layout::{self, BubbleSize, Layout, Side};
use crate::physics::constants::{PET_SIZE_PT, TICK_HZ, WORLD_REFRESH_SECONDS};
use crate::physics::drag::{Drag, DragEvent};
use crate::physics::world::{self, World};
use crate::physics::{behavior, engine};
use crate::platform::{self, Display, Mouse, PetWindowOs, Point, Rect, Screens};

/// The frontend listens for this every tick to redraw the sprite.
pub const POSE_EVENT: &str = "pet://pose";
/// Emitted when a grab resolves into a plain click (opens or closes the bubble).
pub const CLICK_EVENT: &str = "pet://click";
/// Emitted with a [`LayoutEvent`] whenever the window is resized for a bubble.
pub const LAYOUT_EVENT: &str = "pet://layout";

/// The largest bubble the frontend may ask for, in points: a bad value must
/// never make a screen-sized window.
const MAX_BUBBLE_PT: f64 = 900.0;

/// Where the pet and the bubble go inside the window: the frontend places
/// its elements from this.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutEvent {
    pub side: Side,
    pub pet_x: f64,
    pub pet_y: f64,
    pub width: f64,
    pub height: f64,
}

impl From<Layout> for LayoutEvent {
    fn from(layout: Layout) -> Self {
        LayoutEvent {
            side: layout.side,
            pet_x: layout.pet_offset.x,
            pet_y: layout.pet_offset.y,
            width: layout.window.width,
            height: layout.window.height,
        }
    }
}

/// Payload of [`POSE_EVENT`]: what the sprite needs to pick and mirror a frame.
#[derive(Debug, Clone, Copy, serde::Serialize)]
struct PoseEvent {
    pose: engine::Pose,
    facing: engine::Facing,
}

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
    /// False while "Pause wandering" is on (design §9.1).
    wander: bool,
    /// True while the buddy is listening, thinking, working or asking: the
    /// pet stands still (it can still fall and be grabbed). Design §9.2.
    paused: bool,
    displays: Vec<Display>,
    /// The open bubble, if any, and where the window sits around the pet.
    bubble: Option<BubbleSize>,
    layout: Layout,
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
                wander: true,
                paused: false,
                displays,
                bubble: None,
                layout: layout::compute(Point::default(), PET_SIZE_PT, None, Rect::default()),
            }),
            started_at: Instant::now(),
        }
    }

    /// Stands the pet still (or lets it wander again) while the buddy is busy.
    pub fn set_paused(&self, paused: bool) {
        self.inner.lock().unwrap().paused = paused;
    }

    /// Lays the window out around the pet for `bubble` (`None` closes it) and
    /// returns the layout. The window follows the pet from here on.
    fn apply_bubble(&self, bubble: Option<BubbleSize>) -> Layout {
        let mut inner = self.inner.lock().unwrap();
        let pet = pet_top_left(&inner.physics);
        let area = work_area_for(&inner.displays, pet);
        inner.bubble = bubble;
        inner.layout = layout::compute(pet, PET_SIZE_PT, bubble, area);
        inner.layout
    }

    /// The window's top-left corner for the current pet position and layout.
    fn window_origin(&self) -> Point {
        let inner = self.inner.lock().unwrap();
        window_origin(&inner.physics, &inner.layout)
    }

    fn elapsed(&self) -> f64 {
        self.started_at.elapsed().as_secs_f64()
    }

    /// Turns random wandering on or off. Off, the pet stays where it is
    /// (it still falls, and can still be grabbed and thrown).
    pub fn set_wander(&self, wander: bool) {
        self.inner.lock().unwrap().wander = wander;
    }

    /// Called from the main thread every [`WORLD_REFRESH_SECONDS`] with a
    /// freshly read display list.
    fn set_displays(&self, displays: Vec<Display>) {
        let mut inner = self.inner.lock().unwrap();
        inner.world = World::build(&displays);
        inner.displays = displays.clone();
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

/// The frontend calls this when a bubble opens, changes size or closes
/// (`height` of 0 closes it): the window grows toward the side of the pet
/// with room and the frontend is told where to put things.
#[tauri::command]
pub fn set_bubble(
    app: AppHandle,
    window: tauri::WebviewWindow,
    pet: tauri::State<'_, PetWindow>,
    width: f64,
    height: f64,
) -> Result<LayoutEvent, String> {
    let bubble = parse_bubble_size(width, height)?;
    let layout = pet.apply_bubble(bubble);
    let origin = pet.window_origin();
    window
        .set_size(LogicalSize::new(layout.window.width, layout.window.height))
        .map_err(|error| error.to_string())?;
    window
        .set_position(LogicalPosition::new(origin.x, origin.y))
        .map_err(|error| error.to_string())?;
    let event = LayoutEvent::from(layout);
    let _ = app.emit(LAYOUT_EVENT, event);
    Ok(event)
}

/// The frontend calls this as the buddy's mood changes: anything but idle
/// stands the pet still.
#[tauri::command]
pub fn set_paused(pet: tauri::State<'_, PetWindow>, paused: bool) {
    pet.set_paused(paused);
}

/// Takes keyboard focus for the input bubble (design §11). Only called when
/// the user is about to type, so clicking the pet never steals focus.
#[tauri::command]
pub fn take_focus(window: tauri::WebviewWindow) {
    let _ = window.set_focus();
}

/// Returns keyboard focus to the app the user was in before.
#[tauri::command]
pub fn release_focus(app: AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(platform) = handle.try_state::<platform::Platform>() {
            platform.release_focus();
        }
    });
}

/// A zero height closes the bubble; otherwise both sides must be real,
/// reasonable sizes (the frontend is untrusted input).
fn parse_bubble_size(width: f64, height: f64) -> Result<Option<BubbleSize>, String> {
    if height == 0.0 {
        return Ok(None);
    }
    let valid = |side: f64| side.is_finite() && side > 0.0 && side <= MAX_BUBBLE_PT;
    if !valid(width) || !valid(height) {
        return Err(format!("bubble size {width}x{height} is out of range"));
    }
    Ok(Some(BubbleSize { width, height }))
}

/// The physics anchor as the pet sprite's top-left corner.
fn pet_top_left(physics: &engine::State) -> Point {
    window_top_left(
        physics.position,
        physics.surface,
        physics.facing,
        PET_SIZE_PT,
    )
}

/// The window's top-left corner: the pet's corner minus where the pet sits
/// inside the window.
fn window_origin(physics: &engine::State, layout: &Layout) -> Point {
    let pet = pet_top_left(physics);
    Point {
        x: pet.x - layout.pet_offset.x,
        y: pet.y - layout.pet_offset.y,
    }
}

/// The visible area of the display the point is on (else the first one).
fn work_area_for(displays: &[Display], point: Point) -> Rect {
    let center = Point {
        x: point.x + PET_SIZE_PT / 2.0,
        y: point.y + PET_SIZE_PT / 2.0,
    };
    let contains = |rect: &Rect| {
        center.x >= rect.left()
            && center.x < rect.right()
            && center.y >= rect.top()
            && center.y < rect.bottom()
    };
    displays
        .iter()
        .find(|display| contains(&display.frame))
        .or_else(|| displays.first())
        .map_or_else(Rect::default, |display| display.visible_frame)
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

    let (position, pose_event, clicked) = {
        let mut inner = pet.inner.lock().unwrap();

        maybe_refresh_world(app.clone(), &mut inner, dt);

        let (input, clicked) = resolve_input(&mut inner, cursor, buttons, now);
        let (next, _events) = engine::step(&inner.physics, &inner.world, input, dt);
        inner.physics = if inner.wander && !inner.paused {
            maybe_choose_behavior(next, &inner.behaviors)
        } else {
            behavior::hold_still(&next)
        };

        let position = window_origin(&inner.physics, &inner.layout);
        let pose_event = PoseEvent {
            pose: inner.physics.pose,
            facing: inner.physics.facing,
        };
        (position, pose_event, clicked)
    };

    let _ = window.set_position(LogicalPosition::new(position.x, position.y));
    let _ = app.emit(POSE_EVENT, pose_event);
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

    fn screen() -> Display {
        let frame = Rect {
            x: 0.0,
            y: 0.0,
            width: 1440.0,
            height: 900.0,
        };
        Display {
            frame,
            visible_frame: Rect {
                y: 25.0,
                height: 875.0,
                ..frame
            },
        }
    }

    #[test]
    fn a_new_window_is_just_the_pet() {
        let pet = PetWindow::new(vec![screen()]);

        let layout = pet.apply_bubble(None);

        assert_eq!(layout.window.width, PET_SIZE_PT);
        assert_eq!(layout.window.height, PET_SIZE_PT);
    }

    #[test]
    fn opening_a_bubble_grows_the_window_and_closing_shrinks_it_back() {
        let pet = PetWindow::new(vec![screen()]);

        let open = pet.apply_bubble(Some(BubbleSize {
            width: 340.0,
            height: 260.0,
        }));
        let closed = pet.apply_bubble(None);

        assert_eq!(open.window.height, 260.0 + PET_SIZE_PT);
        assert_eq!(closed.window.height, PET_SIZE_PT);
        assert_eq!(closed.pet_offset, Point { x: 0.0, y: 0.0 });
    }

    #[test]
    fn the_window_origin_keeps_the_pet_where_it_was() {
        let pet = PetWindow::new(vec![screen()]);
        let before = pet.window_origin();

        pet.apply_bubble(Some(BubbleSize {
            width: 340.0,
            height: 260.0,
        }));
        let layout = pet.inner.lock().unwrap().layout;
        let origin = pet.window_origin();

        // The pet's own corner is unchanged by opening the bubble.
        assert_eq!(origin.x + layout.pet_offset.x, before.x);
        assert_eq!(origin.y + layout.pet_offset.y, before.y);
    }

    #[test]
    fn pausing_is_remembered() {
        let pet = PetWindow::new(vec![screen()]);

        pet.set_paused(true);

        assert!(pet.inner.lock().unwrap().paused);
    }

    #[test]
    fn zero_height_closes_the_bubble() {
        assert_eq!(parse_bubble_size(340.0, 0.0), Ok(None));
    }

    #[test]
    fn a_sensible_size_is_accepted() {
        assert_eq!(
            parse_bubble_size(340.0, 260.0),
            Ok(Some(BubbleSize {
                width: 340.0,
                height: 260.0
            }))
        );
    }

    #[test]
    fn silly_sizes_are_rejected() {
        for (width, height) in [
            (0.0, 100.0),
            (-5.0, 100.0),
            (100.0, -5.0),
            (f64::NAN, 100.0),
            (100.0, f64::INFINITY),
            (5000.0, 100.0),
            (100.0, 5000.0),
        ] {
            assert!(
                parse_bubble_size(width, height).is_err(),
                "{width}x{height}"
            );
        }
    }

    #[test]
    fn the_work_area_is_the_one_under_the_pet() {
        let left = screen();
        let right = Display {
            frame: Rect {
                x: 1440.0,
                ..left.frame
            },
            visible_frame: Rect {
                x: 1440.0,
                ..left.visible_frame
            },
        };

        let area = work_area_for(
            &[left, right],
            Point {
                x: 1600.0,
                y: 300.0,
            },
        );

        assert_eq!(area.x, 1440.0);
    }

    #[test]
    fn with_no_displays_the_work_area_is_empty() {
        assert_eq!(work_area_for(&[], Point::default()), Rect::default());
    }

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
