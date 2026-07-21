//! Platform-independent state machines behind the Windows backend,
//! extracted so their unit tests run on EVERY host (RT8-9): the
//! `#[cfg(windows)]` type they serve is compile-checked only off-Windows,
//! and a pairing/latch bug is exactly the class a compile-only path hides
//! until a real Windows session hits it.
//!
//! OWNER: KERNEL. No `windows-sys` imports here, by construction —
//! `windows.rs` delegates; these tests are the executable half of the
//! Windows story (evidence matrix §3.5 keeps the honest split).

use crate::base::Size;

/// Incremental UTF-16 → UTF-8 decoder for console KEY_EVENT units.
///
/// Console records deliver one UTF-16 code unit per record, and a
/// surrogate PAIR can straddle a `ReadConsoleInputW` batch boundary — the
/// pending high surrogate must survive across `push` calls. Lone halves
/// become U+FFFD (never panic, never silently drop).
#[derive(Debug, Default)]
pub(crate) struct Utf16Decoder {
    pending_high: Option<u16>,
}

impl Utf16Decoder {
    /// Append `unit`'s UTF-8 encoding to `out` (or hold a high surrogate
    /// until its partner arrives).
    pub(crate) fn push(&mut self, unit: u16, out: &mut Vec<u8>) {
        if let Some(high) = self.pending_high.take() {
            if (0xdc00..=0xdfff).contains(&unit) {
                let c =
                    0x10000u32 + ((u32::from(high) - 0xd800) << 10) + (u32::from(unit) - 0xdc00);
                push_char(
                    char::from_u32(c).unwrap_or(char::REPLACEMENT_CHARACTER),
                    out,
                );
                return;
            }
            // The high half never got its partner: honest replacement,
            // then the current unit processes on its own.
            push_char(char::REPLACEMENT_CHARACTER, out);
        }
        match unit {
            0xd800..=0xdbff => self.pending_high = Some(unit),
            0xdc00..=0xdfff => push_char(char::REPLACEMENT_CHARACTER, out),
            _ => push_char(
                char::from_u32(u32::from(unit)).unwrap_or(char::REPLACEMENT_CHARACTER),
                out,
            ),
        }
    }
}

fn push_char(c: char, out: &mut Vec<u8>) {
    let mut buf = [0u8; 4];
    out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
}

/// Durable wake latch. The Win32 auto-reset event RESETS the moment a
/// wait consumes it, so when input wins the same wakeup this latch is the
/// ONLY memory that a wake happened — dropping it would stall the
/// scheduler until the next keystroke. `arm` remembers; `take` delivers
/// exactly once.
#[derive(Debug, Default)]
pub(crate) struct WakeLatch {
    pending: bool,
}

impl WakeLatch {
    /// Remember a consumed-but-undelivered wake (coalescing: arming an
    /// armed latch stays one wake).
    pub(crate) fn arm(&mut self) {
        self.pending = true;
    }

    /// Deliver the wake: true exactly once per armed period.
    pub(crate) fn take(&mut self) -> bool {
        std::mem::take(&mut self.pending)
    }
}

/// Resize observation: dedupe + sanity in one place, shared by the two
/// windows detection paths (WINDOW_BUFFER_SIZE records and the RT5-12a
/// re-query on every wake). Reports a size only when it is fresh news —
/// non-empty and different from the last report.
#[derive(Debug, Default)]
pub(crate) struct ResizeTracker {
    seen: Size,
}

impl ResizeTracker {
    /// Baseline at session entry (no event for the initial size).
    pub(crate) fn reset(&mut self, size: Size) {
        self.seen = size;
    }

    /// Fold a fresh measurement; `Some` = report this resize.
    pub(crate) fn observe(&mut self, fresh: Size) -> Option<Size> {
        if fresh != self.seen && !fresh.is_empty() {
            self.seen = fresh;
            Some(fresh)
        } else {
            None
        }
    }
}

/// Clamp a KEY_EVENT repeat count: 0 is treated as 1 (conhost quirk
/// tolerance), and a hostile/corrupt count cannot expand one record into
/// unbounded output.
pub(crate) fn clamp_repeat(count: u16, cap: u16) -> u16 {
    count.clamp(1, cap)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_units(units: &[u16]) -> String {
        let mut d = Utf16Decoder::default();
        let mut out = Vec::new();
        for &u in units {
            d.push(u, &mut out);
        }
        String::from_utf8_lossy(&out).into_owned()
    }

    #[test]
    fn utf16_bmp_and_astral_pairs() {
        assert_eq!(decode_units(&[0x68, 0x69]), "hi");
        assert_eq!(decode_units(&[0x6f22, 0x5b57]), "漢字");
        // U+1F389 🎉 = D83C DF89: a full pair -> 4-byte UTF-8.
        assert_eq!(decode_units(&[0xd83c, 0xdf89]), "🎉");
        // Mixed stream: text, pair, text.
        assert_eq!(decode_units(&[0x61, 0xd83c, 0xdf89, 0x62]), "a🎉b");
    }

    #[test]
    fn utf16_pair_split_across_batches_survives() {
        // THE RT8-9 case: the high half arrives in one ReadConsoleInputW
        // batch, the low half in the next — state must carry across
        // separate push sequences on the same decoder.
        let mut d = Utf16Decoder::default();
        let mut out = Vec::new();
        d.push(0xd83c, &mut out); // batch 1 ends mid-pair
        assert!(out.is_empty(), "no premature emission");
        d.push(0xdf89, &mut out); // batch 2 completes it
        assert_eq!(String::from_utf8_lossy(&out), "🎉");
    }

    #[test]
    fn utf16_lone_halves_become_replacement_never_lost() {
        // Lone high then BMP: U+FFFD, then the BMP char intact.
        assert_eq!(decode_units(&[0xd83c, 0x41]), "\u{fffd}A");
        // Lone low with no high.
        assert_eq!(decode_units(&[0xdf89]), "\u{fffd}");
        // High followed by ANOTHER high: first is honest FFFD, second
        // pairs with the low that follows.
        assert_eq!(decode_units(&[0xd83c, 0xd83c, 0xdf89]), "\u{fffd}🎉");
        // Back-to-back lone lows.
        assert_eq!(decode_units(&[0xdc00, 0xdc01]), "\u{fffd}\u{fffd}");
    }

    #[test]
    fn wake_latch_delivers_exactly_once_and_survives_input_wins() {
        let mut w = WakeLatch::default();
        assert!(!w.take(), "unarmed latch delivers nothing");
        w.arm();
        assert!(w.take(), "armed latch delivers");
        assert!(!w.take(), "…exactly once");
        // Coalescing: two arms, one wake.
        w.arm();
        w.arm();
        assert!(w.take());
        assert!(!w.take());
        // The load-bearing sequence: wake armed, then INPUT wins several
        // loop turns (the latch is not consulted), then delivery.
        w.arm();
        for _turn_with_input in 0..3 {
            // read() returns Input without touching the latch
        }
        assert!(w.take(), "wake survived the input turns");
    }

    #[test]
    fn resize_tracker_dedupes_and_guards() {
        let mut r = ResizeTracker::default();
        // First real measurement from the zero baseline is news.
        assert_eq!(r.observe(Size::new(80, 24)), Some(Size::new(80, 24)));
        // The same size again (coalesced records + the every-wake
        // re-query hitting an unchanged console) is not.
        assert_eq!(r.observe(Size::new(80, 24)), None);
        assert_eq!(r.observe(Size::new(120, 40)), Some(Size::new(120, 40)));
        // A zero/degenerate measurement never becomes an event and never
        // poisons the baseline.
        assert_eq!(r.observe(Size::ZERO), None);
        assert_eq!(r.observe(Size::new(0, 40)), None);
        assert_eq!(r.observe(Size::new(120, 40)), None, "baseline kept");
        // Session re-entry baselines without emitting.
        r.reset(Size::new(90, 30));
        assert_eq!(r.observe(Size::new(90, 30)), None);
    }

    #[test]
    fn repeat_clamp_bounds_both_ends() {
        assert_eq!(clamp_repeat(0, 1024), 1, "conhost zero-repeat quirk");
        assert_eq!(clamp_repeat(1, 1024), 1);
        assert_eq!(clamp_repeat(7, 1024), 7);
        assert_eq!(clamp_repeat(u16::MAX, 1024), 1024, "hostile count capped");
    }
}
