//! Everything that asks the operating system about the person in front of it.
//! Each OS has its own file; they all expose the same functions.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
use linux as imp;
#[cfg(target_os = "macos")]
use macos as imp;
#[cfg(target_os = "windows")]
use windows as imp;

#[cfg(target_os = "macos")]
pub use macos::{set_strict_presentation, raise_overlay, Calendar};

/// What the OS can tell us about whether someone is looking at the screen.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Presence {
    /// Seconds since the last keyboard, mouse or touch input, system-wide.
    pub idle_seconds: f64,
    /// Locked, screensaver running, or display asleep.
    pub screen_off: bool,
    /// A video is playing: an app is keeping the display awake, or a video player reports playback.
    pub media_playing: bool,
}

pub fn presence() -> Presence {
    imp::presence()
}

/// Is any app using a camera right now? Never opens the device itself.
pub fn camera_in_use() -> bool {
    imp::camera_in_use()
}

/// Is any app recording from a microphone right now? Never opens the device itself.
pub fn microphone_in_use() -> bool {
    imp::microphone_in_use()
}

pub const OS: &str = if cfg!(target_os = "macos") {
    "macos"
} else if cfg!(target_os = "windows") {
    "windows"
} else {
    "linux"
};
