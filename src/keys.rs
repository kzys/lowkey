use input_linux::Key;

pub const COLS: usize = 14;
pub const ROWS: usize = 4;

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
// Each row holds the keys a physical QWERTY row holds, including that row's
// trailing punctuation (e.g. `\` ends the qwerty row, not the home row,
// matching an ANSI keyboard), so a cell's position is where you'd expect it
// on a real keyboard. Rows that fall short of the widest row (the qwerty
// row, at 14) run out of real keys before the last column; those cells stay
// empty rather than fake a key that isn't there.
pub static KEYS: [[Option<KeyDef>; COLS]; ROWS] = [
    [
        Some(kd("`", "~", Key::Grave)),
        Some(kd("1", "!", Key::Num1)),
        Some(kd("2", "@", Key::Num2)),
        Some(kd("3", "#", Key::Num3)),
        Some(kd("4", "$", Key::Num4)),
        Some(kd("5", "%", Key::Num5)),
        Some(kd("6", "^", Key::Num6)),
        Some(kd("7", "&", Key::Num7)),
        Some(kd("8", "*", Key::Num8)),
        Some(kd("9", "(", Key::Num9)),
        Some(kd("0", ")", Key::Num0)),
        Some(kd("-", "_", Key::Minus)),
        Some(kd("=", "+", Key::Equal)),
        None,
    ],
    [
        Some(kd("Tab", "Tab", Key::Tab)),
        Some(kd("q", "Q", Key::Q)),
        Some(kd("w", "W", Key::W)),
        Some(kd("e", "E", Key::E)),
        Some(kd("r", "R", Key::R)),
        Some(kd("t", "T", Key::T)),
        Some(kd("y", "Y", Key::Y)),
        Some(kd("u", "U", Key::U)),
        Some(kd("i", "I", Key::I)),
        Some(kd("o", "O", Key::O)),
        Some(kd("p", "P", Key::P)),
        Some(kd("[", "{", Key::LeftBrace)),
        Some(kd("]", "}", Key::RightBrace)),
        Some(kd("\\", "|", Key::Backslash)),
    ],
    [
        Some(kd("a", "A", Key::A)),
        Some(kd("s", "S", Key::S)),
        Some(kd("d", "D", Key::D)),
        Some(kd("f", "F", Key::F)),
        Some(kd("g", "G", Key::G)),
        Some(kd("h", "H", Key::H)),
        Some(kd("j", "J", Key::J)),
        Some(kd("k", "K", Key::K)),
        Some(kd("l", "L", Key::L)),
        Some(kd(";", ":", Key::Semicolon)),
        Some(kd("'", "\"", Key::Apostrophe)),
        Some(kd("Ent", "Ent", Key::Enter)),
        None,
        None,
    ],
    [
        Some(kd("Esc", "Esc", Key::Esc)),
        Some(kd("z", "Z", Key::Z)),
        Some(kd("x", "X", Key::X)),
        Some(kd("c", "C", Key::C)),
        Some(kd("v", "V", Key::V)),
        Some(kd("b", "B", Key::B)),
        Some(kd("n", "N", Key::N)),
        Some(kd("m", "M", Key::M)),
        Some(kd(",", "<", Key::Comma)),
        Some(kd(".", ">", Key::Dot)),
        Some(kd("/", "?", Key::Slash)),
        None,
        None,
        None,
    ],
];

// Emitted from pad buttons rather than the grid, so they cost no cells.
pub static EXTRA_KEYS: [Key; 7] = [
    Key::Backspace,
    Key::Space,
    Key::Enter,
    Key::LeftShift,
    Key::LeftCtrl,
    Key::PageUp,
    Key::PageDown,
];

pub const LEGEND: &str =
    "A type  B back  Y space  X/Start enter  L shift  R ctrl  Select quit";

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
            for key in row.iter().flatten() {
                assert!(seen.insert(key.code), "duplicate code for {:?}", key.label);
            }
        }
    }

    #[test]
    fn digit_row_shifts_to_symbols() {
        let expected = ["!", "@", "#", "$", "%", "^", "&", "*", "(", ")"];
        // Column 0 is the backtick; digits run from column 1.
        for (key, want) in KEYS[0][1..11].iter().zip(expected) {
            let key = key.unwrap();
            assert_eq!(key.shifted, want, "shifted label for {:?}", key.label);
        }
    }

    #[test]
    fn letters_shift_to_uppercase() {
        for key in KEYS.iter().flatten().flatten() {
            if key.label.len() == 1 && key.label.chars().next().unwrap().is_ascii_alphabetic() {
                assert_eq!(key.shifted, key.label.to_uppercase());
            }
        }
    }

    #[test]
    fn keys_with_no_shift_variant_are_unchanged() {
        assert_eq!(KEYS[1][0].unwrap().shifted, KEYS[1][0].unwrap().label); // Tab
        assert_eq!(KEYS[2][11].unwrap().shifted, KEYS[2][11].unwrap().label); // Ent
        assert_eq!(KEYS[3][0].unwrap().shifted, KEYS[3][0].unwrap().label); // Esc
    }

    #[test]
    fn rows_shorter_than_the_qwerty_row_end_in_empty_cells() {
        assert!(KEYS[0][13].is_none()); // digit row: 13 real keys
        assert!(KEYS[1].iter().all(Option::is_some)); // qwerty row: full, 14 keys
        assert!(KEYS[2][12].is_none()); // home row: 12 real keys
        assert!(KEYS[2][13].is_none());
        assert!(KEYS[3][11].is_none()); // bottom row: 11 real keys
        assert!(KEYS[3][12].is_none());
        assert!(KEYS[3][13].is_none());
    }
}
