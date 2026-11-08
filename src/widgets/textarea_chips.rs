//! Mention chips inside the TextArea buffer (private sibling of
//! textarea.rs — file-size split, the textarea_model.rs pattern).
//!
//! ## Representation: one marker scalar per chip
//!
//! A chip is ONE Unicode scalar from Supplementary Private Use Area-A
//! (U+F0000 + k) in the buffer, where `k` indexes this state's chip
//! registry. One scalar is one grapheme cluster, so every caret rule
//! the editor already has makes the chip atomic for free: Left/Right
//! step over it in one move, Backspace/Delete remove it whole, a
//! selection takes all of it or none of it, and nothing can be typed
//! INSIDE it. The registry is append-only and deduplicated by
//! `(id, name, kind)`, so it is bounded by the host's roster and a
//! recalled history entry still finds its chips.
//!
//! The marker's display width is its label's (`@name`), which the
//! soft-wrap map and the caret columns read through [`ChipTable`]; the
//! renderer paints the label in the chip style instead of the marker.
//!
//! OWNER: REACT.

use std::rc::Rc;

use crate::text::Segment;
use crate::widgets::mention::{ComposerSubmission, Mention, MentionCandidate};

/// First marker scalar (Supplementary Private Use Area-A).
const CHIP_BASE: u32 = 0xF_0000;
/// Last usable scalar of the area (U+FFFFE/U+FFFFF are noncharacters).
const CHIP_LAST: u32 = 0xF_FFFD;

/// The marker scalar for registry slot `k` — exactly ONE scalar, the
/// whole atomicity argument (module notes).
fn marker(k: usize) -> Option<char> {
    let code = CHIP_BASE.checked_add(u32::try_from(k).ok()?)?;
    if code > CHIP_LAST {
        return None;
    }
    char::from_u32(code)
}

/// Registry slot of a cluster that is exactly one marker scalar.
fn slot_of(cluster: &str) -> Option<usize> {
    let mut chars = cluster.chars();
    let ch = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    let code = ch as u32;
    (CHIP_BASE..=CHIP_LAST)
        .contains(&code)
        .then(|| (code - CHIP_BASE) as usize)
}

/// Display widths and labels of this state's chips, indexed by slot.
/// Cheap to clone (one `Rc`); the default table has no chips, so every
/// width is the cluster's natural width — the pre-chip model exactly.
#[derive(Clone, Default)]
pub(crate) struct ChipTable {
    labels: Rc<Vec<(String, i32)>>,
}

impl ChipTable {
    /// The chip label for a cluster, when it is a registered marker.
    pub fn label(&self, cluster: &str) -> Option<&str> {
        let slot = slot_of(cluster)?;
        self.labels.get(slot).map(|(l, _)| l.as_str())
    }

    /// Display width of one measured cluster: a chip's label width,
    /// otherwise the natural width.
    pub fn seg_width(&self, seg: &Segment<'_>) -> i32 {
        match slot_of(seg.cluster).and_then(|s| self.labels.get(s)) {
            Some((_, w)) => *w,
            None => seg.width,
        }
    }

    /// Display width of a run of text (sum over its clusters).
    pub fn width(&self, s: &str) -> i32 {
        if self.labels.is_empty() {
            return crate::text::width(s);
        }
        crate::text::segments(s)
            .map(|seg| self.seg_width(&seg))
            .sum()
    }
}

/// The append-only, deduplicated chip registry behind one TextAreaState.
#[derive(Default)]
pub(crate) struct ChipRegistry {
    entries: Vec<MentionCandidate>,
    table: ChipTable,
}

impl ChipRegistry {
    /// The marker for `candidate`, registering it on first use. `None`
    /// only when the private-use area is exhausted (65 534 DISTINCT
    /// candidates in one state's lifetime).
    pub fn marker_for(&mut self, candidate: &MentionCandidate) -> Option<char> {
        let same = |e: &MentionCandidate| {
            e.id == candidate.id && e.name == candidate.name && e.kind == candidate.kind
        };
        if let Some(k) = self.entries.iter().position(same) {
            return marker(k);
        }
        let ch = marker(self.entries.len())?;
        self.entries.push(candidate.clone());
        let label = candidate.label();
        let w = crate::text::width(&label);
        Rc::make_mut(&mut self.table.labels).push((label, w));
        Some(ch)
    }

    pub fn table(&self) -> ChipTable {
        self.table.clone()
    }

    /// `{text, mentions}` for a raw buffer: each registered marker is
    /// spelled `@name` and listed in text order with its ordinal. A
    /// marker this registry never issued stays as it is (it is not a
    /// chip of this state).
    pub fn submission(&self, raw: &str) -> ComposerSubmission {
        let mut out = ComposerSubmission {
            text: String::with_capacity(raw.len()),
            mentions: Vec::new(),
        };
        for ch in raw.chars() {
            let mut buf = [0u8; 4];
            let entry = slot_of(ch.encode_utf8(&mut buf)).and_then(|k| self.entries.get(k));
            match entry {
                Some(c) => {
                    out.text.push_str(&c.label());
                    out.mentions.push(Mention {
                        id: c.id.clone(),
                        name: c.name.clone(),
                        kind: c.kind,
                        index: out.mentions.len(),
                    });
                }
                None => out.text.push(ch),
            }
        }
        out
    }
}
