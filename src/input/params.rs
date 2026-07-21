//! CSI parameter-list parsing shared by every CSI decoder.
//!
//! OWNER: KERNEL. One grammar serves keys, mouse and caps replies:
//! `;`-separated parameters, `:`-separated subparameters (kitty), a leading
//! private marker (`<`, `?`, `>`, `=`), and a trailing intermediate byte
//! (DECRPM's `$`). Fixed-size storage: values saturate, extras drop, and
//! the parse can never allocate or panic — this sits on the fuzz surface.

/// Parsed CSI parameter list: up to 8 `;`-params of up to 4 `:`-subparams.
#[derive(Debug, Default)]
pub(crate) struct CsiParams {
    vals: [[u32; Self::MAX_SUB]; Self::MAX_PARAMS],
    nsub: [u8; Self::MAX_PARAMS],
    present: [bool; Self::MAX_PARAMS],
    count: usize,
    /// First byte when in `0x3C..=0x3F` (`<` `=` `>` `?`), else 0.
    pub private: u8,
    /// Last intermediate byte (`0x20..=0x2F`), else 0.
    pub intermediate: u8,
}

impl CsiParams {
    pub(crate) const MAX_PARAMS: usize = 8;
    pub(crate) const MAX_SUB: usize = 4;

    /// Parse the bytes between `CSI` and the final byte.
    pub(crate) fn parse(bytes: &[u8]) -> CsiParams {
        let mut p = CsiParams::default();
        let mut pi = 0usize;
        let mut si = 0usize;
        let mut any = false;
        for &b in bytes {
            match b {
                b'0'..=b'9' => {
                    if pi < Self::MAX_PARAMS && si < Self::MAX_SUB {
                        p.vals[pi][si] = p.vals[pi][si]
                            .saturating_mul(10)
                            .saturating_add(u32::from(b - b'0'));
                        p.nsub[pi] = p.nsub[pi].max(si as u8 + 1);
                        if si == 0 {
                            p.present[pi] = true;
                        }
                    }
                    any = true;
                }
                b':' => {
                    si += 1;
                    if pi < Self::MAX_PARAMS && si < Self::MAX_SUB {
                        p.nsub[pi] = p.nsub[pi].max(si as u8 + 1);
                    }
                }
                b';' => {
                    pi += 1;
                    si = 0;
                }
                // A private marker only means something as the first byte;
                // anywhere else it is malformed and ignored.
                0x3c..=0x3f => {
                    if p.private == 0 && pi == 0 && si == 0 && !any {
                        p.private = b;
                    }
                }
                0x20..=0x2f => p.intermediate = b,
                _ => {}
            }
        }
        p.count = (pi + 1).min(Self::MAX_PARAMS);
        p
    }

    /// Param `i`'s first subparam, or `def` when absent/empty (the CSI
    /// convention: an omitted parameter has a per-sequence default).
    pub(crate) fn get_or(&self, i: usize, def: u32) -> u32 {
        if i < self.count && i < Self::MAX_PARAMS && self.present[i] {
            self.vals[i][0]
        } else {
            def
        }
    }

    /// Subparam `j` of param `i`, when it exists (empty subs read as 0).
    pub(crate) fn sub(&self, i: usize, j: usize) -> Option<u32> {
        if i < Self::MAX_PARAMS && j < Self::MAX_SUB && j < self.nsub[i] as usize {
            Some(self.vals[i][j])
        } else {
            None
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.count
    }

    /// All present first-subparams, for DA1-style attribute lists.
    pub(crate) fn list(&self) -> Vec<u32> {
        (0..self.count.min(Self::MAX_PARAMS))
            .filter(|&i| self.present[i])
            .map(|i| self.vals[i][0])
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn params_subparams_and_defaults() {
        let p = CsiParams::parse(b"1;5:3;7");
        assert_eq!(p.len(), 3);
        assert_eq!(p.get_or(0, 9), 1);
        assert_eq!(p.get_or(1, 9), 5);
        assert_eq!(p.sub(1, 1), Some(3));
        assert_eq!(p.sub(1, 2), None);
        assert_eq!(p.get_or(2, 9), 7);
        assert_eq!(p.get_or(3, 9), 9); // absent -> default
                                       // Empty param slot reads as its default.
        let p = CsiParams::parse(b"1;;5");
        assert_eq!(p.get_or(1, 42), 42);
        assert_eq!(p.get_or(2, 42), 5);
    }

    #[test]
    fn private_and_intermediate_markers() {
        let p = CsiParams::parse(b"?2026;2$");
        assert_eq!(p.private, b'?');
        assert_eq!(p.intermediate, b'$');
        assert_eq!(p.get_or(0, 0), 2026);
        let p = CsiParams::parse(b"<35;10;5");
        assert_eq!(p.private, b'<');
        // A marker mid-sequence is malformed noise, not a marker.
        let p = CsiParams::parse(b"5?7");
        assert_eq!(p.private, 0);
    }

    #[test]
    fn saturation_and_overflow_are_lossless_ish() {
        // Values saturate instead of wrapping.
        let p = CsiParams::parse(b"99999999999999999999");
        assert_eq!(p.get_or(0, 0), u32::MAX);
        // More params than storage: extras dropped, count capped.
        let p = CsiParams::parse(b"1;2;3;4;5;6;7;8;9;10;11");
        assert_eq!(p.len(), CsiParams::MAX_PARAMS);
        // More subparams than storage: extras ignored, no panic.
        let p = CsiParams::parse(b"1:2:3:4:5:6:7");
        assert_eq!(p.sub(0, 3), Some(4));
    }
}
