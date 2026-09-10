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
    /// Shift's held state changed to this (L1).
    Shift(bool),
    /// Ctrl's held state changed to this (L2).
    Ctrl(bool),
    /// An arrow key tap, from a D-pad/hat press while R1 is held.
    Arrow(Key),
    /// R1's held state changed to this (arrow-key chord).
    R1(bool),
    /// R2's held state changed to this (page chord).
    R2(bool),
    PageUp,
    PageDown,
    Enter,
    Quit,
}

/// Cross-event pad state that a single evdev event can't carry by itself:
/// which held buttons are currently down, since Shift/Ctrl/R1/R2 all act as
/// held chords rather than one-shot presses.
#[derive(Default)]
pub struct PadState {
    tl_held: bool,
    tl2_held: bool,
    r1_held: bool,
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
/// Gamepad), so X carries enter and Y carries space. Start is unmapped.
/// L1 holds shift, L2 holds ctrl — grouped onto the left shoulder since the
/// grid shows their indicator cells together on the left. R1 and R2 are
/// both held chords over the D-pad/hat: R1 turns a direction into an arrow
/// key (so the grid selection doesn't move while you're just moving a text
/// cursor), R2 turns up/down into a page turn of the focused surface.
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
        return Some(PadEvent::R2(held));
    }
    if code == sys::BTN_TR {
        state.r1_held = held;
        return Some(PadEvent::R1(held));
    }
    if code == sys::BTN_TL {
        state.tl_held = held;
        return Some(PadEvent::Shift(state.tl_held));
    }
    if code == sys::BTN_TL2 {
        state.tl2_held = held;
        return Some(PadEvent::Ctrl(state.tl2_held));
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
    } else if code == sys::BTN_WEST {
        Some(PadEvent::Enter)
    } else if code == sys::BTN_SELECT {
        Some(PadEvent::Quit)
    } else {
        None
    }
}

/// Turns a direction into a page turn when R2 is held and the direction is
/// vertical, an arrow key when R1 is held, otherwise a plain grid move. R2
/// takes priority over R1 if both happen to be held.
fn dir_event(state: &PadState, dr: i32, dc: i32) -> PadEvent {
    if state.r2_held && dc == 0 {
        if dr < 0 {
            return PadEvent::PageUp;
        } else if dr > 0 {
            return PadEvent::PageDown;
        }
    }
    if state.r1_held {
        let key = match (dr, dc) {
            (d, 0) if d < 0 => Key::Up,
            (d, 0) if d > 0 => Key::Down,
            (0, d) if d < 0 => Key::Left,
            (0, d) if d > 0 => Key::Right,
            _ => return PadEvent::Move(dr, dc),
        };
        return PadEvent::Arrow(key);
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
    fn l1_holds_shift() {
        assert_eq!(decode1(&ev(sys::EV_KEY, sys::BTN_TL, 1)), Some(PadEvent::Shift(true)));
        assert_eq!(decode1(&ev(sys::EV_KEY, sys::BTN_TL, 0)), Some(PadEvent::Shift(false)));
    }

    #[test]
    fn l2_holds_ctrl() {
        assert_eq!(decode1(&ev(sys::EV_KEY, sys::BTN_TL2, 1)), Some(PadEvent::Ctrl(true)));
        assert_eq!(decode1(&ev(sys::EV_KEY, sys::BTN_TL2, 0)), Some(PadEvent::Ctrl(false)));
    }

    #[test]
    fn r1_and_r2_do_not_hold_ctrl() {
        for code in [sys::BTN_TR, sys::BTN_TR2] {
            assert!(!matches!(decode1(&ev(sys::EV_KEY, code, 1)), Some(PadEvent::Ctrl(_))));
        }
    }

    #[test]
    fn r1_and_r2_report_their_own_held_state() {
        assert_eq!(decode1(&ev(sys::EV_KEY, sys::BTN_TR, 1)), Some(PadEvent::R1(true)));
        assert_eq!(decode1(&ev(sys::EV_KEY, sys::BTN_TR, 0)), Some(PadEvent::R1(false)));
        assert_eq!(decode1(&ev(sys::EV_KEY, sys::BTN_TR2, 1)), Some(PadEvent::R2(true)));
        assert_eq!(decode1(&ev(sys::EV_KEY, sys::BTN_TR2, 0)), Some(PadEvent::R2(false)));
    }

    #[test]
    fn r2_held_turns_vertical_dpad_into_page_turns() {
        let mut state = PadState::new();
        assert_eq!(decode(&mut state, &ev(sys::EV_KEY, sys::BTN_TR2, 1)), Some(PadEvent::R2(true)));

        assert_eq!(decode(&mut state, &ev(sys::EV_KEY, sys::BTN_DPAD_UP, 1)), Some(PadEvent::PageUp));
        assert_eq!(decode(&mut state, &ev(sys::EV_KEY, sys::BTN_DPAD_DOWN, 1)), Some(PadEvent::PageDown));
        // Horizontal directions are unaffected by the R2 chord.
        assert_eq!(decode(&mut state, &ev(sys::EV_KEY, sys::BTN_DPAD_LEFT, 1)), Some(PadEvent::Move(0, -1)));

        assert_eq!(decode(&mut state, &ev(sys::EV_KEY, sys::BTN_TR2, 0)), Some(PadEvent::R2(false)));
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
    fn r1_held_turns_dpad_into_arrow_keys() {
        let mut state = PadState::new();
        assert_eq!(decode(&mut state, &ev(sys::EV_KEY, sys::BTN_TR, 1)), Some(PadEvent::R1(true)));

        assert_eq!(decode(&mut state, &ev(sys::EV_KEY, sys::BTN_DPAD_UP, 1)), Some(PadEvent::Arrow(Key::Up)));
        assert_eq!(decode(&mut state, &ev(sys::EV_KEY, sys::BTN_DPAD_DOWN, 1)), Some(PadEvent::Arrow(Key::Down)));
        assert_eq!(decode(&mut state, &ev(sys::EV_KEY, sys::BTN_DPAD_LEFT, 1)), Some(PadEvent::Arrow(Key::Left)));
        assert_eq!(decode(&mut state, &ev(sys::EV_KEY, sys::BTN_DPAD_RIGHT, 1)), Some(PadEvent::Arrow(Key::Right)));

        assert_eq!(decode(&mut state, &ev(sys::EV_KEY, sys::BTN_TR, 0)), Some(PadEvent::R1(false)));
        assert_eq!(decode(&mut state, &ev(sys::EV_KEY, sys::BTN_DPAD_UP, 1)), Some(PadEvent::Move(-1, 0)));
    }

    #[test]
    fn r2_takes_priority_over_r1_when_both_held() {
        let mut state = PadState::new();
        decode(&mut state, &ev(sys::EV_KEY, sys::BTN_TR, 1));
        decode(&mut state, &ev(sys::EV_KEY, sys::BTN_TR2, 1));

        assert_eq!(decode(&mut state, &ev(sys::EV_KEY, sys::BTN_DPAD_UP, 1)), Some(PadEvent::PageUp));
    }

    #[test]
    fn west_maps_to_enter_select_quits() {
        assert_eq!(decode1(&ev(sys::EV_KEY, sys::BTN_WEST, 1)), Some(PadEvent::Enter));
        assert_eq!(decode1(&ev(sys::EV_KEY, sys::BTN_SELECT, 1)), Some(PadEvent::Quit));
    }

    #[test]
    fn start_is_unmapped() {
        assert_eq!(decode1(&ev(sys::EV_KEY, sys::BTN_START, 1)), None);
    }

    #[test]
    fn unknown_and_irrelevant_events_decode_to_none() {
        assert_eq!(decode1(&ev(sys::EV_KEY, 0x999, 1)), None);
        assert_eq!(decode1(&ev(sys::EV_SYN, 0, 0)), None);
    }
}
