//! Which keys are held down.
//!
//! The Windows console reports a key's release as well as its press, so a key is held
//! from the one to the other, and steering while firing works. A terminal that reports
//! presses alone (most of Unix's, without the kitty keyboard protocol) repeats them while
//! a key is down; there a key counts as held for a moment after each press.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};

/// How long a press counts as a hold where releases are not reported: past the
/// keyboard's first repeat delay, so a held key does not stutter.
const HOLD_WITHOUT_RELEASES: Duration = Duration::from_millis(550);

#[derive(Default)]
pub(crate) struct Keys {
    down: HashMap<KeyCode, Instant>,
    releases_reported: bool,
    /// Keys pressed since the last [`Keys::take_pressed`], for one-shot actions.
    pressed: Vec<KeyCode>,
}

impl Keys {
    pub(crate) fn handle(&mut self, event: KeyEvent, now: Instant) {
        let code = normalize(event.code);
        match event.kind {
            KeyEventKind::Press => {
                if !self.down.contains_key(&code) {
                    self.pressed.push(code);
                }
                self.down.insert(code, now);
            }
            KeyEventKind::Repeat => {
                self.down.insert(code, now);
            }
            KeyEventKind::Release => {
                self.releases_reported = true;
                self.down.remove(&code);
            }
        }
    }

    pub(crate) fn held(&self, code: KeyCode, now: Instant) -> bool {
        self.down.get(&code).is_some_and(|&since| {
            self.releases_reported || now.duration_since(since) < HOLD_WITHOUT_RELEASES
        })
    }

    pub(crate) fn take_pressed(&mut self) -> Vec<KeyCode> {
        std::mem::take(&mut self.pressed)
    }

    /// Forget presses that timed out, so a key pressed again counts as a new press.
    pub(crate) fn expire(&mut self, now: Instant) {
        if !self.releases_reported {
            self.down
                .retain(|_, since| now.duration_since(*since) < HOLD_WITHOUT_RELEASES);
        }
    }
}

/// Letters in one case, so Shift or Caps Lock change nothing.
const fn normalize(code: KeyCode) -> KeyCode {
    match code {
        KeyCode::Char(c) => KeyCode::Char(c.to_ascii_lowercase()),
        other => other,
    }
}
