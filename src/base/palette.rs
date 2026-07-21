//! Canonical xterm palette tables — the ONE source both the presenter's
//! downlevel quantization and the testing VT model must use (two
//! hand-typed copies of this table will drift, and then the diff/present
//! property test lies in both directions — REDTEAM finding RT1-7).

use super::color::Rgba;

/// The 16 xterm system colors (indices 0..=15), default xterm values.
pub const SYSTEM_16: [Rgba; 16] = [
    Rgba::rgb(0x00, 0x00, 0x00), // 0  black
    Rgba::rgb(0x80, 0x00, 0x00), // 1  red
    Rgba::rgb(0x00, 0x80, 0x00), // 2  green
    Rgba::rgb(0x80, 0x80, 0x00), // 3  yellow
    Rgba::rgb(0x00, 0x00, 0x80), // 4  blue
    Rgba::rgb(0x80, 0x00, 0x80), // 5  magenta
    Rgba::rgb(0x00, 0x80, 0x80), // 6  cyan
    Rgba::rgb(0xc0, 0xc0, 0xc0), // 7  white (light gray)
    Rgba::rgb(0x80, 0x80, 0x80), // 8  bright black
    Rgba::rgb(0xff, 0x00, 0x00), // 9  bright red
    Rgba::rgb(0x00, 0xff, 0x00), // 10 bright green
    Rgba::rgb(0xff, 0xff, 0x00), // 11 bright yellow
    Rgba::rgb(0x00, 0x00, 0xff), // 12 bright blue
    Rgba::rgb(0xff, 0x00, 0xff), // 13 bright magenta
    Rgba::rgb(0x00, 0xff, 0xff), // 14 bright cyan
    Rgba::rgb(0xff, 0xff, 0xff), // 15 bright white
];

/// Level values of the 6x6x6 color cube (indices 16..=231).
pub const CUBE_LEVELS: [u8; 6] = [0x00, 0x5f, 0x87, 0xaf, 0xd7, 0xff];

/// Resolve any xterm-256 index to its RGB value.
pub const fn xterm_256(index: u8) -> Rgba {
    match index {
        0..=15 => SYSTEM_16[index as usize],
        16..=231 => {
            let i = index as usize - 16;
            let r = CUBE_LEVELS[i / 36];
            let g = CUBE_LEVELS[(i / 6) % 6];
            let b = CUBE_LEVELS[i % 6];
            Rgba::rgb(r, g, b)
        }
        232..=255 => {
            let v = 8 + (index as u16 - 232) * 10;
            Rgba::rgb(v as u8, v as u8, v as u8)
        }
    }
}

/// The full 256-entry table, for consumers that prefer a slice.
pub static XTERM_256: [Rgba; 256] = {
    let mut t = [Rgba::BLACK; 256];
    let mut i = 0usize;
    while i < 256 {
        t[i] = xterm_256(i as u8);
        i += 1;
    }
    t
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spot_values() {
        assert_eq!(xterm_256(1), Rgba::rgb(0x80, 0, 0));
        assert_eq!(xterm_256(16), Rgba::rgb(0, 0, 0));
        assert_eq!(xterm_256(21), Rgba::rgb(0, 0, 0xff));
        assert_eq!(xterm_256(196), Rgba::rgb(0xff, 0, 0));
        assert_eq!(xterm_256(232), Rgba::rgb(8, 8, 8));
        assert_eq!(xterm_256(255), Rgba::rgb(238, 238, 238));
        assert_eq!(XTERM_256[196], xterm_256(196));
    }
}
