//! This module provides platform related functions.

#[cfg(feature = "events")]
pub use self::windows::supports_keyboard_enhancement;
#[cfg(test)]
pub(crate) use self::windows::temp_screen_buffer;
pub(crate) use self::windows::{
    clear, disable_raw_mode, enable_raw_mode, is_raw_mode_enabled, scroll_down, scroll_up,
    set_size, set_window_title, size, window_size,
};

mod windows;
