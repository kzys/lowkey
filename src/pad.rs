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
    ToggleShift,
    ToggleCtrl,
    ToggleAlt,
    Enter,
    Quit,
}

/// Decodes one raw evdev event, or None if it's not one gpkbd acts on.
///
/// B is physically the bottom face button on this device's Nintendo-style
/// layout, so it carries backspace; A confirms and types the selected key.
/// Either shoulder button on a side latches that side's modifier, since the
/// target device has four shoulder buttons instead of two.
pub fn decode(ev: &sys::input_event) -> Option<PadEvent> {
    if ev.type_ as i32 == sys::EV_ABS {
        return match ev.code as i32 {
            c if c == sys::ABS_HAT0X => Some(hat_event(ev.value, |v| (0, v))),
            c if c == sys::ABS_HAT0Y => Some(hat_event(ev.value, |v| (v, 0))),
            _ => None,
        };
    }

    if ev.type_ as i32 != sys::EV_KEY {
        return None;
    }
    let code = ev.code as i32;

    if let Some((dr, dc)) = dpad_dir(code) {
        return match ev.value {
            1 => Some(PadEvent::Move(dr, dc)),
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
    } else if code == sys::BTN_WEST {
        Some(PadEvent::Space)
    } else if code == sys::BTN_NORTH {
        Some(PadEvent::ToggleAlt)
    } else if code == sys::BTN_TL || code == sys::BTN_TL2 {
        Some(PadEvent::ToggleShift)
    } else if code == sys::BTN_TR || code == sys::BTN_TR2 {
        Some(PadEvent::ToggleCtrl)
    } else if code == sys::BTN_START {
        Some(PadEvent::Enter)
    } else if code == sys::BTN_SELECT {
        Some(PadEvent::Quit)
    } else {
        None
    }
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
fn hat_event(value: i32, dir: impl Fn(i32) -> (i32, i32)) -> PadEvent {
    if value == 0 {
        PadEvent::MoveEnd
    } else {
        let (dr, dc) = dir(value.signum());
        PadEvent::Move(dr, dc)
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

pub fn read_pad(pad: &EvdevHandle<File>) -> Vec<PadEvent> {
    let mut evs: [sys::input_event; 32] = unsafe { std::mem::zeroed() };
    let n = match pad.read(&mut evs) {
        Ok(n) => n,
        Err(_) => return Vec::new(),
    };
    evs[..n].iter().filter_map(decode).collect()
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

    #[test]
    fn face_buttons_swap_a_and_b() {
        assert_eq!(decode(&ev(sys::EV_KEY, sys::BTN_SOUTH, 1)), Some(PadEvent::Backspace));
        assert_eq!(decode(&ev(sys::EV_KEY, sys::BTN_EAST, 1)), Some(PadEvent::Type));
        assert_eq!(decode(&ev(sys::EV_KEY, sys::BTN_WEST, 1)), Some(PadEvent::Space));
    }

    #[test]
    fn button_release_is_ignored_for_press_only_buttons() {
        assert_eq!(decode(&ev(sys::EV_KEY, sys::BTN_EAST, 0)), None);
        assert_eq!(decode(&ev(sys::EV_KEY, sys::BTN_SOUTH, 0)), None);
    }

    #[test]
    fn dpad_reports_move_then_move_end() {
        assert_eq!(decode(&ev(sys::EV_KEY, sys::BTN_DPAD_RIGHT, 1)), Some(PadEvent::Move(0, 1)));
        assert_eq!(decode(&ev(sys::EV_KEY, sys::BTN_DPAD_RIGHT, 0)), Some(PadEvent::MoveEnd));
        assert_eq!(decode(&ev(sys::EV_KEY, sys::BTN_DPAD_UP, 1)), Some(PadEvent::Move(-1, 0)));
    }

    #[test]
    fn hat_axes_report_move_then_move_end() {
        assert_eq!(decode(&ev(sys::EV_ABS, sys::ABS_HAT0X, 1)), Some(PadEvent::Move(0, 1)));
        assert_eq!(decode(&ev(sys::EV_ABS, sys::ABS_HAT0X, -1)), Some(PadEvent::Move(0, -1)));
        assert_eq!(decode(&ev(sys::EV_ABS, sys::ABS_HAT0X, 0)), Some(PadEvent::MoveEnd));
        assert_eq!(decode(&ev(sys::EV_ABS, sys::ABS_HAT0Y, 1)), Some(PadEvent::Move(1, 0)));
    }

    #[test]
    fn shoulder_buttons_latch_by_side_not_by_button() {
        for code in [sys::BTN_TL, sys::BTN_TL2] {
            assert_eq!(decode(&ev(sys::EV_KEY, code, 1)), Some(PadEvent::ToggleShift));
        }
        for code in [sys::BTN_TR, sys::BTN_TR2] {
            assert_eq!(decode(&ev(sys::EV_KEY, code, 1)), Some(PadEvent::ToggleCtrl));
        }
    }

    #[test]
    fn north_start_select_map_to_alt_enter_quit() {
        assert_eq!(decode(&ev(sys::EV_KEY, sys::BTN_NORTH, 1)), Some(PadEvent::ToggleAlt));
        assert_eq!(decode(&ev(sys::EV_KEY, sys::BTN_START, 1)), Some(PadEvent::Enter));
        assert_eq!(decode(&ev(sys::EV_KEY, sys::BTN_SELECT, 1)), Some(PadEvent::Quit));
    }

    #[test]
    fn unknown_and_irrelevant_events_decode_to_none() {
        assert_eq!(decode(&ev(sys::EV_KEY, 0x999, 1)), None);
        assert_eq!(decode(&ev(sys::EV_SYN, 0, 0)), None);
    }
}
