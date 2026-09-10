use std::fs::{File, OpenOptions};
use std::os::unix::fs::OpenOptionsExt;

use input_linux::{sys, EvdevHandle, Key};

/// A pad input translated into what the keyboard cares about, independent of
/// which physical button or axis produced it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadEvent {
    /// A direction was pressed (D-pad button or hat axis moving off center).
    Move(i32, i32),
    /// The held direction was released; auto-repeat should stop.
    MoveEnd,
    Type,
    Backspace,
    Space,
    /// Shift's held state changed to this (either left shoulder button).
    Shift(bool),
    /// Ctrl's held state changed to this (R1).
    Ctrl(bool),
    PageUp,
    PageDown,
    Enter,
    Quit,
}

/// Cross-event pad state that a single evdev event can't carry by itself:
/// which held buttons are currently down, since Shift/Ctrl/R2 all act as
/// held chords rather than one-shot presses.
#[derive(Default)]
pub struct PadState {
    tl_held: bool,
    tl2_held: bool,
    tr_held: bool,
    r2_held: bool,
}

impl PadState {
    pub fn new() -> Self {
        PadState::default()
    }
}

/// Decodes one raw evdev event, or None if it's not one gpkbd acts on.
///
/// B is physically the bottom face button on this device's Nintendo-style
/// layout, so it carries backspace; A confirms and types the selected key.
/// This driver's BTN_WEST/BTN_NORTH are swapped from the standard
/// Nintendo-layout convention: physical X reports BTN_WEST and physical Y
/// reports BTN_NORTH (confirmed with an evdev capture against a real H700
/// Gamepad), so X carries enter (alongside Start) and Y carries space.
/// Either left shoulder button holds shift, since the target device has
/// four shoulder buttons instead of two. R1 holds ctrl the same way; R2 is
/// reserved as a held chord for D-pad up/down to page the focused surface.
pub fn decode(state: &mut PadState, ev: &sys::input_event) -> Option<PadEvent> {
    if ev.type_ as i32 == sys::EV_ABS {
        return match ev.code as i32 {
            c if c == sys::ABS_HAT0X => Some(hat_event(state, ev.value, |v| (0, v))),
            c if c == sys::ABS_HAT0Y => Some(hat_event(state, ev.value, |v| (v, 0))),
            _ => None,
        };
    }

    if ev.type_ as i32 != sys::EV_KEY {
        return None;
    }
    let code = ev.code as i32;
    let held = ev.value != 0;

    if code == sys::BTN_TR2 {
        state.r2_held = held;
        return None;
    }
    if code == sys::BTN_TL || code == sys::BTN_TL2 {
        if code == sys::BTN_TL {
            state.tl_held = held;
        } else {
            state.tl2_held = held;
        }
        return Some(PadEvent::Shift(state.tl_held || state.tl2_held));
    }
    if code == sys::BTN_TR {
        state.tr_held = held;
        return Some(PadEvent::Ctrl(state.tr_held));
    }

    if let Some((dr, dc)) = dpad_dir(code) {
        return match ev.value {
            1 => Some(dir_event(state, dr, dc)),
            0 => Some(PadEvent::MoveEnd),
            _ => None,
        };
    }

    if ev.value != 1 {
        return None;
    }

    if code == sys::BTN_SOUTH {
        Some(PadEvent::Backspace)
    } else if code == sys::BTN_EAST {
        Some(PadEvent::Type)
    } else if code == sys::BTN_NORTH {
        Some(PadEvent::Space)
    } else if code == sys::BTN_WEST || code == sys::BTN_START {
        Some(PadEvent::Enter)
    } else if code == sys::BTN_SELECT {
        Some(PadEvent::Quit)
    } else {
        None
    }
}

/// Turns a direction into a page turn when R2 is held and the direction is
/// vertical, otherwise a plain grid move.
fn dir_event(state: &PadState, dr: i32, dc: i32) -> PadEvent {
    if state.r2_held && dc == 0 {
        if dr < 0 {
            return PadEvent::PageUp;
        } else if dr > 0 {
            return PadEvent::PageDown;
        }
    }
    PadEvent::Move(dr, dc)
}

fn dpad_dir(code: i32) -> Option<(i32, i32)> {
    if code == sys::BTN_DPAD_LEFT {
        Some((0, -1))
    } else if code == sys::BTN_DPAD_RIGHT {
        Some((0, 1))
    } else if code == sys::BTN_DPAD_UP {
        Some((-1, 0))
    } else if code == sys::BTN_DPAD_DOWN {
        Some((1, 0))
    } else {
        None
    }
}

// A hat reports -1, 0 or 1; act on the press and the return to center.
fn hat_event(state: &PadState, value: i32, dir: impl Fn(i32) -> (i32, i32)) -> PadEvent {
    if value == 0 {
        PadEvent::MoveEnd
    } else {
        let (dr, dc) = dir(value.signum());
        dir_event(state, dr, dc)
    }
}

// open_pad finds an evdev node whose name contains want, or the first node
// advertising a gamepad button when want is None.
pub fn open_pad(want: Option<&str>) -> Option<EvdevHandle<File>> {
    for i in 0..32 {
        let path = format!("/dev/input/event{i}");
        let file = match OpenOptions::new().read(true).custom_flags(libc::O_NONBLOCK).open(&path) {
            Ok(f) => f,
            Err(_) => continue,
        };
        let handle = EvdevHandle::new(file);

        let Ok(raw_name) = handle.device_name() else {
            continue;
        };
        let name_bytes = raw_name.split(|&b| b == 0).next().unwrap_or(&[]);
        let name = String::from_utf8_lossy(name_bytes);

        let found = match want {
            Some(w) => name.contains(w),
            None => handle.key_bits().map(|b| b.get(Key::ButtonSouth)).unwrap_or(false),
        };
        if found {
            eprintln!("pad: {name} ({path})");
            return Some(handle);
        }
    }
    None
}

pub fn read_pad(pad: &EvdevHandle<File>, state: &mut PadState) -> Vec<PadEvent> {
    let mut evs: [sys::input_event; 32] = unsafe { std::mem::zeroed() };
    let n = match pad.read(&mut evs) {
        Ok(n) => n,
        Err(_) => return Vec::new(),
    };
    evs[..n].iter().filter_map(|ev| decode(state, ev)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(type_: i32, code: i32, value: i32) -> sys::input_event {
        let mut ev: sys::input_event = unsafe { std::mem::zeroed() };
        ev.type_ = type_ as u16;
        ev.code = code as u16;
        ev.value = value;
        ev
    }

    fn decode1(ev: &sys::input_event) -> Option<PadEvent> {
        decode(&mut PadState::new(), ev)
    }

    #[test]
    fn face_buttons_swap_a_and_b() {
        assert_eq!(decode1(&ev(sys::EV_KEY, sys::BTN_SOUTH, 1)), Some(PadEvent::Backspace));
        assert_eq!(decode1(&ev(sys::EV_KEY, sys::BTN_EAST, 1)), Some(PadEvent::Type));
        // This driver's BTN_NORTH/BTN_WEST are swapped from the physical Y/X
        // labels; see decode's doc comment.
        assert_eq!(decode1(&ev(sys::EV_KEY, sys::BTN_NORTH, 1)), Some(PadEvent::Space));
    }

    #[test]
    fn button_release_is_ignored_for_press_only_buttons() {
        assert_eq!(decode1(&ev(sys::EV_KEY, sys::BTN_EAST, 0)), None);
        assert_eq!(decode1(&ev(sys::EV_KEY, sys::BTN_SOUTH, 0)), None);
    }

    #[test]
    fn dpad_reports_move_then_move_end() {
        assert_eq!(decode1(&ev(sys::EV_KEY, sys::BTN_DPAD_RIGHT, 1)), Some(PadEvent::Move(0, 1)));
        assert_eq!(decode1(&ev(sys::EV_KEY, sys::BTN_DPAD_RIGHT, 0)), Some(PadEvent::MoveEnd));
        assert_eq!(decode1(&ev(sys::EV_KEY, sys::BTN_DPAD_UP, 1)), Some(PadEvent::Move(-1, 0)));
    }

    #[test]
    fn hat_axes_report_move_then_move_end() {
        assert_eq!(decode1(&ev(sys::EV_ABS, sys::ABS_HAT0X, 1)), Some(PadEvent::Move(0, 1)));
        assert_eq!(decode1(&ev(sys::EV_ABS, sys::ABS_HAT0X, -1)), Some(PadEvent::Move(0, -1)));
        assert_eq!(decode1(&ev(sys::EV_ABS, sys::ABS_HAT0X, 0)), Some(PadEvent::MoveEnd));
        assert_eq!(decode1(&ev(sys::EV_ABS, sys::ABS_HAT0Y, 1)), Some(PadEvent::Move(1, 0)));
    }

    #[test]
    fn either_left_shoulder_button_holds_shift() {
        for code in [sys::BTN_TL, sys::BTN_TL2] {
            assert_eq!(decode1(&ev(sys::EV_KEY, code, 1)), Some(PadEvent::Shift(true)));
            assert_eq!(decode1(&ev(sys::EV_KEY, code, 0)), Some(PadEvent::Shift(false)));
        }
    }

    #[test]
    fn shift_stays_held_until_both_left_shoulders_release() {
        let mut state = PadState::new();
        assert_eq!(decode(&mut state, &ev(sys::EV_KEY, sys::BTN_TL, 1)), Some(PadEvent::Shift(true)));
        assert_eq!(decode(&mut state, &ev(sys::EV_KEY, sys::BTN_TL2, 1)), Some(PadEvent::Shift(true)));
        // Releasing just one of the two still-held shoulder buttons keeps shift held.
        assert_eq!(decode(&mut state, &ev(sys::EV_KEY, sys::BTN_TL, 0)), Some(PadEvent::Shift(true)));
        assert_eq!(decode(&mut state, &ev(sys::EV_KEY, sys::BTN_TL2, 0)), Some(PadEvent::Shift(false)));
    }

    #[test]
    fn r1_holds_ctrl_r2_does_not() {
        assert_eq!(decode1(&ev(sys::EV_KEY, sys::BTN_TR, 1)), Some(PadEvent::Ctrl(true)));
        assert_eq!(decode1(&ev(sys::EV_KEY, sys::BTN_TR, 0)), Some(PadEvent::Ctrl(false)));
        assert_eq!(decode1(&ev(sys::EV_KEY, sys::BTN_TR2, 1)), None);
        assert_eq!(decode1(&ev(sys::EV_KEY, sys::BTN_TR2, 0)), None);
    }

    #[test]
    fn r2_held_turns_vertical_dpad_into_page_turns() {
        let mut state = PadState::new();
        assert_eq!(decode(&mut state, &ev(sys::EV_KEY, sys::BTN_TR2, 1)), None);

        assert_eq!(decode(&mut state, &ev(sys::EV_KEY, sys::BTN_DPAD_UP, 1)), Some(PadEvent::PageUp));
        assert_eq!(decode(&mut state, &ev(sys::EV_KEY, sys::BTN_DPAD_DOWN, 1)), Some(PadEvent::PageDown));
        // Horizontal directions are unaffected by the R2 chord.
        assert_eq!(decode(&mut state, &ev(sys::EV_KEY, sys::BTN_DPAD_LEFT, 1)), Some(PadEvent::Move(0, -1)));

        assert_eq!(decode(&mut state, &ev(sys::EV_KEY, sys::BTN_TR2, 0)), None);
        assert_eq!(decode(&mut state, &ev(sys::EV_KEY, sys::BTN_DPAD_UP, 1)), Some(PadEvent::Move(-1, 0)));
    }

    #[test]
    fn r2_held_turns_hat_vertical_into_page_turns() {
        let mut state = PadState::new();
        decode(&mut state, &ev(sys::EV_KEY, sys::BTN_TR2, 1));

        assert_eq!(decode(&mut state, &ev(sys::EV_ABS, sys::ABS_HAT0Y, -1)), Some(PadEvent::PageUp));
        assert_eq!(decode(&mut state, &ev(sys::EV_ABS, sys::ABS_HAT0Y, 1)), Some(PadEvent::PageDown));
        assert_eq!(decode(&mut state, &ev(sys::EV_ABS, sys::ABS_HAT0X, 1)), Some(PadEvent::Move(0, 1)));
    }

    #[test]
    fn west_and_start_both_map_to_enter_select_quits() {
        assert_eq!(decode1(&ev(sys::EV_KEY, sys::BTN_WEST, 1)), Some(PadEvent::Enter));
        assert_eq!(decode1(&ev(sys::EV_KEY, sys::BTN_START, 1)), Some(PadEvent::Enter));
        assert_eq!(decode1(&ev(sys::EV_KEY, sys::BTN_SELECT, 1)), Some(PadEvent::Quit));
    }

    #[test]
    fn unknown_and_irrelevant_events_decode_to_none() {
        assert_eq!(decode1(&ev(sys::EV_KEY, 0x999, 1)), None);
        assert_eq!(decode1(&ev(sys::EV_SYN, 0, 0)), None);
    }
}
