use std::time::{Duration, Instant};

use input_linux::Key;

use crate::keys::{COLS, KEYS, ROWS};

const REPEAT_DELAY: Duration = Duration::from_millis(400);
const REPEAT_INTERVAL: Duration = Duration::from_millis(120);

/// A held-button action that repeats until release: a grid move (applied
/// directly by tick_repeat) or a key tap (dispatched by the caller, since
/// typing is I/O this state machine doesn't do itself).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RepeatAction {
    Move(i32, i32),
    Tap(Key),
}

/// Navigation, modifier-latch and held-button-repeat state for the grid.
/// Pure state machine: no I/O, so it's straightforward to unit test.
pub struct Keyboard {
    pub sel_row: usize,
    pub sel_col: usize,
    pub shift: bool,
    pub ctrl: bool,
    pub dirty: bool,
    repeat_action: Option<RepeatAction>,
    repeat_at: Instant,
}

impl Keyboard {
    pub fn new() -> Self {
        Keyboard {
            sel_row: 0,
            sel_col: 0,
            shift: false,
            ctrl: false,
            dirty: false,
            repeat_action: None,
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

    /// Sets shift to the pad's currently held state, the way a real keyboard's
    /// shift key works, rather than toggling.
    pub fn set_shift(&mut self, held: bool) {
        self.shift = held;
        self.dirty = true;
    }

    /// Sets ctrl to the pad's currently held state; see set_shift.
    pub fn set_ctrl(&mut self, held: bool) {
        self.ctrl = held;
        self.dirty = true;
    }

    pub fn latched(&self) -> bool {
        self.shift || self.ctrl
    }

    /// Shift and Ctrl to apply to a tap: real held modifiers, so this just
    /// reads their current pad state rather than consuming anything.
    pub fn modifiers(&self) -> (bool, bool) {
        (self.shift, self.ctrl)
    }

    /// Begins auto-repeat for a just-pressed action.
    pub fn start_repeat(&mut self, action: RepeatAction, now: Instant) {
        self.repeat_action = Some(action);
        self.repeat_at = now + REPEAT_DELAY;
    }

    /// Ends auto-repeat on release, regardless of what action was held.
    pub fn stop_repeat(&mut self) {
        self.repeat_action = None;
    }

    /// The poll timeout (in milliseconds) needed to wake up for the next
    /// repeat, or -1 (block indefinitely) when nothing is held.
    pub fn repeat_timeout_ms(&self, now: Instant) -> i32 {
        match self.repeat_action {
            None => -1,
            Some(_) if self.repeat_at <= now => 0,
            Some(_) => (self.repeat_at - now).as_millis().min(i32::MAX as u128) as i32,
        }
    }

    /// Fires a repeat if one is due, applying a Move directly (it's grid
    /// state this struct already owns) and returning it so the caller can
    /// also react; a Tap is only returned, since typing it is the caller's
    /// job. None if nothing was due.
    pub fn tick_repeat(&mut self, now: Instant) -> Option<RepeatAction> {
        let action = self.repeat_action?;
        if now < self.repeat_at {
            return None;
        }
        self.repeat_at = now + REPEAT_INTERVAL;
        if let RepeatAction::Move(dr, dc) = action {
            self.move_sel(dr, dc);
        }
        Some(action)
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
    fn modifiers_reads_held_state_without_clearing_it() {
        let mut kb = Keyboard::new();
        kb.set_shift(true);
        kb.dirty = false;

        assert_eq!(kb.modifiers(), (true, false));
        assert_eq!((kb.shift, kb.ctrl), (true, false));
        assert!(!kb.dirty);
    }

    #[test]
    fn modifiers_are_false_by_default() {
        let kb = Keyboard::new();
        assert_eq!(kb.modifiers(), (false, false));
    }

    #[test]
    fn repeat_fires_after_delay_then_at_interval() {
        let mut kb = Keyboard::new();
        let t0 = Instant::now();
        kb.start_repeat(RepeatAction::Move(0, 1), t0);

        assert_eq!(kb.repeat_timeout_ms(t0), 400);
        assert_eq!(kb.tick_repeat(t0 + Duration::from_millis(399)), None);
        assert_eq!(kb.sel_col, 0);

        let t1 = t0 + Duration::from_millis(400);
        assert_eq!(kb.tick_repeat(t1), Some(RepeatAction::Move(0, 1)));
        assert_eq!(kb.sel_col, 1);

        assert_eq!(kb.tick_repeat(t1 + Duration::from_millis(119)), None);
        assert_eq!(kb.tick_repeat(t1 + Duration::from_millis(120)), Some(RepeatAction::Move(0, 1)));
        assert_eq!(kb.sel_col, 2);
    }

    #[test]
    fn repeat_of_a_tap_does_not_move_the_selection() {
        let mut kb = Keyboard::new();
        let t0 = Instant::now();
        kb.start_repeat(RepeatAction::Tap(Key::PageDown), t0);

        let t1 = t0 + Duration::from_millis(400);
        assert_eq!(kb.tick_repeat(t1), Some(RepeatAction::Tap(Key::PageDown)));
        assert_eq!((kb.sel_row, kb.sel_col), (0, 0));
    }

    #[test]
    fn stop_repeat_cancels_pending_repeats() {
        let mut kb = Keyboard::new();
        let t0 = Instant::now();
        kb.start_repeat(RepeatAction::Move(0, 1), t0);
        kb.stop_repeat();

        assert_eq!(kb.repeat_timeout_ms(t0), -1);
        assert_eq!(kb.tick_repeat(t0 + Duration::from_secs(10)), None);
    }
}
