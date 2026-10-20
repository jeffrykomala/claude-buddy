//! The macOS implementation of the `platform` traits (design §9.1). This is
//! the only module in v1 that implements `platform::*`; a future Windows
//! port adds `platform/windows/` alongside it.

mod app;
mod env;
mod mouse;
mod process;
mod screens;

pub use env::MacShellEnv;
pub use mouse::MacMouse;
pub use process::MacProcessCtl;
pub use screens::MacScreens;

use std::process::{Child, Command};

use crate::platform::{Buttons, Mouse, Point, ProcessCtl, Screens, ShellEnv, Signal};

/// The macOS platform: one small struct implementing every trait that's
/// ready so far. `WindowList` is added in Phase 3.
#[derive(Debug, Default)]
pub struct MacosPlatform {
    screens: MacScreens,
    mouse: MacMouse,
    shell_env: MacShellEnv,
    process: MacProcessCtl,
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

impl ShellEnv for MacosPlatform {
    fn path(&self) -> String {
        self.shell_env.path()
    }
}

impl ProcessCtl for MacosPlatform {
    fn spawn_group(&self, command: Command) -> std::io::Result<Child> {
        self.process.spawn_group(command)
    }

    fn signal_group(&self, pgid: u32, signal: Signal) {
        self.process.signal_group(pgid, signal);
    }
}
