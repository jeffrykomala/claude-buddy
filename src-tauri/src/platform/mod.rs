//! Platform abstraction (design §9.0).
//!
//! Only this module (and its `macos`/`fake` submodules) may call OS APIs.
//! Everything else — physics, the Claude runner, approvals, helpers, UI glue —
//! talks to the OS only through the traits declared here, so a future
//! Windows port only needs a new `platform/windows/` implementing the same
//! traits.

#[cfg(target_os = "macos")]
pub mod macos;

pub mod fake;

/// A point in the top-left-origin global coordinate space used throughout
/// the app (design §6.1): `(0, 0)` is the top-left of the main display and
/// `y` grows downward, in points (logical pixels).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// A rectangle in the same top-left-origin space as [`Point`].
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub fn left(&self) -> f64 {
        self.x
    }

    pub fn right(&self) -> f64 {
        self.x + self.width
    }

    pub fn top(&self) -> f64 {
        self.y
    }

    pub fn bottom(&self) -> f64 {
        self.y + self.height
    }
}

/// One connected display: its full frame and the visible area inside it
/// (`visibleFrame` already excludes the menu bar and the Dock).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Display {
    pub frame: Rect,
    pub visible_frame: Rect,
}

/// Which mouse buttons are currently held down.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Buttons {
    pub left: bool,
    pub right: bool,
}

/// The on-screen bounds of another app's window, used to build ledges
/// (design §6.6). The field list is intentionally minimal: titles are never
/// read, so no Screen Recording permission is needed.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct WindowRect {
    pub id: u32,
    pub frame: Rect,
}

/// Every connected display's frame and visible area (design §6.2, §6.7).
pub trait Screens {
    fn work_areas(&self) -> Vec<Display>;
}

/// Other apps' on-screen windows, front to back (design §6.6).
///
/// Declared now; implemented in Phase 3 (`platform/macos/window_list.rs`).
pub trait WindowList {
    fn scan(&self) -> Vec<WindowRect>;
}

/// The global mouse cursor position and button state (design §6.5).
pub trait Mouse {
    fn cursor(&self) -> Point;
    fn buttons(&self) -> Buttons;
}

/// The PATH used when spawning `claude` and other child processes, so a
/// `.app` launched from Finder can still find tools installed by Homebrew,
/// nvm, etc. (design §9.1).
///
/// Declared now; implemented in Phase 2 (`platform/macos/env.rs`).
pub trait ShellEnv {
    fn path(&self) -> String;
}

/// A signal to send to a child process group, gentlest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    /// Ends a Claude turn cleanly.
    Interrupt,
    Terminate,
    Kill,
}

/// Starting a child process in its own process group and signalling that
/// group to stop it (design §9.1, §10's cancel flow). Signalling by group id
/// (not by `Child`) lets one thread read a child's output while another
/// cancels it.
pub trait ProcessCtl {
    /// Starts `command` as the leader of a new process group, so signalling
    /// the group also reaches everything the child launches.
    fn spawn_group(&self, command: std::process::Command) -> std::io::Result<std::process::Child>;
    /// Sends `signal` to every process in the group led by `pgid` (the
    /// child's pid). Does nothing if the group is already gone.
    fn signal_group(&self, pgid: u32, signal: Signal);
}

/// One-time OS setup for the pet window: no Dock icon, visible on every
/// Space (including over full-screen apps), floating above normal windows
/// (design §11).
pub trait PetWindowOs {
    fn setup(&self, window: &tauri::WebviewWindow);
    /// Hands keyboard focus back to the app the user was using before a
    /// bubble took it for typing (design §11). Must run on the main thread.
    fn release_focus(&self);
}

/// The current platform's implementation of every trait above.
#[cfg(target_os = "macos")]
pub type Platform = macos::MacosPlatform;

/// Returns the platform implementation for the OS this binary was built for.
#[cfg(target_os = "macos")]
pub fn current() -> Platform {
    macos::MacosPlatform::new()
}
