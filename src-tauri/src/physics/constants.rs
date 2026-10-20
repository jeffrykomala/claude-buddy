//! Every physics and timing magic number lives here, named and documented,
//! per the project's "constants for every physics and timing number" rule.

/// How often `pet_window` rebuilds the `World` from the platform's displays
/// (design §6.2).
pub const WORLD_REFRESH_SECONDS: f64 = 0.25;

/// The physics tick rate (design §6.3).
pub const TICK_HZ: f64 = 60.0;

/// Downward acceleration while airborne, in points per second squared
/// (design §6.3).
pub const GRAVITY_PT_S2: f64 = 1000.0;

/// The fastest the pet can fall, in points per second (design §6.3).
pub const TERMINAL_VELOCITY_PT_S: f64 = 900.0;

/// Falls landing faster than this play **splat** instead of **land**
/// (design §6.3: "a slow fall plays land; a fast fall plays a splat").
/// Chosen as two thirds of terminal velocity.
pub const SPLAT_VELOCITY_PT_S: f64 = 600.0;

/// How much speed a splat landing or a wall bounce keeps, as a fraction of
/// the incoming speed (design §6.3, §6.5).
pub const BOUNCE_DAMPING: f64 = 0.3;

/// How long the pet lies flat after a splat before resuming behaviors, in
/// seconds.
pub const SPLAT_LIE_FLAT_SECONDS: f64 = 1.2;

/// How long the **land** pose plays before resuming behaviors, in seconds.
pub const LAND_POSE_SECONDS: f64 = 0.4;

/// Chance per wall hit that the pet clings and climbs instead of bouncing
/// off (design §6.3: "about half the time").
pub const WALL_CLING_PROBABILITY: f64 = 0.5;

/// Walking speed on a floor or ledge, in points per second.
pub const WALK_SPEED_PT_S: f64 = 60.0;

/// Running speed on a floor or ledge, in points per second.
pub const RUN_SPEED_PT_S: f64 = 140.0;

/// Climbing speed on a wall or crawling speed on the ceiling, in points per
/// second.
pub const CLIMB_SPEED_PT_S: f64 = 50.0;

/// The upward speed given to a jump-off-wall (design §6.4), in points per
/// second.
pub const JUMP_SPEED_PT_S: f64 = 400.0;

/// How close (in points) the pet's anchor must be to a floor/ceiling/wall
/// before it's considered touching it.
pub const SURFACE_EPSILON_PT: f64 = 0.5;

/// Walking onto a neighboring display with a lower floor only works if the
/// drop is within this many points; a bigger drop makes the pet fall
/// instead (design §6.7).
pub const MAX_FLOOR_STEP_DOWN_PT: f64 = 24.0;

/// Below this cursor travel distance, a press-then-release is a **click**
/// rather than a drag (design §6.5).
pub const CLICK_MAX_DISTANCE_PT: f64 = 4.0;

/// Below this duration, a press-then-release is a **click** rather than a
/// drag (design §6.5).
pub const CLICK_MAX_DURATION_SECONDS: f64 = 0.25;

/// The throw velocity is averaged over this trailing window before release
/// (design §6.5).
pub const THROW_VELOCITY_WINDOW_SECONDS: f64 = 0.1;

/// The throw velocity is capped at this speed, in points per second (design
/// §6.5).
pub const THROW_VELOCITY_MAX_PT_S: f64 = 1500.0;
