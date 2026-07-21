//! Queryable VT model state: DEC private mode flags and the diagnostic
//! counters the diff/present property tests assert on.
//!
//! OWNER: REDTEAM. Split from `vt.rs`; re-exported through it so both
//! `testing::vt::Modes` and `testing::Modes` remain valid paths.

/// DEC private modes + derived flags, queryable by tests.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Modes {
    set: std::collections::BTreeSet<u32>,
}

impl Modes {
    pub fn is_set(&self, mode: u32) -> bool {
        self.set.contains(&mode)
    }
    pub fn all_set(&self) -> Vec<u32> {
        self.set.iter().copied().collect()
    }
    pub fn synchronized_output(&self) -> bool {
        self.is_set(2026)
    }
    pub fn alt_screen(&self) -> bool {
        self.is_set(1049)
    }
    pub fn cursor_visible(&self) -> bool {
        self.is_set(25)
    }
    pub fn bracketed_paste(&self) -> bool {
        self.is_set(2004)
    }
    pub fn focus_reporting(&self) -> bool {
        self.is_set(1004)
    }
    pub fn sgr_mouse(&self) -> bool {
        self.is_set(1006)
    }
    pub fn autowrap(&self) -> bool {
        self.is_set(7)
    }
    pub(super) fn insert(&mut self, mode: u32) {
        self.set.insert(mode);
    }
    pub(super) fn remove(&mut self, mode: u32) {
        self.set.remove(&mode);
    }
}

/// Diagnostic counters. Tests assert on these to prove the presenter
/// emitted only modeled traffic and balanced its brackets.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VtCounters {
    /// Sequences/bytes the model does not understand. Zero for a clean run.
    pub unknown: u64,
    /// Invalid UTF-8 byte runs replaced with U+FFFD.
    pub utf8_errors: u64,
    /// DECSET 2026 count (synchronized update begins).
    pub sync_begins: u64,
    /// DECRST 2026 count (synchronized update ends).
    pub sync_ends: u64,
    /// DCS/APC/PM/SOS frames consumed (probe traffic etc.), unmodeled.
    pub string_frames: u64,
    /// Kitty keyboard pushes minus pops floor-clamped at 0 (`CSI > u` / `CSI < u`).
    pub kitty_push_depth: u64,
}
