//! Palette access for the VT model — a thin re-export of THE canonical
//! table in `base::palette` (integrator ruling on RT1-7: one table, no
//! hand-typed copies anywhere).
//!
//! OWNER: REDTEAM.
//!
//! History note: cycle 1 embedded xterm's compiled-in defaults here
//! (system colors 1..=6 in the 0xcd family). The integrator's canonical
//! table chose the classic 0x80-family system colors instead; the VT
//! model follows the ruling — what matters for the diff/present property
//! is that the presenter's downlevel and this model resolve indexed
//! colors IDENTICALLY, which the re-export guarantees by construction.
//! The equality test below exists so any future divergence (someone
//! re-introducing a local table) fails loudly here, in the referee's
//! own file.

pub use crate::base::palette::{xterm_256, CUBE_LEVELS, SYSTEM_16, XTERM_256};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::Rgba;

    #[test]
    fn reexport_is_the_base_table() {
        // Symbol identity: these must literally be base::palette's items.
        assert_eq!(SYSTEM_16.as_ptr(), crate::base::palette::SYSTEM_16.as_ptr());
        assert_eq!(XTERM_256.as_ptr(), crate::base::palette::XTERM_256.as_ptr());
        for i in 0..=255u8 {
            assert_eq!(xterm_256(i), crate::base::palette::xterm_256(i));
            assert_eq!(XTERM_256[i as usize], xterm_256(i));
        }
    }

    #[test]
    fn structural_invariants_of_the_canonical_table() {
        // Cube corners and gray ramp — properties any xterm-256 table
        // must satisfy regardless of the system-16 flavor chosen.
        assert_eq!(xterm_256(16), Rgba::rgb(0, 0, 0));
        assert_eq!(xterm_256(231), Rgba::WHITE);
        assert_eq!(xterm_256(196), Rgba::rgb(255, 0, 0));
        assert_eq!(xterm_256(21), Rgba::rgb(0, 0, 255));
        assert_eq!(xterm_256(46), Rgba::rgb(0, 255, 0));
        assert_eq!(xterm_256(232), Rgba::rgb(8, 8, 8));
        assert_eq!(xterm_256(255), Rgba::rgb(238, 238, 238));
        // Gray ramp is strictly monotonic.
        for i in 233..=255u8 {
            assert!(xterm_256(i).r > xterm_256(i - 1).r);
        }
        // Cube levels are the xterm ramp.
        assert_eq!(CUBE_LEVELS, [0x00, 0x5f, 0x87, 0xaf, 0xd7, 0xff]);
    }
}
