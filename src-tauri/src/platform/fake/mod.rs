//! A scripted platform implementation used by tests (physics, drag, …) so
//! they run deterministically without a real screen, mouse or windows.

mod mouse;
mod screens;

// Not consumed yet (pet_window.rs wires these into the real tick loop in
// 1.3); kept public so tests elsewhere can use them meanwhile.
#[allow(unused_imports)]
pub use mouse::FakeMouse;
#[allow(unused_imports)]
pub use screens::FakeScreens;
