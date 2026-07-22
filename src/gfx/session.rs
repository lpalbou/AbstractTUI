//! Image placement lifecycle across frames: what the terminal
//! currently HOLDS vs what the app wants shown, reconciled with the
//! cheapest protocol traffic the active channel allows.
//!
//! Channel economics (why this object exists):
//!
//! - **kitty**: images are uploaded ONCE under a client id; moving or
//!   re-fitting an existing image is a tiny `a=p` placement escape (no
//!   pixel retransmission), removal is `a=d,d=I`. The session tracks
//!   (id, content version, rect) per slot to emit the minimum.
//! - **iTerm2**: NO ids, no placement model — any change (content,
//!   move, resize) means re-emitting the full base64 PNG at the new
//!   cursor position, and STALE cells must be cleared by the cell
//!   layer (the old image scrolls/overdraws like text). Redraw cost is
//!   the whole payload every time; the session just re-emits.
//! - **sixel**: same as iTerm2 (paints at cursor, no ids), plus the
//!   RT1-11 single-palette rule — the LAST emission owns the shared
//!   registers, so many live sixel images recolor each other; the
//!   session re-emits on any change and callers should prefer one live
//!   sixel image per screen (documented v1 limit).
//! - **mosaic**: cells come back to the caller every sync; there is no
//!   terminal-side state to reconcile.
//!
//! The session never touches the terminal: bytes go through the same
//! [`ExternalSink`] the presenter adapts (damage contract §6).

use std::collections::HashMap;

use crate::base::Rect;
use crate::gfx::bitmap::Bitmap;
use crate::gfx::pipeline::{Channel, ExternalSink, ImageOutput, ImageRenderer, RenderedImage};
use crate::gfx::proto::kitty;
use crate::term::caps::{GraphicsCaps, WrapKind};

/// Caller-facing slot key. Stable across frames (a widget instance id,
/// an overlay handle — the caller's identity for "this picture").
pub type SlotKey = u64;

#[derive(Clone, Debug)]
struct SlotState {
    /// Caller-declared content version: bump it when pixels changed.
    version: u64,
    rect: Rect,
    channel: Channel,
    kitty_id: Option<u32>,
}

/// What a sync did — callers use it to know whether cells need
/// repainting (mosaic) or nothing changed at all.
#[derive(Debug)]
pub enum SyncOutcome {
    /// Terminal state already matches (no bytes, no cells).
    Unchanged,
    /// Bytes were written to the sink (protocol channels).
    Emitted(RenderedImage),
    /// The caller must blit these cells (mosaic channel).
    Cells(RenderedImage),
}

/// Tracks terminal-held image state across frames. One session per
/// sink/terminal; slots keyed by the caller.
///
/// ```
/// use abstracttui::base::{Point, Rect, Rgba};
/// use abstracttui::gfx::{Bitmap, ExternalSink, ImageSession};
/// use abstracttui::term::caps::GraphicsCaps;
///
/// // Bytes go through the presenter's sink (damage contract §6) —
/// // a Vec stands in here.
/// struct Sink(Vec<u8>);
/// impl ExternalSink for Sink {
///     fn external_write(&mut self, bytes: &[u8], _at: Point) {
///         self.0.extend_from_slice(bytes);
///     }
/// }
///
/// let caps = GraphicsCaps::with(|g| g.kitty_graphics = true);
/// let mut session = ImageSession::new();
/// let mut sink = Sink(Vec::new());
/// let img = Bitmap::new(4, 4, Rgba::rgb(9, 9, 9));
///
/// // slot key 1, content version 1: first sync transmits.
/// session.sync(&mut sink, 1, 1, &img, Rect::new(0, 0, 8, 4), &caps);
/// assert_eq!(session.live_kitty_ids().len(), 1);
/// // Same version + rect: no traffic. Move: placement escape only.
/// // Bump the version when pixels change; release() frees the upload.
/// session.check_invariants().unwrap();
/// ```
pub struct ImageSession {
    renderer: ImageRenderer,
    slots: HashMap<SlotKey, SlotState>,
    /// Lifetime kitty traffic tally (accounting for the invariant
    /// checker): uploads (a=T with an id), placement escapes (a=p),
    /// deletes (a=d,d=I). Counters, not state — they only grow.
    kitty_transmits: u64,
    kitty_places: u64,
    kitty_deletes: u64,
}

impl Default for ImageSession {
    fn default() -> Self {
        ImageSession {
            renderer: ImageRenderer::new(),
            slots: HashMap::new(),
            kitty_transmits: 0,
            kitty_places: 0,
            kitty_deletes: 0,
        }
    }
}

impl ImageSession {
    pub fn new() -> ImageSession {
        ImageSession::default()
    }

    /// Access the renderer's configuration (mosaic mode, z, dither...).
    pub fn renderer_mut(&mut self) -> &mut ImageRenderer {
        &mut self.renderer
    }

    /// Reconcile one slot with the terminal. `version` is the caller's
    /// content counter — same version + same rect = nothing to do;
    /// same version + new rect = kitty re-places without retransmit
    /// (other channels re-emit); new version = full transmit.
    pub fn sync(
        &mut self,
        sink: &mut dyn ExternalSink,
        key: SlotKey,
        version: u64,
        img: &Bitmap,
        rect: Rect,
        caps: &GraphicsCaps,
    ) -> SyncOutcome {
        let channel = crate::gfx::pipeline::choose_channel(caps);
        let prior = self.slots.get(&key).cloned();

        // Channel changed since last sync (caps upgraded mid-session):
        // drop the old state honestly, start over.
        let prior = match prior {
            Some(p) if p.channel != channel => {
                self.release(sink, key, caps);
                None
            }
            p => p,
        };

        if let Some(prev) = &prior {
            if prev.version == version && prev.rect == rect {
                return SyncOutcome::Unchanged;
            }
            // kitty move/refit with unchanged pixels: placement escape
            // only — delete the visible placement, re-place by id.
            if channel == Channel::Kitty && prev.version == version {
                if let Some(id) = prev.kitty_id {
                    let mut bytes = kitty::place(
                        id,
                        Some(rect.w as u32),
                        Some(rect.h as u32),
                        self.renderer.config.z,
                    );
                    bytes = wrap_for(caps, bytes);
                    sink.external_write(&bytes, rect.origin());
                    self.kitty_places += 1;
                    self.slots.insert(
                        key,
                        SlotState {
                            version,
                            rect,
                            channel,
                            kitty_id: Some(id),
                        },
                    );
                    return SyncOutcome::Emitted(RenderedImage {
                        channel,
                        output: ImageOutput::Bytes {
                            bytes: Vec::new(),
                            at: rect.origin(),
                        },
                        warnings: Vec::new(),
                        kitty_id: Some(id),
                    });
                }
            }
            // Content changed on kitty: free the superseded upload
            // before transmitting the replacement (unbounded terminal
            // memory otherwise — quilts of stale frames).
            if channel == Channel::Kitty {
                if let Some(id) = prev.kitty_id {
                    let bytes = wrap_for(caps, kitty::delete_by_id(id, true));
                    sink.external_write(&bytes, rect.origin());
                    self.kitty_deletes += 1;
                }
            }
        }

        // Full render + emit through the shared pipeline (tmux wrap
        // happens inside the renderer).
        let rendered = self.renderer.render(img, rect, caps);
        match &rendered.output {
            ImageOutput::Bytes { bytes, at } => {
                sink.external_write(bytes, *at);
                if channel == Channel::Kitty && rendered.kitty_id.is_some() {
                    self.kitty_transmits += 1;
                }
                self.slots.insert(
                    key,
                    SlotState {
                        version,
                        rect,
                        channel,
                        kitty_id: rendered.kitty_id,
                    },
                );
                SyncOutcome::Emitted(rendered)
            }
            ImageOutput::Cells(_) => {
                self.slots.insert(
                    key,
                    SlotState {
                        version,
                        rect,
                        channel,
                        kitty_id: None,
                    },
                );
                SyncOutcome::Cells(rendered)
            }
        }
    }

    /// Drop a slot: kitty frees the upload; cursor-paint channels have
    /// nothing to delete (the CELL layer repaints over the corpse —
    /// documented cost of id-less protocols).
    pub fn release(&mut self, sink: &mut dyn ExternalSink, key: SlotKey, caps: &GraphicsCaps) {
        if let Some(state) = self.slots.remove(&key) {
            if let (Channel::Kitty, Some(id)) = (state.channel, state.kitty_id) {
                let bytes = wrap_for(caps, kitty::delete_by_id(id, true));
                sink.external_write(&bytes, state.rect.origin());
                self.kitty_deletes += 1;
            }
        }
    }

    /// Drop everything (screen clear / app shutdown).
    pub fn release_all(&mut self, sink: &mut dyn ExternalSink, caps: &GraphicsCaps) {
        let keys: Vec<SlotKey> = self.slots.keys().copied().collect();
        for key in keys {
            self.release(sink, key, caps);
        }
    }

    /// Slots currently believed live on the terminal.
    pub fn live_slots(&self) -> usize {
        self.slots.len()
    }

    /// Slots on byte channels (kitty/iTerm2/sixel). The driver's scroll
    /// guard reads this: terminal-executed scrolls (DECSTBM + SU/SD)
    /// move protocol images WITH the text — kitty mandates it, sixel
    /// pixels scroll on xterm-class emulators — which would desync the
    /// session's placement bookkeeping. Mosaic slots live in the cell
    /// model and scroll correctly through the ordinary diff.
    pub fn live_byte_slots(&self) -> usize {
        self.slots
            .values()
            .filter(|s| s.channel != Channel::Mosaic)
            .count()
    }

    /// The channel and rect the session believes the terminal holds for
    /// `key` — the driver's vacated-rect bookkeeping (which cells must
    /// repaint when a placement moves/retires) and a diagnostics probe.
    pub fn slot_info(&self, key: SlotKey) -> Option<(Channel, Rect)> {
        self.slots.get(&key).map(|s| (s.channel, s.rect))
    }

    /// Forget a slot WITHOUT terminal-side deletes: the caller repainted
    /// the cells under a cursor-paint slot (mosaic patches overwritten in
    /// the surface; iTerm2/sixel pixels overwritten by re-emitted cells),
    /// so the terminal no longer matches and the next `sync` must
    /// re-emit in full. Kitty slots REFUSE: their state lives in
    /// terminal image memory, not in cells — forgetting one would leak
    /// the upload and re-transmit under a fresh id ([`Self::release`]
    /// is the kitty verb).
    pub(crate) fn invalidate_slot(&mut self, key: SlotKey) {
        if self
            .slots
            .get(&key)
            .is_some_and(|s| s.channel != Channel::Kitty)
        {
            self.slots.remove(&key);
        }
    }

    /// Kitty ids the session believes the terminal currently holds
    /// (sorted — cross-check against a protocol model's live set).
    pub fn live_kitty_ids(&self) -> Vec<u32> {
        let mut ids: Vec<u32> = self.slots.values().filter_map(|s| s.kitty_id).collect();
        ids.sort_unstable();
        ids
    }

    /// Lifetime kitty traffic: (transmits, placements, deletes).
    pub fn kitty_traffic(&self) -> (u64, u64, u64) {
        (self.kitty_transmits, self.kitty_places, self.kitty_deletes)
    }

    /// Self-consistency audit of the session's terminal-state
    /// accounting (cycle-7 hardening — the protocol paths are
    /// byte-verified, not live-verified, so the bookkeeping must be
    /// checkable at any step). Cheap; call it after every sync in
    /// tests, or from debug assertions in an app. Invariants:
    ///
    /// 1. kitty slots carry an id (an id-less kitty slot could never
    ///    be moved or freed — a guaranteed terminal-memory leak);
    /// 2. non-kitty slots carry NO id (an id implies terminal-held
    ///    state that cursor-paint channels do not have);
    /// 3. ids are unique across slots (a shared id means one release
    ///    would delete another slot's pixels);
    /// 4. delete accounting: every upload is freed at most once, and
    ///    live uploads == transmits − deletes (nothing leaks, nothing
    ///    is double-freed).
    pub fn check_invariants(&self) -> Result<(), String> {
        let mut ids = Vec::new();
        for (key, slot) in &self.slots {
            match (slot.channel, slot.kitty_id) {
                (Channel::Kitty, None) => {
                    return Err(format!("slot {key}: kitty slot without an id (unfreeable)"));
                }
                (Channel::Kitty, Some(id)) => ids.push(id),
                (_, Some(id)) => {
                    return Err(format!(
                        "slot {key}: id {id} on a {:?} slot (no terminal-held state to name)",
                        slot.channel
                    ));
                }
                (_, None) => {}
            }
        }
        ids.sort_unstable();
        if let Some(w) = ids.windows(2).find(|w| w[0] == w[1]) {
            return Err(format!("kitty id {} owned by two slots", w[0]));
        }
        if self.kitty_deletes > self.kitty_transmits {
            return Err(format!(
                "delete accounting: {} deletes exceed {} transmits",
                self.kitty_deletes, self.kitty_transmits
            ));
        }
        let live = ids.len() as u64;
        let expected = self.kitty_transmits - self.kitty_deletes;
        if live != expected {
            return Err(format!(
                "live accounting: {live} kitty slots but transmits({}) - deletes({}) = {expected}",
                self.kitty_transmits, self.kitty_deletes
            ));
        }
        Ok(())
    }
}

/// tmux passthrough for session-authored escapes (the pipeline wraps
/// its own output; place/delete are authored here).
fn wrap_for(caps: &GraphicsCaps, bytes: Vec<u8>) -> Vec<u8> {
    match caps.wrap {
        Some(WrapKind::Tmux) => crate::term::tmux_wrap(&bytes),
        None => bytes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::{Point, Rgba};

    struct Sink(Vec<(Vec<u8>, Point)>);
    impl ExternalSink for Sink {
        fn external_write(&mut self, bytes: &[u8], at: Point) {
            self.0.push((bytes.to_vec(), at));
        }
    }

    fn kitty_caps() -> GraphicsCaps {
        GraphicsCaps {
            wrap: None,
            kitty_graphics: true,
            iterm2_images: false,
            sixel: false,
            sixel_max_registers: None,
            cell_pixel_size: None,
        }
    }

    fn img() -> Bitmap {
        Bitmap::new(4, 4, Rgba::rgb(9, 9, 9))
    }

    fn text(bytes: &[u8]) -> String {
        String::from_utf8_lossy(bytes).into_owned()
    }

    #[test]
    fn kitty_lifecycle_transmit_place_delete() {
        let mut s = ImageSession::new();
        let mut sink = Sink(Vec::new());
        let caps = kitty_caps();
        let rect_a = Rect::new(1, 1, 8, 4);

        // First sync: full transmit (a=T).
        let out = s.sync(&mut sink, 7, 1, &img(), rect_a, &caps);
        assert!(matches!(out, SyncOutcome::Emitted(_)));
        assert_eq!(sink.0.len(), 1);
        assert!(text(&sink.0[0].0).contains("a=T"));

        // Same version + same rect: silence.
        assert!(matches!(
            s.sync(&mut sink, 7, 1, &img(), rect_a, &caps),
            SyncOutcome::Unchanged
        ));
        assert_eq!(sink.0.len(), 1);

        // Same version, new rect: placement escape only, same id.
        let rect_b = Rect::new(10, 2, 6, 3);
        let out = s.sync(&mut sink, 7, 1, &img(), rect_b, &caps);
        assert!(matches!(out, SyncOutcome::Emitted(_)));
        assert_eq!(sink.0.len(), 2);
        let placed = text(&sink.0[1].0);
        assert!(placed.contains("a=p") && placed.contains("i=1"), "{placed}");
        assert!(!placed.contains("a=T"), "no retransmission on move");
        assert_eq!(sink.0[1].1, Point::new(10, 2));

        // New version: delete old data + fresh transmit under a new id.
        let out = s.sync(&mut sink, 7, 2, &img(), rect_b, &caps);
        assert!(matches!(out, SyncOutcome::Emitted(_)));
        assert_eq!(sink.0.len(), 4);
        assert!(text(&sink.0[2].0).contains("d=I"), "free the stale upload");
        assert!(text(&sink.0[3].0).contains("a=T"));

        // Release: the upload dies.
        s.release(&mut sink, 7, &caps);
        assert_eq!(sink.0.len(), 5);
        assert!(text(&sink.0[4].0).contains("a=d"));
        assert_eq!(s.live_slots(), 0);
    }

    #[test]
    fn iterm2_reemits_on_every_change() {
        let mut s = ImageSession::new();
        let mut sink = Sink(Vec::new());
        let caps = GraphicsCaps {
            kitty_graphics: false,
            iterm2_images: true,
            ..kitty_caps()
        };
        let rect = Rect::new(0, 0, 4, 2);
        s.sync(&mut sink, 1, 1, &img(), rect, &caps);
        assert_eq!(sink.0.len(), 1);
        // Unchanged: silent.
        assert!(matches!(
            s.sync(&mut sink, 1, 1, &img(), rect, &caps),
            SyncOutcome::Unchanged
        ));
        // Moved: full re-emit (no placement model — documented cost).
        s.sync(&mut sink, 1, 1, &img(), Rect::new(5, 5, 4, 2), &caps);
        assert_eq!(sink.0.len(), 2);
        assert!(text(&sink.0[1].0).starts_with("\u{1b}]1337;File="));
        // Release writes nothing (nothing addressable to delete).
        s.release(&mut sink, 1, &caps);
        assert_eq!(sink.0.len(), 2);
    }

    #[test]
    fn mosaic_channel_returns_cells_and_tracks() {
        let mut s = ImageSession::new();
        let mut sink = Sink(Vec::new());
        let caps = GraphicsCaps {
            kitty_graphics: false,
            ..kitty_caps()
        };
        let out = s.sync(&mut sink, 3, 1, &img(), Rect::new(0, 0, 2, 1), &caps);
        let SyncOutcome::Cells(r) = out else {
            panic!("mosaic expected")
        };
        assert!(matches!(r.output, ImageOutput::Cells(ref c) if !c.is_empty()));
        assert!(sink.0.is_empty(), "mosaic writes no bytes");
        assert!(matches!(
            s.sync(&mut sink, 3, 1, &img(), Rect::new(0, 0, 2, 1), &caps),
            SyncOutcome::Unchanged
        ));
    }

    #[test]
    fn channel_upgrade_resets_the_slot() {
        let mut s = ImageSession::new();
        let mut sink = Sink(Vec::new());
        let mosaic_caps = GraphicsCaps {
            kitty_graphics: false,
            ..kitty_caps()
        };
        s.sync(&mut sink, 9, 1, &img(), Rect::new(0, 0, 2, 1), &mosaic_caps);
        // Caps upgrade mid-session (late probe reply): same slot goes
        // through the kitty path cleanly.
        let out = s.sync(
            &mut sink,
            9,
            1,
            &img(),
            Rect::new(0, 0, 2, 1),
            &kitty_caps(),
        );
        assert!(matches!(out, SyncOutcome::Emitted(_)));
        assert!(text(&sink.0.last().unwrap().0).contains("a=T"));
    }

    /// Cross-check the session's OWN accounting against REDTEAM's
    /// KittyModel replaying the exact bytes it emitted (cycle-7: the
    /// protocol paths are byte-verified only, so two independent
    /// implementations of "what does the terminal hold now" must
    /// agree at every step — and the invariant checker must hold).
    #[test]
    fn session_accounting_agrees_with_the_kitty_model() {
        use crate::testing::kitty_model::KittyModel;

        for wrapped in [false, true] {
            let mut s = ImageSession::new();
            let mut sink = Sink(Vec::new());
            let mut caps = kitty_caps();
            if wrapped {
                caps.wrap = Some(WrapKind::Tmux);
            }
            let mut model = if wrapped {
                KittyModel::with_tmux_unwrap()
            } else {
                KittyModel::new()
            };
            let mut fed = 0usize;
            let mut step = |s: &ImageSession, sink: &Sink, model: &mut KittyModel, what: &str| {
                for (bytes, _) in &sink.0[fed..] {
                    model.feed(bytes);
                }
                fed = sink.0.len();
                s.check_invariants()
                    .unwrap_or_else(|e| panic!("[{what}] {e}"));
                assert_eq!(
                    s.live_kitty_ids(),
                    model.live_data_ids(),
                    "[{what} wrapped={wrapped}] session vs model disagree on held ids"
                );
                for id in s.live_kitty_ids() {
                    assert_eq!(
                        model.transmit_count(id),
                        1,
                        "[{what}] id {id} transmitted more than once"
                    );
                }
            };

            // Fresh slot; second slot; move; new content; releases.
            s.sync(&mut sink, 1, 1, &img(), Rect::new(0, 0, 4, 2), &caps);
            step(&s, &sink, &mut model, "first transmit");
            s.sync(&mut sink, 2, 1, &img(), Rect::new(6, 0, 4, 2), &caps);
            step(&s, &sink, &mut model, "second slot");
            s.sync(&mut sink, 1, 1, &img(), Rect::new(0, 4, 4, 2), &caps);
            step(&s, &sink, &mut model, "move (a=p)");
            s.sync(&mut sink, 1, 2, &img(), Rect::new(0, 4, 4, 2), &caps);
            step(&s, &sink, &mut model, "new version (delete+retransmit)");
            s.release(&mut sink, 2, &caps);
            step(&s, &sink, &mut model, "release slot 2");
            s.release_all(&mut sink, &caps);
            step(&s, &sink, &mut model, "release all");
            assert_eq!(s.live_slots(), 0);
            assert!(
                model.live_data_ids().is_empty(),
                "terminal still holds pixels"
            );
            let (t, p, d) = s.kitty_traffic();
            assert_eq!(t, d, "every upload freed exactly once by the end");
            assert!(p >= 1, "the move must have used a placement escape");
        }
    }

    #[test]
    fn invariant_checker_catches_broken_accounting() {
        // The checker must FAIL on corrupted state, not just pass on
        // good state (a checker that cannot fail checks nothing).
        let mut s = ImageSession::new();
        let mut sink = Sink(Vec::new());
        let caps = kitty_caps();
        s.sync(&mut sink, 1, 1, &img(), Rect::new(0, 0, 4, 2), &caps);
        assert!(s.check_invariants().is_ok());
        // Forge a duplicate id on a second slot.
        let forged = s.slots.get(&1).cloned().unwrap();
        s.slots.insert(2, forged);
        let err = s.check_invariants().unwrap_err();
        assert!(err.contains("owned by two slots"), "{err}");
        s.slots.remove(&2);
        // Forge an id-less kitty slot.
        s.slots.get_mut(&1).unwrap().kitty_id = None;
        let err = s.check_invariants().unwrap_err();
        assert!(err.contains("without an id"), "{err}");
    }

    #[test]
    fn slot_info_and_byte_slot_census_track_the_channels() {
        let mut s = ImageSession::new();
        let mut sink = Sink(Vec::new());
        let kitty = kitty_caps();
        let mosaic = GraphicsCaps {
            kitty_graphics: false,
            ..kitty_caps()
        };
        s.sync(&mut sink, 1, 1, &img(), Rect::new(0, 0, 4, 2), &kitty);
        s.sync(&mut sink, 2, 1, &img(), Rect::new(8, 0, 4, 2), &kitty);
        assert_eq!(
            s.slot_info(1),
            Some((Channel::Kitty, Rect::new(0, 0, 4, 2)))
        );
        assert_eq!(s.live_byte_slots(), 2);
        assert_eq!(s.slot_info(99), None);

        // A mosaic slot joins the census as a NON-byte slot.
        let mut s2 = ImageSession::new();
        s2.sync(&mut sink, 7, 1, &img(), Rect::new(0, 0, 2, 1), &mosaic);
        assert_eq!(s2.live_byte_slots(), 0, "mosaic lives in the cell model");
        assert_eq!(s2.live_slots(), 1);
    }

    #[test]
    fn invalidate_slot_forces_reemission_but_refuses_kitty() {
        let mut s = ImageSession::new();
        let mut sink = Sink(Vec::new());
        let mosaic = GraphicsCaps {
            kitty_graphics: false,
            ..kitty_caps()
        };
        let rect = Rect::new(0, 0, 2, 1);
        s.sync(&mut sink, 1, 1, &img(), rect, &mosaic);
        assert!(matches!(
            s.sync(&mut sink, 1, 1, &img(), rect, &mosaic),
            SyncOutcome::Unchanged
        ));
        // Invalidate: the same (version, rect) re-renders — the caller
        // repainted the cells under the slot and needs fresh patches.
        s.invalidate_slot(1);
        assert!(matches!(
            s.sync(&mut sink, 1, 1, &img(), rect, &mosaic),
            SyncOutcome::Cells(_)
        ));

        // Kitty slots refuse: their state is terminal image memory —
        // forgetting one would leak the upload and re-transmit under a
        // fresh id. The slot must survive and stay Unchanged.
        let caps = kitty_caps();
        s.sync(&mut sink, 5, 1, &img(), rect, &caps);
        let before = sink.0.len();
        s.invalidate_slot(5);
        assert!(s.slot_info(5).is_some(), "kitty slot must survive");
        assert!(matches!(
            s.sync(&mut sink, 5, 1, &img(), rect, &caps),
            SyncOutcome::Unchanged
        ));
        assert_eq!(sink.0.len(), before, "no retransmission");
        s.check_invariants().unwrap();
    }

    #[test]
    fn tmux_wrap_covers_session_authored_escapes() {
        let mut s = ImageSession::new();
        let mut sink = Sink(Vec::new());
        let mut caps = kitty_caps();
        caps.wrap = Some(WrapKind::Tmux);
        let rect = Rect::new(0, 0, 4, 2);
        s.sync(&mut sink, 5, 1, &img(), rect, &caps);
        // Move: the session-authored a=p must be wrapped too.
        s.sync(&mut sink, 5, 1, &img(), Rect::new(2, 2, 4, 2), &caps);
        for (bytes, _) in &sink.0 {
            assert!(
                bytes.starts_with(b"\x1bPtmux;"),
                "unwrapped escape reached the sink"
            );
        }
        s.release(&mut sink, 5, &caps);
        assert!(sink.0.last().unwrap().0.starts_with(b"\x1bPtmux;"));
    }
}
