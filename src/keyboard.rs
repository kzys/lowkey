use std::time::{Duration, Instant};

use input_linux::Key;

use crate::keys::{COLS, KEYS, ROWS};

const REPEAT_DELAY: Duration = Duration::from_millis(400);
const REPEAT_INTERVAL: Duration = Duration::from_millis(120);

/// Navigation, modifier-latch and held-direction-repeat state for the grid.
/// Pure state machine: no I/O, so it's straightforward to unit test.
pub struct Keyboard {
    pub sel_row: usize,
    pub sel_col: usize,
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub dirty: bool,
    repeat_dir: Option<(i32, i32)>,
    repeat_at: Instant,
}

impl Keyboard {
    pub fn new() -> Self {
        Keyboard {
            sel_row: 0,
            sel_col: 0,
            shift: false,
            ctrl: false,
            alt: false,
            dirty: false,
            repeat_dir: None,
            repeat_at: Instant::now(),
        }
    }

    pub fn move_sel(&mut self, dr: i32, dc: i32) {
        self.sel_row = (self.sel_row as i32 + dr).rem_euclid(ROWS as i32) as usize;
        self.sel_col = (self.sel_col as i32 + dc).rem_euclid(COLS as i32) as usize;
        self.dirty = true;
    }

    pub fn selected_key(&self) -> Key {
        KEYS[self.sel_row][self.sel_col].code
    }

    pub fn toggle_shift(&mut self) {
        self.shift = !self.shift;
        self.dirty = true;
    }

    pub fn toggle_ctrl(&mut self) {
        self.ctrl = !self.ctrl;
        self.dirty = true;
    }

    pub fn toggle_alt(&mut self) {
        self.alt = !self.alt;
        self.dirty = true;
    }

    pub fn latched(&self) -> bool {
        self.shift || self.ctrl || self.alt
    }

    /// Returns the modifiers to apply to a tap, then clears them: modifiers
    /// are one-shot, the way a phone keyboard behaves.
    pub fn consume_modifiers(&mut self) -> (bool, bool, bool) {
        let mods = (self.shift, self.ctrl, self.alt);
        if mods.0 || mods.1 || mods.2 {
            self.shift = false;
            self.ctrl = false;
            self.alt = false;
            self.dirty = true;
        }
        mods
    }

    /// Begins auto-repeat for a just-pressed direction.
    pub fn start_repeat(&mut self, dr: i32, dc: i32, now: Instant) {
        self.repeat_dir = Some((dr, dc));
        self.repeat_at = now + REPEAT_DELAY;
    }

    /// Ends auto-repeat on release, regardless of which direction was held.
    pub fn stop_repeat(&mut self) {
        self.repeat_dir = None;
    }

    /// The poll timeout (in milliseconds) needed to wake up for the next
    /// repeat, or -1 (block indefinitely) when nothing is held.
    pub fn repeat_timeout_ms(&self, now: Instant) -> i32 {
        match self.repeat_dir {
            None => -1,
            Some(_) if self.repeat_at <= now => 0,
            Some(_) => (self.repeat_at - now).as_millis().min(i32::MAX as u128) as i32,
        }
    }

    /// Fires a repeat move if one is due. Returns whether it moved.
    pub fn tick_repeat(&mut self, now: Instant) -> bool {
        let Some((dr, dc)) = self.repeat_dir else { return false };
        if now < self.repeat_at {
            return false;
        }
        self.move_sel(dr, dc);
        self.repeat_at = now + REPEAT_INTERVAL;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_sel_wraps_at_grid_edges() {
        let mut kb = Keyboard::new();
        kb.move_sel(0, -1);
        assert_eq!((kb.sel_row, kb.sel_col), (0, COLS - 1));

        let mut kb = Keyboard::new();
        kb.move_sel(-1, 0);
        assert_eq!((kb.sel_row, kb.sel_col), (ROWS - 1, 0));

        let mut kb = Keyboard::new();
        for _ in 0..COLS {
            kb.move_sel(0, 1);
        }
        assert_eq!((kb.sel_row, kb.sel_col), (0, 0));
    }

    #[test]
    fn consume_modifiers_clears_one_shot_latches() {
        let mut kb = Keyboard::new();
        kb.toggle_shift();
        kb.toggle_alt();
        kb.dirty = false;

        let mods = kb.consume_modifiers();
        assert_eq!(mods, (true, false, true));
        assert_eq!((kb.shift, kb.ctrl, kb.alt), (false, false, false));
        assert!(kb.dirty);
    }

    #[test]
    fn consume_modifiers_is_a_noop_with_nothing_latched() {
        let mut kb = Keyboard::new();
        kb.dirty = false;
        assert_eq!(kb.consume_modifiers(), (false, false, false));
        assert!(!kb.dirty);
    }

    #[test]
    fn repeat_fires_after_delay_then_at_interval() {
        let mut kb = Keyboard::new();
        let t0 = Instant::now();
        kb.start_repeat(0, 1, t0);

        assert_eq!(kb.repeat_timeout_ms(t0), 400);
        assert!(!kb.tick_repeat(t0 + Duration::from_millis(399)));
        assert_eq!(kb.sel_col, 0);

        let t1 = t0 + Duration::from_millis(400);
        assert!(kb.tick_repeat(t1));
        assert_eq!(kb.sel_col, 1);

        assert!(!kb.tick_repeat(t1 + Duration::from_millis(119)));
        assert!(kb.tick_repeat(t1 + Duration::from_millis(120)));
        assert_eq!(kb.sel_col, 2);
    }

    #[test]
    fn stop_repeat_cancels_pending_repeats() {
        let mut kb = Keyboard::new();
        let t0 = Instant::now();
        kb.start_repeat(0, 1, t0);
        kb.stop_repeat();

        assert_eq!(kb.repeat_timeout_ms(t0), -1);
        assert!(!kb.tick_repeat(t0 + Duration::from_secs(10)));
    }
}
