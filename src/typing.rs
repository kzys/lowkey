use std::fs::{File, OpenOptions};
use std::os::unix::fs::OpenOptionsExt;
use std::time::Duration;

use input_linux::{sys, EventKind, InputId, Key, KeyEvent, KeyState, SynchronizeEvent, UInputHandle};

use crate::keyboard::Keyboard;
use crate::keys::{Cell, EXTRA_KEYS, KEYS};
use crate::util::die;

/// One step of a key tap, in emission order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TapStep {
    Key(Key, bool),
    Sync,
}

/// Builds the event sequence for tapping `key` with the given modifiers held:
/// modifiers down, the key down/sync/up, modifiers up, then a final sync.
/// Consumers rely on that final sync to see a complete report.
pub fn tap_sequence(shift: bool, ctrl: bool, key: Key) -> Vec<TapStep> {
    let mut seq = Vec::with_capacity(6);
    if shift {
        seq.push(TapStep::Key(Key::LeftShift, true));
    }
    if ctrl {
        seq.push(TapStep::Key(Key::LeftCtrl, true));
    }

    seq.push(TapStep::Key(key, true));
    seq.push(TapStep::Sync);
    seq.push(TapStep::Key(key, false));

    if ctrl {
        seq.push(TapStep::Key(Key::LeftCtrl, false));
    }
    if shift {
        seq.push(TapStep::Key(Key::LeftShift, false));
    }
    seq.push(TapStep::Sync);

    seq
}

fn emit_key(uinput: &UInputHandle<File>, key: Key, pressed: bool) {
    let value = if pressed { KeyState::PRESSED } else { KeyState::RELEASED };
    let ev = KeyEvent::new(Default::default(), key, value).into_event();
    if !matches!(uinput.write(std::slice::from_ref(ev.as_raw())), Ok(1)) {
        eprintln!("uinput write: key event failed");
    }
}

fn emit_syn(uinput: &UInputHandle<File>) {
    let ev = SynchronizeEvent::report(Default::default()).into_event();
    if !matches!(uinput.write(std::slice::from_ref(ev.as_raw())), Ok(1)) {
        eprintln!("uinput write: sync event failed");
    }
}

/// Types `key` through uinput with the keyboard's currently held modifiers.
pub fn tap(uinput: &UInputHandle<File>, keyboard: &mut Keyboard, key: Key) {
    let (shift, ctrl) = keyboard.modifiers();
    for step in tap_sequence(shift, ctrl, key) {
        match step {
            TapStep::Key(k, pressed) => emit_key(uinput, k, pressed),
            TapStep::Sync => emit_syn(uinput),
        }
    }
}

pub fn open_uinput() -> UInputHandle<File> {
    let file = OpenOptions::new()
        .write(true)
        .custom_flags(libc::O_NONBLOCK)
        .open("/dev/uinput")
        .unwrap_or_else(|e| die(&format!("open /dev/uinput: {e}")));

    let uinput = UInputHandle::new(file);
    uinput
        .set_evbit(EventKind::Key)
        .unwrap_or_else(|e| die(&format!("UI_SET_EVBIT: {e}")));

    for row in &KEYS {
        for cell in row {
            if let Cell::Key(k) = cell {
                let _ = uinput.set_keybit(k.code);
            }
        }
    }
    for code in EXTRA_KEYS {
        let _ = uinput.set_keybit(code);
    }

    let id = InputId { bustype: sys::BUS_VIRTUAL, vendor: 0x1209, product: 0x0001, version: 0 };
    uinput
        .create(&id, b"gpkbd", 0, &[])
        .unwrap_or_else(|e| die(&format!("UI_DEV_SETUP/UI_DEV_CREATE: {e}")));

    // Give the compositor a moment to notice the device before typing at it.
    std::thread::sleep(Duration::from_millis(300));
    uinput
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_tap_has_no_modifier_steps() {
        assert_eq!(
            tap_sequence(false, false, Key::Q),
            vec![TapStep::Key(Key::Q, true), TapStep::Sync, TapStep::Key(Key::Q, false), TapStep::Sync]
        );
    }

    #[test]
    fn shifted_tap_wraps_key_in_shift_press_and_release() {
        assert_eq!(
            tap_sequence(true, false, Key::Q),
            vec![
                TapStep::Key(Key::LeftShift, true),
                TapStep::Key(Key::Q, true),
                TapStep::Sync,
                TapStep::Key(Key::Q, false),
                TapStep::Key(Key::LeftShift, false),
                TapStep::Sync,
            ]
        );
    }

    #[test]
    fn all_modifiers_press_in_order_and_release_in_reverse() {
        let seq = tap_sequence(true, true, Key::A);
        assert_eq!(
            seq,
            vec![
                TapStep::Key(Key::LeftShift, true),
                TapStep::Key(Key::LeftCtrl, true),
                TapStep::Key(Key::A, true),
                TapStep::Sync,
                TapStep::Key(Key::A, false),
                TapStep::Key(Key::LeftCtrl, false),
                TapStep::Key(Key::LeftShift, false),
                TapStep::Sync,
            ]
        );
    }

    #[test]
    fn tap_reads_keyboards_held_shift_without_clearing_it() {
        let mut kb = Keyboard::new();
        kb.set_shift(true);

        let (shift, ctrl) = kb.modifiers();
        assert_eq!((shift, ctrl), (true, false));
        assert!(kb.shift, "shift is a held modifier: a tap must not clear it");
    }
}
