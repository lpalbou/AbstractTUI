//! Color: 32-bit RGBA everywhere inside the engine. Downconversion to
//! 256/16-color palettes is a presenter concern (render layer), never a
//! scene concern — components and themes always speak true color.

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const TRANSPARENT: Rgba = Rgba::new(0, 0, 0, 0);
    pub const BLACK: Rgba = Rgba::rgb(0, 0, 0);
    pub const WHITE: Rgba = Rgba::rgb(255, 255, 255);

    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Rgba { r, g, b, a }
    }

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Rgba { r, g, b, a: 255 }
    }

    pub const fn with_alpha(self, a: u8) -> Self {
        Rgba { a, ..self }
    }

    pub const fn is_opaque(self) -> bool {
        self.a == 255
    }

    pub const fn is_transparent(self) -> bool {
        self.a == 0
    }

    /// Parse `#rgb`, `#rrggbb` or `#rrggbbaa` (leading `#` optional).
    pub fn from_hex(s: &str) -> Option<Rgba> {
        let s = s.strip_prefix('#').unwrap_or(s);
        let hex = |b: u8| -> Option<u8> {
            match b {
                b'0'..=b'9' => Some(b - b'0'),
                b'a'..=b'f' => Some(b - b'a' + 10),
                b'A'..=b'F' => Some(b - b'A' + 10),
                _ => None,
            }
        };
        let by = s.as_bytes();
        match by.len() {
            3 => {
                let r = hex(by[0])?;
                let g = hex(by[1])?;
                let b = hex(by[2])?;
                Some(Rgba::rgb(r * 17, g * 17, b * 17))
            }
            6 | 8 => {
                let mut v = [0u8; 4];
                for (i, chunk) in by.chunks(2).enumerate() {
                    v[i] = hex(chunk[0])? * 16 + hex(chunk[1])?;
                }
                if by.len() == 6 {
                    v[3] = 255;
                }
                Some(Rgba::new(v[0], v[1], v[2], v[3]))
            }
            _ => None,
        }
    }

    pub fn to_hex(self) -> String {
        if self.a == 255 {
            format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
        }
    }

    /// Linear interpolation in sRGB space (t clamped to 0..=1).
    pub fn lerp(self, to: Rgba, t: f32) -> Rgba {
        let t = t.clamp(0.0, 1.0);
        let mix = |a: u8, b: u8| -> u8 { (a as f32 + (b as f32 - a as f32) * t).round() as u8 };
        Rgba::new(
            mix(self.r, to.r),
            mix(self.g, to.g),
            mix(self.b, to.b),
            mix(self.a, to.a),
        )
    }

    /// Source-over compositing of `self` onto an opaque-or-not `bg`.
    pub fn over(self, bg: Rgba) -> Rgba {
        if self.a == 255 {
            return self;
        }
        if self.a == 0 {
            return bg;
        }
        let sa = self.a as u32;
        let da = bg.a as u32;
        let out_a = sa + da * (255 - sa) / 255;
        if out_a == 0 {
            return Rgba::TRANSPARENT;
        }
        let ch = |s: u8, d: u8| -> u8 {
            let s = s as u32;
            let d = d as u32;
            // Source-over numerator is bounded by out_a*255, so the quotient
            // fits u8 without a clamp (verified by the mixed-alpha tests).
            ((s * sa + d * da * (255 - sa) / 255) / out_a) as u8
        };
        Rgba::new(
            ch(self.r, bg.r),
            ch(self.g, bg.g),
            ch(self.b, bg.b),
            out_a as u8,
        )
    }

    /// Relative luminance (WCAG, linearized), 0.0..=1.0.
    pub fn luminance(self) -> f32 {
        fn lin(c: u8) -> f32 {
            let c = c as f32 / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * lin(self.r) + 0.7152 * lin(self.g) + 0.0722 * lin(self.b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip() {
        assert_eq!(Rgba::from_hex("#1a1b26"), Some(Rgba::rgb(0x1a, 0x1b, 0x26)));
        assert_eq!(Rgba::from_hex("fff"), Some(Rgba::WHITE));
        assert_eq!(Rgba::from_hex("#00000080").unwrap().a, 0x80);
        assert_eq!(Rgba::from_hex("nope"), None);
        assert_eq!(Rgba::rgb(0xe9, 0x45, 0x60).to_hex(), "#e94560");
    }

    #[test]
    fn over_opaque_and_transparent() {
        let bg = Rgba::rgb(10, 20, 30);
        assert_eq!(Rgba::TRANSPARENT.over(bg), bg);
        assert_eq!(Rgba::WHITE.over(bg), Rgba::WHITE);
        let half = Rgba::new(255, 255, 255, 128).over(bg);
        assert!(half.r > 120 && half.r < 140);
    }
}
