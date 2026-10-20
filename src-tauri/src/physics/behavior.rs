//! Weighted random behaviors, as in Shimeji's `behaviors.xml`, tunable in
//! `behaviors.json` (design §6.4). Loading the file is not this module's
//! job — `behaviors.json` is read by `pet_window` (the only thing allowed
//! to touch the filesystem) and handed to [`BehaviorConfig::parse`], which
//! is itself pure (just a string-to-struct parse).

use serde::{Deserialize, Serialize};

use crate::physics::engine::{Facing, Pose, State, Surface};
use crate::physics::{constants::*, rng};

/// A behavior the pet can choose while on a floor, wall or ceiling. Some
/// variants (`SitDangle`, `JumpToLedge`) need ledges and are reserved for
/// Phase 3; they're valid to deserialize now so `behaviors.json` doesn't
/// have to change later, but the built-in [`default_config`] never picks
/// them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Stand,
    Walk,
    Run,
    Sit,
    SitDangle,
    Sleep,
    JumpToLedge,
    WalkToWall,
    ClimbUp,
    ClimbDown,
    HangOnWall,
    JumpOffWall,
    CrawlLeft,
    CrawlRight,
    HangOnCeiling,
    LetGo,
}

impl Action {
    /// The sprite pose this action renders as.
    pub fn pose(self) -> Pose {
        match self {
            Action::Stand => Pose::Stand,
            Action::Walk | Action::WalkToWall => Pose::Walk,
            Action::Run => Pose::Run,
            Action::Sit => Pose::Sit,
            Action::SitDangle => Pose::SitDangle,
            Action::Sleep => Pose::Sleep,
            Action::JumpToLedge => Pose::Jump,
            Action::ClimbUp | Action::ClimbDown => Pose::Climb,
            Action::HangOnWall => Pose::WallHang,
            Action::JumpOffWall => Pose::Jump,
            Action::CrawlLeft | Action::CrawlRight => Pose::CeilingCrawl,
            Action::HangOnCeiling => Pose::CeilingHang,
            Action::LetGo => Pose::Fall,
        }
    }

    /// The horizontal/vertical speed this action moves at, signed for
    /// direction; `None` for actions that don't move the pet themselves
    /// (e.g. `JumpOffWall`, which `engine::jump_off_wall` handles
    /// directly).
    pub fn velocity(self, facing: Facing) -> Option<crate::platform::Point> {
        use crate::platform::Point;
        let left = matches!(facing, Facing::Left);
        match self {
            Action::Walk | Action::WalkToWall => Some(Point {
                x: if left {
                    -WALK_SPEED_PT_S
                } else {
                    WALK_SPEED_PT_S
                },
                y: 0.0,
            }),
            Action::Run => Some(Point {
                x: if left {
                    -RUN_SPEED_PT_S
                } else {
                    RUN_SPEED_PT_S
                },
                y: 0.0,
            }),
            Action::ClimbUp => Some(Point {
                x: 0.0,
                y: -CLIMB_SPEED_PT_S,
            }),
            Action::ClimbDown => Some(Point {
                x: 0.0,
                y: CLIMB_SPEED_PT_S,
            }),
            Action::CrawlLeft => Some(Point {
                x: -CLIMB_SPEED_PT_S,
                y: 0.0,
            }),
            Action::CrawlRight => Some(Point {
                x: CLIMB_SPEED_PT_S,
                y: 0.0,
            }),
            Action::Stand
            | Action::Sit
            | Action::SitDangle
            | Action::Sleep
            | Action::HangOnWall
            | Action::HangOnCeiling => Some(Point { x: 0.0, y: 0.0 }),
            Action::JumpToLedge | Action::JumpOffWall | Action::LetGo => None,
        }
    }
}

/// One weighted entry in a [`BehaviorSet`]: `weight` relative to its
/// siblings, and a `[min_seconds, max_seconds]` duration range to pick
/// from.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WeightedAction {
    pub action: Action,
    pub weight: u32,
    pub min_seconds: f64,
    pub max_seconds: f64,
}

pub type BehaviorSet = Vec<WeightedAction>;

/// The full weighted behavior table, one set per surface (design §6.4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BehaviorConfig {
    pub floor: BehaviorSet,
    pub wall: BehaviorSet,
    pub ceiling: BehaviorSet,
}

impl BehaviorConfig {
    /// Parses `behaviors.json`'s contents. Pure (no file I/O): the caller
    /// reads the file.
    pub fn parse(json: &str) -> serde_json::Result<BehaviorConfig> {
        serde_json::from_str(json)
    }
}

impl Default for BehaviorConfig {
    fn default() -> Self {
        default_config()
    }
}

fn entry(action: Action, weight: u32, min_seconds: f64, max_seconds: f64) -> WeightedAction {
    WeightedAction {
        action,
        weight,
        min_seconds,
        max_seconds,
    }
}

/// The built-in weighted behaviors (design §6.4 table), used until
/// `behaviors.json` is loaded or if it's missing/invalid.
pub fn default_config() -> BehaviorConfig {
    BehaviorConfig {
        floor: vec![
            entry(Action::Stand, 3, 1.0, 2.5),
            entry(Action::Walk, 5, 1.5, 4.0),
            entry(Action::Run, 1, 1.0, 2.0),
            entry(Action::Sit, 2, 2.0, 5.0),
            entry(Action::Sleep, 1, 4.0, 8.0),
            entry(Action::WalkToWall, 2, 1.5, 4.0),
        ],
        wall: vec![
            entry(Action::ClimbUp, 3, 1.0, 2.5),
            entry(Action::ClimbDown, 3, 1.0, 2.5),
            entry(Action::HangOnWall, 2, 1.0, 3.0),
            entry(Action::JumpOffWall, 1, 0.0, 0.0),
        ],
        ceiling: vec![
            entry(Action::CrawlLeft, 3, 1.0, 2.5),
            entry(Action::CrawlRight, 3, 1.0, 2.5),
            entry(Action::HangOnCeiling, 2, 1.0, 3.0),
            entry(Action::LetGo, 1, 0.0, 0.0),
        ],
    }
}

/// Picks the next action for `surface`, weighted-randomly from `config`,
/// plus how long to keep doing it. Pure and deterministic given `rng_state`
/// (design: "seeded RNG for test determinism").
///
/// Returns `None` if `surface` has no behavior set (e.g. `Air`, `Dragged`)
/// or the set is empty.
pub fn choose(
    surface: Surface,
    config: &BehaviorConfig,
    rng_state: u64,
) -> Option<(Action, f64, u64)> {
    let set = match surface {
        Surface::Floor => &config.floor,
        Surface::Wall => &config.wall,
        Surface::Ceiling => &config.ceiling,
        Surface::Air | Surface::Dragged => return None,
    };
    if set.is_empty() {
        return None;
    }

    let total_weight: u32 = set.iter().map(|e| e.weight).sum();
    if total_weight == 0 {
        return None;
    }

    let pick_seed = rng::next(rng_state);
    let target = (rng::unit(pick_seed) * total_weight as f64) as u32;
    let mut cumulative = 0u32;
    let chosen = set
        .iter()
        .find(|e| {
            cumulative += e.weight;
            target < cumulative
        })
        .unwrap_or_else(|| set.last().expect("checked non-empty above"));

    let duration_seed = rng::next(pick_seed);
    let span = (chosen.min_seconds, chosen.max_seconds);
    let duration = span.0 + rng::unit(duration_seed) * (span.1 - span.0).max(0.0);

    Some((chosen.action, duration, duration_seed))
}

/// Applies a chosen action to `state`: sets its pose, velocity and resets
/// the behavior timer to `duration`. Transitions that need special physics
/// (`JumpOffWall`, `LetGo`) are left to the caller (`engine::jump_off_wall`,
/// `engine::let_go_from_ceiling`).
pub fn apply(state: &State, action: Action, duration: f64, rng_state: u64) -> State {
    let velocity = action.velocity(state.facing).unwrap_or(state.velocity);
    State {
        velocity,
        pose: action.pose(),
        behavior_timer: duration,
        rng_state,
        ..*state
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::Point;

    fn idle_state(surface: Surface) -> State {
        State {
            position: Point { x: 0.0, y: 0.0 },
            velocity: Point { x: 0.0, y: 0.0 },
            facing: Facing::Left,
            surface,
            pose: Pose::Stand,
            behavior_timer: 0.0,
            rng_state: 1,
        }
    }

    #[test]
    fn choose_returns_none_for_air_and_dragged() {
        let config = default_config();
        assert_eq!(choose(Surface::Air, &config, 1), None);
        assert_eq!(choose(Surface::Dragged, &config, 1), None);
    }

    #[test]
    fn choose_only_ever_picks_entries_from_the_right_set() {
        let config = default_config();
        let wall_actions = [
            Action::ClimbUp,
            Action::ClimbDown,
            Action::HangOnWall,
            Action::JumpOffWall,
        ];

        let mut seed = 1u64;
        for _ in 0..200 {
            let (action, duration, next_seed) = choose(Surface::Wall, &config, seed).unwrap();
            assert!(
                wall_actions.contains(&action),
                "{action:?} isn't a wall behavior"
            );
            assert!(duration >= 0.0);
            seed = next_seed;
        }
    }

    #[test]
    fn choose_is_deterministic_for_a_given_seed() {
        let config = default_config();
        let a = choose(Surface::Floor, &config, 99).unwrap();
        let b = choose(Surface::Floor, &config, 99).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn choose_respects_weights_over_many_draws() {
        let config = BehaviorConfig {
            floor: vec![
                entry(Action::Stand, 1, 1.0, 1.0),
                entry(Action::Walk, 9, 1.0, 1.0),
            ],
            wall: vec![],
            ceiling: vec![],
        };

        let mut seed = 1u64;
        let mut walk_count = 0;
        let draws = 2000;
        for _ in 0..draws {
            let (action, _, next_seed) = choose(Surface::Floor, &config, seed).unwrap();
            if action == Action::Walk {
                walk_count += 1;
            }
            seed = next_seed;
        }

        let walk_ratio = walk_count as f64 / draws as f64;
        assert!(
            (0.75..0.98).contains(&walk_ratio),
            "expected ~90% Walk, got {walk_ratio}"
        );
    }

    #[test]
    fn choose_returns_none_for_an_empty_set() {
        let config = BehaviorConfig {
            floor: vec![],
            wall: vec![],
            ceiling: vec![],
        };
        assert_eq!(choose(Surface::Floor, &config, 1), None);
    }

    #[test]
    fn apply_sets_pose_velocity_and_timer() {
        let state = idle_state(Surface::Floor);
        let after = apply(&state, Action::Walk, 2.5, 7);

        assert_eq!(after.pose, Pose::Walk);
        assert_eq!(after.behavior_timer, 2.5);
        assert_eq!(after.rng_state, 7);
        assert_ne!(after.velocity, Point { x: 0.0, y: 0.0 });
    }

    #[test]
    fn parse_reads_a_minimal_json_config() {
        let json = r#"{
            "floor": [{"action": "stand", "weight": 1, "min_seconds": 1.0, "max_seconds": 2.0}],
            "wall": [],
            "ceiling": []
        }"#;

        let config = BehaviorConfig::parse(json).unwrap();
        assert_eq!(config.floor.len(), 1);
        assert_eq!(config.floor[0].action, Action::Stand);
    }

    #[test]
    fn the_repos_behaviors_json_parses_and_matches_the_default_config() {
        // Compiled in, not read from disk, so this stays a pure test: it
        // just guards against behaviors.json drifting out of sync with the
        // Action schema or default_config().
        let json = include_str!("../../../behaviors.json");
        let config = BehaviorConfig::parse(json).expect("behaviors.json should parse");
        assert_eq!(config, default_config());
    }

    #[test]
    fn default_config_never_picks_the_ledge_reserved_actions() {
        let config = default_config();
        let reserved = [Action::SitDangle, Action::JumpToLedge];
        for set in [&config.floor, &config.wall, &config.ceiling] {
            for entry in set {
                assert!(
                    !reserved.contains(&entry.action),
                    "{:?} needs ledges (Phase 3)",
                    entry.action
                );
            }
        }
    }
}
