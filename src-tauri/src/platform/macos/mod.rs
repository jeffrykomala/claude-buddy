//! The macOS implementation of the `platform` traits (design §9.1). This is
//! the only module in v1 that implements `platform::*`; a future Windows
//! port adds `platform/windows/` alongside it.

mod app;
mod mouse;
mod screens;

pub use mouse::MacMouse;
pub use screens::MacScreens;

use crate::platform::{Buttons, Mouse, Point, Screens};

/// The macOS platform: one small struct implementing every trait that's
/// ready in this phase. `WindowList`, `ShellEnv` and `ProcessCtl` are added
/// in later phases.
#[derive(Debug, Default)]
pub struct MacosPlatform {
    screens: MacScreens,
    mouse: MacMouse,
}

impl MacosPlatform {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Screens for MacosPlatform {
    fn work_areas(&self) -> Vec<crate::platform::Display> {
        self.screens.work_areas()
    }
}

impl Mouse for MacosPlatform {
    fn cursor(&self) -> Point {
        self.mouse.cursor()
    }

    fn buttons(&self) -> Buttons {
        self.mouse.buttons()
    }
}
