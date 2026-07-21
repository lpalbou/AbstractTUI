//! Text attribute bitset — split from `cell.rs` (file-size budget);
//! logically part of the cell model and re-exported through it
//! (`render::cell::Attrs` / `render::Attrs` are unchanged paths).

/// Text attribute bitset (hand-rolled to avoid a bitflags dependency).
///
/// Bit order is also the deterministic SGR emission order used by the
/// presenter; keep declaration order aligned with ascending SGR codes.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Default)]
pub struct Attrs(u16);

impl Attrs {
    /// No attributes set.
    pub const NONE: Attrs = Attrs(0);
    /// SGR 1. Terminals may render as brighter ink instead of a heavier face.
    pub const BOLD: Attrs = Attrs(1 << 0);
    /// SGR 2 (faint). Combines with BOLD at the terminal's discretion.
    pub const DIM: Attrs = Attrs(1 << 1);
    /// SGR 3.
    pub const ITALIC: Attrs = Attrs(1 << 2);
    /// SGR 4. Color it with [`Style::underline_color`](super::style::Style::underline_color).
    pub const UNDERLINE: Attrs = Attrs(1 << 3);
    /// SGR 4:3 (curly). Degrades to UNDERLINE when unsupported (caps-driven).
    pub const UNDERCURL: Attrs = Attrs(1 << 4);
    /// SGR 5. Widely ignored by modern terminals; use sparingly.
    pub const BLINK: Attrs = Attrs(1 << 5);
    /// SGR 7 (swap fg/bg at draw time — the terminal does it, not us).
    pub const REVERSE: Attrs = Attrs(1 << 6);
    /// SGR 8 (invisible ink; still occupies its cells).
    pub const HIDDEN: Attrs = Attrs(1 << 7);
    /// SGR 9 (strikethrough).
    pub const STRIKE: Attrs = Attrs(1 << 8);
    /// Every defined attribute (the valid-bits mask).
    pub const ALL: Attrs = Attrs(0x01FF);

    /// The raw bitset (stable across versions only for serialization
    /// round-trips through [`Attrs::from_bits_truncate`]).
    pub const fn bits(self) -> u16 {
        self.0
    }

    /// Masks unknown bits so stray data can never leak into SGR emission.
    pub const fn from_bits_truncate(bits: u16) -> Attrs {
        Attrs(bits & Attrs::ALL.0)
    }

    /// True when no attribute is set.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// True when EVERY bit of `other` is set in `self`.
    pub const fn contains(self, other: Attrs) -> bool {
        (self.0 & other.0) == other.0
    }

    /// True when ANY bit of `other` is set in `self`.
    pub const fn intersects(self, other: Attrs) -> bool {
        (self.0 & other.0) != 0
    }

    /// Union (also available as `|`).
    pub const fn with(self, other: Attrs) -> Attrs {
        Attrs(self.0 | other.0)
    }

    /// Difference: `self` with `other`'s bits cleared.
    pub const fn without(self, other: Attrs) -> Attrs {
        Attrs(self.0 & !other.0)
    }
}

impl std::ops::BitOr for Attrs {
    type Output = Attrs;
    fn bitor(self, rhs: Attrs) -> Attrs {
        self.with(rhs)
    }
}

impl std::ops::BitOrAssign for Attrs {
    fn bitor_assign(&mut self, rhs: Attrs) {
        *self = self.with(rhs);
    }
}

impl std::ops::BitAnd for Attrs {
    type Output = Attrs;
    fn bitand(self, rhs: Attrs) -> Attrs {
        Attrs(self.0 & rhs.0)
    }
}

impl std::ops::Sub for Attrs {
    type Output = Attrs;
    fn sub(self, rhs: Attrs) -> Attrs {
        self.without(rhs)
    }
}

impl std::ops::SubAssign for Attrs {
    fn sub_assign(&mut self, rhs: Attrs) {
        *self = self.without(rhs);
    }
}

impl std::fmt::Debug for Attrs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        const NAMES: [(Attrs, &str); 9] = [
            (Attrs::BOLD, "BOLD"),
            (Attrs::DIM, "DIM"),
            (Attrs::ITALIC, "ITALIC"),
            (Attrs::UNDERLINE, "UNDERLINE"),
            (Attrs::UNDERCURL, "UNDERCURL"),
            (Attrs::BLINK, "BLINK"),
            (Attrs::REVERSE, "REVERSE"),
            (Attrs::HIDDEN, "HIDDEN"),
            (Attrs::STRIKE, "STRIKE"),
        ];
        if self.is_empty() {
            return f.write_str("Attrs(NONE)");
        }
        f.write_str("Attrs(")?;
        let mut first = true;
        for (bit, name) in NAMES {
            if self.contains(bit) {
                if !first {
                    f.write_str("|")?;
                }
                f.write_str(name)?;
                first = false;
            }
        }
        f.write_str(")")
    }
}
