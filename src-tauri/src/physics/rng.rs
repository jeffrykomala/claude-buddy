//! A tiny seeded xorshift64 PRNG. Used wherever physics needs a random
//! decision (wall cling-or-bounce, which behavior to pick next) while
//! keeping `engine::step` and `behavior::choose` pure, deterministic
//! functions of their inputs (design: "seeded RNG for test determinism").

/// Advances the seed one step.
pub fn next(seed: u64) -> u64 {
    let mut x = seed;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    x
}

/// Maps a seed to a float in `[0, 1)`.
pub fn unit(seed: u64) -> f64 {
    (seed >> 11) as f64 / (1u64 << 53) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_is_deterministic_for_a_given_seed() {
        assert_eq!(next(1), next(1));
        assert_ne!(next(1), next(2));
    }

    #[test]
    fn unit_stays_within_zero_and_one() {
        let mut seed = 1u64;
        for _ in 0..1000 {
            seed = next(seed);
            let value = unit(seed);
            assert!((0.0..1.0).contains(&value), "{value} out of range");
        }
    }
}
