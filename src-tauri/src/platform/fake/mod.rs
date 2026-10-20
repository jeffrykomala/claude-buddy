//! A scripted platform implementation used by tests (physics, drag, …) so
//! they run deterministically without a real screen, mouse or windows.

mod mouse;
mod screens;

pub use mouse::FakeMouse;
pub use screens::FakeScreens;
