//! Shimeji-style physics (design §6). Everything in this module is pure —
//! no OS calls — so it's fully unit-tested without a screen. `pet_window.rs`
//! feeds it data read through the `platform` traits.

pub mod behavior;
pub mod constants;
pub mod drag;
pub mod engine;
pub mod rng;
pub mod world;
