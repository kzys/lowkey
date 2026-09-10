use input_linux::Key;

pub const COLS: usize = 10;
pub const ROWS: usize = 5;

#[derive(Copy, Clone)]
pub struct KeyDef {
    pub label: &'static str,
    pub shifted: &'static str,
    pub code: Key,
}

const fn kd(label: &'static str, shifted: &'static str, code: Key) -> KeyDef {
    KeyDef { label, shifted, code }
}

// The grid is uniform, which keeps both hit testing and navigation trivial.
pub static KEYS: [[KeyDef; COLS]; ROWS] = [
    [
        kd("1", "!", Key::Num1),
        kd("2", "@", Key::Num2),
        kd("3", "#", Key::Num3),
        kd("4", "$", Key::Num4),
        kd("5", "%", Key::Num5),
        kd("6", "^", Key::Num6),
        kd("7", "&", Key::Num7),
        kd("8", "*", Key::Num8),
        kd("9", "(", Key::Num9),
        kd("0", ")", Key::Num0),
    ],
    [
        kd("q", "Q", Key::Q),
        kd("w", "W", Key::W),
        kd("e", "E", Key::E),
        kd("r", "R", Key::R),
        kd("t", "T", Key::T),
        kd("y", "Y", Key::Y),
        kd("u", "U", Key::U),
        kd("i", "I", Key::I),
        kd("o", "O", Key::O),
        kd("p", "P", Key::P),
    ],
    [
        kd("a", "A", Key::A),
        kd("s", "S", Key::S),
        kd("d", "D", Key::D),
        kd("f", "F", Key::F),
        kd("g", "G", Key::G),
        kd("h", "H", Key::H),
        kd("j", "J", Key::J),
        kd("k", "K", Key::K),
        kd("l", "L", Key::L),
        kd(";", ":", Key::Semicolon),
    ],
    [
        kd("z", "Z", Key::Z),
        kd("x", "X", Key::X),
        kd("c", "C", Key::C),
        kd("v", "V", Key::V),
        kd("b", "B", Key::B),
        kd("n", "N", Key::N),
        kd("m", "M", Key::M),
        kd(",", "<", Key::Comma),
        kd(".", ">", Key::Dot),
        kd("/", "?", Key::Slash),
    ],
    [
        kd("Esc", "Esc", Key::Esc),
        kd("Tab", "Tab", Key::Tab),
        kd("-", "_", Key::Minus),
        kd("=", "+", Key::Equal),
        kd("'", "\"", Key::Apostrophe),
        kd("`", "~", Key::Grave),
        kd("\\", "|", Key::Backslash),
        kd("[", "{", Key::LeftBrace),
        kd("]", "}", Key::RightBrace),
        kd("Ent", "Ent", Key::Enter),
    ],
];

// Emitted from pad buttons rather than the grid, so they cost no cells.
pub static EXTRA_KEYS: [Key; 6] = [
    Key::Backspace,
    Key::Space,
    Key::Enter,
    Key::LeftShift,
    Key::LeftCtrl,
    Key::LeftAlt,
];

pub const LEGEND: &str =
    "A type  B back  Y space  X alt  L shift  R ctrl  Start enter  Select quit";

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn grid_dimensions_match_consts() {
        assert_eq!(KEYS.len(), ROWS);
        for row in &KEYS {
            assert_eq!(row.len(), COLS);
        }
    }

    #[test]
    fn every_key_code_is_unique() {
        let mut seen = HashSet::new();
        for row in &KEYS {
            for key in row {
                assert!(seen.insert(key.code), "duplicate code for {:?}", key.label);
            }
        }
    }

    #[test]
    fn number_row_shifts_to_symbols() {
        let expected = ["!", "@", "#", "$", "%", "^", "&", "*", "(", ")"];
        for (key, want) in KEYS[0].iter().zip(expected) {
            assert_eq!(key.shifted, want, "shifted label for {:?}", key.label);
        }
    }

    #[test]
    fn letters_shift_to_uppercase() {
        for key in KEYS[1].iter().chain(KEYS[2].iter()).chain(KEYS[3].iter()) {
            if key.label.len() == 1 && key.label.chars().next().unwrap().is_ascii_alphabetic() {
                assert_eq!(key.shifted, key.label.to_uppercase());
            }
        }
    }

    #[test]
    fn keys_with_no_shift_variant_are_unchanged() {
        assert_eq!(KEYS[4][0].shifted, KEYS[4][0].label); // Esc
        assert_eq!(KEYS[4][1].shifted, KEYS[4][1].label); // Tab
        assert_eq!(KEYS[4][9].shifted, KEYS[4][9].label); // Ent
    }
}
