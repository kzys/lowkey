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

/// One grid cell: a typeable key, an empty gap, or a chord indicator (Shift,
/// Ctrl, R1's arrow-key chord, R2's page chord). Indicators aren't typeable
/// — their chord is held through a pad button, not the grid — they just
/// show whether that chord is currently held.
#[derive(Copy, Clone)]
pub enum Cell {
    Empty,
    Key(KeyDef),
    Shift,
    Ctrl,
    R1,
    R2,
}

const fn key(label: &'static str, shifted: &'static str, code: Key) -> Cell {
    Cell::Key(kd(label, shifted, code))
}

// The grid is uniform, which keeps both hit testing and navigation trivial.
// Each row holds the keys a physical QWERTY row holds, including that row's
// trailing punctuation (e.g. `\` ends the qwerty row, not the home row,
// matching an ANSI keyboard), so a cell's position is where you'd expect it
// on a real keyboard. The home and bottom rows lead with a Ctrl/Shift
// indicator rather than a real key, which — like the backtick/Tab/Esc
// leading the other rows — lines up 1/Q/A/Z (and every column after) in the
// same column across all four rows. Rows that fall short of the widest row
// (the qwerty row, at 14) run out of real keys before the last column;
// those cells stay empty rather than fake a key that isn't there, except
// the bottom row's last two cells, which hold R2 then R1 — read left to
// right in that order to match reaching across the shoulder from R2 (the
// far button) to R1 (the near one).
pub static KEYS: [[Cell; COLS]; ROWS] = [
    [
        key("`", "~", Key::Grave),
        key("1", "!", Key::Num1),
        key("2", "@", Key::Num2),
        key("3", "#", Key::Num3),
        key("4", "$", Key::Num4),
        key("5", "%", Key::Num5),
        key("6", "^", Key::Num6),
        key("7", "&", Key::Num7),
        key("8", "*", Key::Num8),
        key("9", "(", Key::Num9),
        key("0", ")", Key::Num0),
        key("-", "_", Key::Minus),
        key("=", "+", Key::Equal),
        key("Esc", "Esc", Key::Esc),
    ],
    [
        key("Tab", "Tab", Key::Tab),
        key("q", "Q", Key::Q),
        key("w", "W", Key::W),
        key("e", "E", Key::E),
        key("r", "R", Key::R),
        key("t", "T", Key::T),
        key("y", "Y", Key::Y),
        key("u", "U", Key::U),
        key("i", "I", Key::I),
        key("o", "O", Key::O),
        key("p", "P", Key::P),
        key("[", "{", Key::LeftBrace),
        key("]", "}", Key::RightBrace),
        key("\\", "|", Key::Backslash),
    ],
    [
        Cell::Ctrl,
        key("a", "A", Key::A),
        key("s", "S", Key::S),
        key("d", "D", Key::D),
        key("f", "F", Key::F),
        key("g", "G", Key::G),
        key("h", "H", Key::H),
        key("j", "J", Key::J),
        key("k", "K", Key::K),
        key("l", "L", Key::L),
        key(";", ":", Key::Semicolon),
        key("'", "\"", Key::Apostrophe),
        key("Enter", "Enter", Key::Enter),
        Cell::Empty,
    ],
    [
        Cell::Shift,
        key("z", "Z", Key::Z),
        key("x", "X", Key::X),
        key("c", "C", Key::C),
        key("v", "V", Key::V),
        key("b", "B", Key::B),
        key("n", "N", Key::N),
        key("m", "M", Key::M),
        key(",", "<", Key::Comma),
        key(".", ">", Key::Dot),
        key("/", "?", Key::Slash),
        Cell::Empty,
        Cell::R2,
        Cell::R1,
    ],
];

// Emitted from pad buttons rather than the grid, so they cost no cells.
pub static EXTRA_KEYS: [Key; 11] = [
    Key::Backspace,
    Key::Space,
    Key::Enter,
    Key::LeftShift,
    Key::LeftCtrl,
    Key::PageUp,
    Key::PageDown,
    Key::Up,
    Key::Down,
    Key::Left,
    Key::Right,
];

pub const LEGEND: &str =
    "A type  B back  Y space  X/Start enter  L shift  R ctrl  Select quit";

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn key_at(r: usize, c: usize) -> KeyDef {
        match KEYS[r][c] {
            Cell::Key(k) => k,
            _ => panic!("KEYS[{r}][{c}] is not a Key cell"),
        }
    }

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
            for cell in row {
                if let Cell::Key(k) = cell {
                    assert!(seen.insert(k.code), "duplicate code for {:?}", k.label);
                }
            }
        }
    }

    #[test]
    fn digit_row_shifts_to_symbols() {
        let expected = ["!", "@", "#", "$", "%", "^", "&", "*", "(", ")"];
        // Column 0 is the backtick; digits run from column 1.
        for (i, want) in expected.iter().enumerate() {
            assert_eq!(key_at(0, i + 1).shifted, *want, "shifted label for column {}", i + 1);
        }
    }

    #[test]
    fn letters_shift_to_uppercase() {
        for row in &KEYS {
            for cell in row {
                if let Cell::Key(k) = cell {
                    if k.label.len() == 1 && k.label.chars().next().unwrap().is_ascii_alphabetic() {
                        assert_eq!(k.shifted, k.label.to_uppercase());
                    }
                }
            }
        }
    }

    #[test]
    fn keys_with_no_shift_variant_are_unchanged() {
        assert_eq!(key_at(0, 13).shifted, key_at(0, 13).label); // Esc
        assert_eq!(key_at(1, 0).shifted, key_at(1, 0).label); // Tab
        assert_eq!(key_at(2, 12).shifted, key_at(2, 12).label); // Ent
    }

    #[test]
    fn rows_shorter_than_the_qwerty_row_end_in_empty_cells() {
        assert!(matches!(KEYS[0][13], Cell::Key(_))); // digit row: full, Esc at the end
        assert!(KEYS[1].iter().all(|c| matches!(c, Cell::Key(_)))); // qwerty row: full
        assert!(matches!(KEYS[2][13], Cell::Empty)); // home row: leading Ctrl, no trailing indicator
        assert!(matches!(KEYS[3][11], Cell::Empty)); // bottom row: leading Shift, trailing R2 then R1
        assert!(matches!(KEYS[3][12], Cell::R2));
        assert!(matches!(KEYS[3][13], Cell::R1));
    }

    #[test]
    fn one_two_q_a_z_line_up_in_the_same_columns() {
        assert!(matches!(KEYS[0][0], Cell::Key(k) if k.label == "`"));
        assert!(matches!(KEYS[1][0], Cell::Key(k) if k.label == "Tab"));
        assert!(matches!(KEYS[2][0], Cell::Ctrl));
        assert!(matches!(KEYS[3][0], Cell::Shift));

        assert_eq!(key_at(0, 1).label, "1");
        assert_eq!(key_at(1, 1).label, "q");
        assert_eq!(key_at(2, 1).label, "a");
        assert_eq!(key_at(3, 1).label, "z");
    }

    #[test]
    fn esc_ends_the_digit_row() {
        assert_eq!(key_at(0, 13).label, "Esc");
    }
}
