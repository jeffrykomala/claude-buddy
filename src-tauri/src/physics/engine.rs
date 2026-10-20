//! The physics engine: a pure `step()` function with no OS calls, fully
//! unit-tested (design §6.3).

use serde::{Deserialize, Serialize};

use crate::physics::constants::*;
use crate::physics::rng;
use crate::physics::world::World;
use crate::platform::Point;

/// Which way the pet is facing. Sprites face left by default; facing right
/// mirrors them (design §9.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Facing {
    Left,
    Right,
}

/// What the pet is currently touching.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    Air,
    Floor,
    Wall,
    Ceiling,
    /// Being held by the mouse; physics is suspended (design §6.5).
    Dragged,
}

/// The pet's current animation pose, matching the sprite pose names of
/// design §9.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pose {
    Stand,
    Walk,
    Run,
    Sit,
    SitDangle,
    Sleep,
    Fall,
    Land,
    Splat,
    Climb,
    WallHang,
    CeilingCrawl,
    CeilingHang,
    Jump,
    Dragged,
    Thrown,
}

/// What `pet_window` should react to this tick (sounds, notifications,
/// mood changes later; for now mostly useful for tests and logging).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Landed,
    Splatted,
    ClungToWall,
    BouncedOffWall,
    ReachedCeiling,
    LetGoFromCeiling,
    StartedFalling,
    Grabbed,
    Thrown,
}

/// What happened to the mouse this tick (design §6.5). Click-vs-drag
/// classification and throw velocity are computed by `physics::drag`; the
/// engine just reacts to the outcome.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Input {
    /// Not being touched; physics runs normally.
    None,
    /// Grabbed and being moved to `position` (already computed from the
    /// cursor and grab offset) every tick.
    Dragged { position: Point },
    /// Just released, with the throw velocity design §6.5 computed.
    Thrown { velocity: Point },
}

/// The pet's full physics state. Immutable: `step()` takes a `&State` and
/// returns a new one (design's "step() returns a new state" rule).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct State {
    /// The point where the pet currently touches its surface: feet on a
    /// floor, back on a ceiling, side on a wall. While airborne it's simply
    /// the pet's tracked position. Rendering (outside this module) offsets
    /// the sprite from this anchor using the current pose and sprite size.
    pub position: Point,
    pub velocity: Point,
    pub facing: Facing,
    pub surface: Surface,
    pub pose: Pose,
    /// Seconds remaining before `behavior::choose` picks the next action,
    /// or before a held pose (land, splat) ends.
    pub behavior_timer: f64,
    /// A small seeded PRNG's state, advanced whenever the engine needs a
    /// random decision (e.g. cling-or-bounce), so `step()` stays a pure,
    /// deterministic function of its inputs (design: "seeded RNG for test
    /// determinism").
    pub rng_state: u64,
}

impl State {
    pub fn new(position: Point, rng_seed: u64) -> Self {
        State {
            position,
            velocity: Point::default(),
            facing: Facing::Left,
            surface: Surface::Air,
            pose: Pose::Fall,
            behavior_timer: 0.0,
            rng_state: rng_seed,
        }
    }
}

/// Advances the physics simulation by `dt` seconds.
pub fn step(state: &State, world: &World, input: Input, dt: f64) -> (State, Vec<Event>) {
    match input {
        Input::Dragged { position } => (dragged(state, position), vec![Event::Grabbed]),
        Input::Thrown { velocity } => (thrown(state, velocity), vec![Event::Thrown]),
        Input::None => step_physics(state, world, dt),
    }
}

fn dragged(state: &State, position: Point) -> State {
    State {
        position,
        velocity: Point::default(),
        facing: state.facing,
        surface: Surface::Dragged,
        pose: Pose::Dragged,
        behavior_timer: 0.0,
        rng_state: state.rng_state,
    }
}

fn thrown(state: &State, velocity: Point) -> State {
    State {
        position: state.position,
        velocity,
        facing: if velocity.x >= 0.0 {
            Facing::Right
        } else {
            Facing::Left
        },
        surface: Surface::Air,
        pose: Pose::Thrown,
        behavior_timer: 0.0,
        rng_state: state.rng_state,
    }
}

fn step_physics(state: &State, world: &World, dt: f64) -> (State, Vec<Event>) {
    match state.surface {
        Surface::Air => step_air(state, world, dt),
        Surface::Floor => step_floor(state, world, dt),
        Surface::Wall => step_wall(state, world, dt),
        Surface::Ceiling => step_ceiling(state, world, dt),
        Surface::Dragged => (
            State {
                surface: Surface::Air,
                pose: Pose::Fall,
                ..*state
            },
            vec![Event::StartedFalling],
        ),
    }
}

/// Falling (or flying, just after a throw): gravity pulls down, capped at
/// terminal velocity; landing on a floor, hitting a wall or reaching a
/// ceiling end the fall.
fn step_air(state: &State, world: &World, dt: f64) -> (State, Vec<Event>) {
    let vy = (state.velocity.y + GRAVITY_PT_S2 * dt).min(TERMINAL_VELOCITY_PT_S);
    let mut next = Point {
        x: state.position.x + state.velocity.x * dt,
        y: state.position.y + vy * dt,
    };
    let mut velocity = Point {
        x: state.velocity.x,
        y: vy,
    };

    // A wall in the way of horizontal travel: cling (half the time) or
    // bounce off (design §6.3).
    if let Some(wall) = world.wall_near(
        next.x,
        next.y,
        SURFACE_EPSILON_PT.max(velocity.x.abs() * dt),
    ) {
        let (cling, rng_state) = roll(state.rng_state, WALL_CLING_PROBABILITY);
        next.x = wall.x;
        // Climbing sprites face away from the wall, into the room.
        let outward_facing = if velocity.x >= 0.0 {
            Facing::Left
        } else {
            Facing::Right
        };
        if cling {
            return (
                State {
                    position: next,
                    velocity: Point { x: 0.0, y: 0.0 },
                    facing: outward_facing,
                    surface: Surface::Wall,
                    pose: Pose::Climb,
                    behavior_timer: 0.0,
                    rng_state,
                },
                vec![Event::ClungToWall],
            );
        }
        velocity.x = -velocity.x * BOUNCE_DAMPING;
        return (
            State {
                position: next,
                velocity,
                facing: if velocity.x >= 0.0 {
                    Facing::Right
                } else {
                    Facing::Left
                },
                surface: Surface::Air,
                pose: Pose::Fall,
                behavior_timer: 0.0,
                rng_state,
            },
            vec![Event::BouncedOffWall],
        );
    }

    // A floor below: land softly, or splat if falling fast.
    if let Some(floor) = world.floor_at(next.x)
        && next.y >= floor.y
    {
        next.y = floor.y;
        if velocity.y >= SPLAT_VELOCITY_PT_S {
            return (
                State {
                    position: next,
                    velocity: Point { x: 0.0, y: 0.0 },
                    facing: state.facing,
                    surface: Surface::Floor,
                    pose: Pose::Splat,
                    behavior_timer: SPLAT_LIE_FLAT_SECONDS,
                    rng_state: state.rng_state,
                },
                vec![Event::Splatted],
            );
        }
        return (
            State {
                position: next,
                velocity: Point { x: 0.0, y: 0.0 },
                facing: state.facing,
                surface: Surface::Floor,
                pose: Pose::Land,
                behavior_timer: LAND_POSE_SECONDS,
                rng_state: state.rng_state,
            },
            vec![Event::Landed],
        );
    }

    // A ceiling above, reached while climbing a wall: start crawling
    // (handled in step_wall); reached here only when thrown upward hard
    // enough, which simply stops the ascent.
    if let Some(ceiling) = world.ceiling_at(next.x)
        && next.y <= ceiling.y
    {
        next.y = ceiling.y;
        velocity.y = 0.0;
    }

    (
        State {
            position: next,
            velocity,
            facing: state.facing,
            surface: Surface::Air,
            pose: Pose::Fall,
            behavior_timer: 0.0,
            rng_state: state.rng_state,
        },
        Vec::new(),
    )
}

/// Standing/walking on a floor: horizontal movement comes from the current
/// behavior (applied by the caller via `velocity`); walking off the edge
/// either steps down onto a lower neighbor or starts a fall.
fn step_floor(state: &State, world: &World, dt: f64) -> (State, Vec<Event>) {
    if state.behavior_timer > 0.0 {
        return (
            State {
                behavior_timer: (state.behavior_timer - dt).max(0.0),
                ..*state
            },
            Vec::new(),
        );
    }

    let next_x = state.position.x + state.velocity.x * dt;

    // Walking into a wall starts climbing it (design §6.4: "walk to a wall
    // and climb"); the next behavior pick decides which way.
    if let Some(wall) = world.wall_near(
        next_x,
        state.position.y,
        SURFACE_EPSILON_PT.max(state.velocity.x.abs() * dt),
    ) {
        let outward_facing = if state.velocity.x >= 0.0 {
            Facing::Left
        } else {
            Facing::Right
        };
        return (
            State {
                position: Point {
                    x: wall.x,
                    y: state.position.y,
                },
                velocity: Point { x: 0.0, y: 0.0 },
                facing: outward_facing,
                surface: Surface::Wall,
                pose: Pose::Climb,
                behavior_timer: 0.0,
                rng_state: state.rng_state,
            },
            Vec::new(),
        );
    }

    if let Some(floor) = world.floor_at(next_x) {
        let drop = floor.y - state.position.y;
        if drop.abs() <= MAX_FLOOR_STEP_DOWN_PT {
            return (
                State {
                    position: Point {
                        x: next_x,
                        y: floor.y,
                    },
                    ..*state
                },
                Vec::new(),
            );
        }
    }

    // No floor under the next step (or too big a drop): start falling.
    (
        State {
            position: Point {
                x: next_x,
                y: state.position.y,
            },
            velocity: Point {
                x: state.velocity.x,
                y: 0.0,
            },
            facing: state.facing,
            surface: Surface::Air,
            pose: Pose::Fall,
            behavior_timer: 0.0,
            rng_state: state.rng_state,
        },
        vec![Event::StartedFalling],
    )
}

/// Climbing (or hanging still on) a wall: vertical movement comes from the
/// chosen behavior; reaching the top moves onto the ceiling, reaching the
/// bottom returns to the floor.
fn step_wall(state: &State, world: &World, dt: f64) -> (State, Vec<Event>) {
    let next_y = state.position.y + state.velocity.y * dt;

    if let Some(ceiling) = world.ceiling_at(state.position.x)
        && next_y <= ceiling.y
    {
        return (
            State {
                position: Point {
                    x: state.position.x,
                    y: ceiling.y,
                },
                velocity: Point { x: 0.0, y: 0.0 },
                facing: state.facing,
                surface: Surface::Ceiling,
                pose: Pose::CeilingCrawl,
                behavior_timer: 0.0,
                rng_state: state.rng_state,
            },
            vec![Event::ReachedCeiling],
        );
    }

    if let Some(floor) = world.floor_at(state.position.x)
        && next_y >= floor.y
    {
        return (
            State {
                position: Point {
                    x: state.position.x,
                    y: floor.y,
                },
                velocity: Point { x: 0.0, y: 0.0 },
                facing: state.facing,
                surface: Surface::Floor,
                pose: Pose::Stand,
                behavior_timer: 0.0,
                rng_state: state.rng_state,
            },
            vec![Event::Landed],
        );
    }

    (
        State {
            position: Point {
                x: state.position.x,
                y: next_y,
            },
            ..*state
        },
        Vec::new(),
    )
}

/// Crawling on (or hanging still from) the ceiling. Letting go is a
/// deliberate transition the caller makes via [`let_go_from_ceiling`] once
/// `behavior::choose` picks `LetGo` (design §6.4).
fn step_ceiling(state: &State, world: &World, dt: f64) -> (State, Vec<Event>) {
    let next_x = (state.position.x + state.velocity.x * dt).clamp(
        world
            .ceiling_at(state.position.x)
            .map(|c| c.x0)
            .unwrap_or(state.position.x),
        world
            .ceiling_at(state.position.x)
            .map(|c| c.x1)
            .unwrap_or(state.position.x),
    );

    (
        State {
            position: Point {
                x: next_x,
                y: state.position.y,
            },
            behavior_timer: (state.behavior_timer - dt).max(0.0),
            ..*state
        },
        Vec::new(),
    )
}

/// Lets go from the ceiling and starts falling (design §6.4: "let go
/// (fall)"), called by the caller once `behavior::choose` picks `LetGo`.
pub fn let_go_from_ceiling(state: &State) -> (State, Vec<Event>) {
    (
        State {
            velocity: Point { x: 0.0, y: 0.0 },
            surface: Surface::Air,
            pose: Pose::Fall,
            behavior_timer: 0.0,
            ..*state
        },
        vec![Event::LetGoFromCeiling],
    )
}

/// Jumps off a wall, away from it (design §6.4: "jump off"), called by the
/// caller once `behavior::choose` picks `JumpOffWall`.
pub fn jump_off_wall(state: &State) -> (State, Vec<Event>) {
    let away_from_wall = if state.facing == Facing::Left {
        -1.0
    } else {
        1.0
    };
    (
        State {
            velocity: Point {
                x: away_from_wall * WALK_SPEED_PT_S,
                y: -JUMP_SPEED_PT_S,
            },
            surface: Surface::Air,
            pose: Pose::Jump,
            behavior_timer: 0.0,
            ..*state
        },
        vec![Event::StartedFalling],
    )
}

/// Draws a random boolean, `true` with probability `p`, using the shared
/// seeded PRNG. Returns `(outcome, next_seed)`.
fn roll(seed: u64, p: f64) -> (bool, u64) {
    let next_seed = rng::next(seed);
    (rng::unit(next_seed) < p, next_seed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::{Display, Rect};

    fn single_display_world() -> World {
        let frame = Rect {
            x: 0.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
        };
        World::build(&[Display {
            frame,
            visible_frame: frame,
        }])
    }

    fn falling_state(x: f64, y: f64, vy: f64) -> State {
        State {
            position: Point { x, y },
            velocity: Point { x: 0.0, y: vy },
            facing: Facing::Left,
            surface: Surface::Air,
            pose: Pose::Fall,
            behavior_timer: 0.0,
            rng_state: 1,
        }
    }

    #[test]
    fn gravity_increases_downward_speed_until_terminal_velocity() {
        let world = single_display_world();
        // x = 5000 has no floor/wall/ceiling nearby, so it just keeps
        // falling instead of landing partway through the test.
        let state = falling_state(5000.0, 0.0, 0.0);

        let (after_one_tick, _) = step(&state, &world, Input::None, 1.0 / 60.0);
        assert!(after_one_tick.velocity.y > 0.0);
        assert!(after_one_tick.velocity.y < TERMINAL_VELOCITY_PT_S);

        let (after_two_seconds, _) = step(&after_one_tick, &world, Input::None, 2.0);
        assert_eq!(after_two_seconds.velocity.y, TERMINAL_VELOCITY_PT_S);
    }

    #[test]
    fn a_slow_fall_lands_on_the_floor() {
        let world = single_display_world();
        let state = falling_state(500.0, 1079.0, 100.0);

        let (after, events) = step(&state, &world, Input::None, 1.0 / 60.0);

        assert_eq!(after.surface, Surface::Floor);
        assert_eq!(after.pose, Pose::Land);
        assert_eq!(after.position.y, 1080.0);
        assert_eq!(events, vec![Event::Landed]);
    }

    #[test]
    fn a_fast_fall_splats() {
        let world = single_display_world();
        let state = falling_state(500.0, 1000.0, TERMINAL_VELOCITY_PT_S);

        let (after, events) = step(&state, &world, Input::None, 1.0);

        assert_eq!(after.surface, Surface::Floor);
        assert_eq!(after.pose, Pose::Splat);
        assert!(after.behavior_timer > 0.0);
        assert_eq!(events, vec![Event::Splatted]);
    }

    #[test]
    fn thrown_into_a_wall_either_clings_or_bounces() {
        let world = single_display_world();
        // Flying hard to the right, about to cross the right wall at x=1920.
        let state = State {
            position: Point {
                x: 1918.0,
                y: 500.0,
            },
            velocity: Point { x: 400.0, y: 0.0 },
            facing: Facing::Right,
            surface: Surface::Air,
            pose: Pose::Thrown,
            behavior_timer: 0.0,
            rng_state: 42,
        };

        let (after, events) = step(&state, &world, Input::None, 1.0 / 60.0);

        assert_eq!(after.position.x, 1920.0);
        match after.surface {
            Surface::Wall => {
                assert_eq!(after.pose, Pose::Climb);
                assert_eq!(events, vec![Event::ClungToWall]);
            }
            Surface::Air => {
                assert_eq!(events, vec![Event::BouncedOffWall]);
                assert!(
                    after.velocity.x < 0.0,
                    "bouncing off the right wall should send it left"
                );
            }
            other => panic!("expected Wall or Air, got {other:?}"),
        }
    }

    #[test]
    fn cling_or_bounce_is_deterministic_for_a_given_seed() {
        let world = single_display_world();
        let state = State {
            position: Point {
                x: 1918.0,
                y: 500.0,
            },
            velocity: Point { x: 400.0, y: 0.0 },
            facing: Facing::Right,
            surface: Surface::Air,
            pose: Pose::Thrown,
            behavior_timer: 0.0,
            rng_state: 7,
        };

        let (a, _) = step(&state, &world, Input::None, 1.0 / 60.0);
        let (b, _) = step(&state, &world, Input::None, 1.0 / 60.0);

        assert_eq!(a.surface, b.surface);
    }

    #[test]
    fn climbing_a_wall_onto_the_ceiling() {
        let world = single_display_world();
        let state = State {
            position: Point { x: 0.0, y: 10.0 },
            velocity: Point {
                x: 0.0,
                y: -CLIMB_SPEED_PT_S,
            },
            facing: Facing::Left,
            surface: Surface::Wall,
            pose: Pose::Climb,
            behavior_timer: 0.0,
            rng_state: 1,
        };

        let (after, events) = step(&state, &world, Input::None, 1.0);

        assert_eq!(after.surface, Surface::Ceiling);
        assert_eq!(after.pose, Pose::CeilingCrawl);
        assert_eq!(after.position.y, 0.0);
        assert_eq!(events, vec![Event::ReachedCeiling]);
    }

    #[test]
    fn letting_go_from_the_ceiling_falls() {
        let state = State {
            position: Point { x: 500.0, y: 0.0 },
            velocity: Point { x: 10.0, y: 0.0 },
            facing: Facing::Left,
            surface: Surface::Ceiling,
            pose: Pose::CeilingHang,
            behavior_timer: 0.0,
            rng_state: 1,
        };

        let (after, events) = let_go_from_ceiling(&state);

        assert_eq!(after.surface, Surface::Air);
        assert_eq!(after.pose, Pose::Fall);
        assert_eq!(after.velocity, Point { x: 0.0, y: 0.0 });
        assert_eq!(events, vec![Event::LetGoFromCeiling]);
    }

    #[test]
    fn jumping_off_a_wall_sends_it_outward_and_up() {
        let state = State {
            position: Point { x: 0.0, y: 500.0 },
            velocity: Point { x: 0.0, y: 0.0 },
            facing: Facing::Left,
            surface: Surface::Wall,
            pose: Pose::Climb,
            behavior_timer: 0.0,
            rng_state: 1,
        };

        let (after, events) = jump_off_wall(&state);

        assert_eq!(after.surface, Surface::Air);
        assert_eq!(after.pose, Pose::Jump);
        assert!(
            after.velocity.x < 0.0,
            "should jump away from the wall it was facing"
        );
        assert!(after.velocity.y < 0.0, "should jump upward");
        assert_eq!(events, vec![Event::StartedFalling]);
    }

    #[test]
    fn being_dragged_pauses_physics_and_follows_the_cursor() {
        let world = single_display_world();
        let state = falling_state(500.0, 500.0, 300.0);

        let (after, events) = step(
            &state,
            &world,
            Input::Dragged {
                position: Point { x: 42.0, y: 84.0 },
            },
            1.0 / 60.0,
        );

        assert_eq!(after.surface, Surface::Dragged);
        assert_eq!(after.pose, Pose::Dragged);
        assert_eq!(after.position, Point { x: 42.0, y: 84.0 });
        assert_eq!(after.velocity, Point { x: 0.0, y: 0.0 });
        assert_eq!(events, vec![Event::Grabbed]);
    }

    #[test]
    fn releasing_applies_the_throw_velocity_and_resumes_falling() {
        let world = single_display_world();
        let dragged = State {
            position: Point { x: 500.0, y: 500.0 },
            velocity: Point { x: 0.0, y: 0.0 },
            facing: Facing::Left,
            surface: Surface::Dragged,
            pose: Pose::Dragged,
            behavior_timer: 0.0,
            rng_state: 1,
        };

        let (after, events) = step(
            &dragged,
            &world,
            Input::Thrown {
                velocity: Point {
                    x: 800.0,
                    y: -200.0,
                },
            },
            1.0 / 60.0,
        );

        assert_eq!(after.surface, Surface::Air);
        assert_eq!(after.pose, Pose::Thrown);
        assert_eq!(
            after.velocity,
            Point {
                x: 800.0,
                y: -200.0
            }
        );
        assert_eq!(after.facing, Facing::Right);
        assert_eq!(events, vec![Event::Thrown]);
    }

    #[test]
    fn walking_into_a_wall_starts_climbing_it() {
        let world = single_display_world();
        let state = State {
            position: Point {
                x: 1919.6,
                y: 1080.0,
            },
            velocity: Point {
                x: WALK_SPEED_PT_S,
                y: 0.0,
            },
            facing: Facing::Right,
            surface: Surface::Floor,
            pose: Pose::Walk,
            behavior_timer: 0.0,
            rng_state: 1,
        };

        let (after, events) = step(&state, &world, Input::None, 1.0 / 60.0);

        assert_eq!(
            after.position.x,
            world.walls.iter().find(|w| w.x > 1900.0).unwrap().x
        );
        assert_eq!(after.velocity.x, 0.0);
        assert_eq!(after.surface, Surface::Wall);
        assert_eq!(after.pose, Pose::Climb);
        assert!(events.is_empty());
    }

    #[test]
    fn walking_across_two_displays_at_the_same_height_does_not_fall() {
        let left = Rect {
            x: 0.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
        };
        let right = Rect {
            x: 1920.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
        };
        let world = World::build(&[
            Display {
                frame: left,
                visible_frame: left,
            },
            Display {
                frame: right,
                visible_frame: right,
            },
        ]);
        let state = State {
            position: Point {
                x: 1919.6,
                y: 1080.0,
            },
            velocity: Point {
                x: WALK_SPEED_PT_S,
                y: 0.0,
            },
            facing: Facing::Right,
            surface: Surface::Floor,
            pose: Pose::Walk,
            behavior_timer: 0.0,
            rng_state: 1,
        };

        let (after, events) = step(&state, &world, Input::None, 1.0 / 60.0);

        assert_eq!(after.surface, Surface::Floor);
        assert!(after.position.x > 1919.6);
        assert!(events.is_empty());
    }
}
