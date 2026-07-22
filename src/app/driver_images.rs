//! Driver image pass (RT4-1): root-surface steal/restore for phase D
//! and the `gfx::ImageSession` reconciliation over dirty/retired image
//! overlays — split from `driver.rs` (file budget); same `impl Driver`
//! surface, same phase story (mosaic cells pre-flatten, protocol bytes
//! through the post-present bracket).
//!
//! Study-2 lifecycle truth (MEDIA): a placement CHANGE leaves state
//! behind that no later phase repairs on its own —
//!
//! - mosaic patches were blitted INTO the root surface, so the vacated
//!   cells hold image pixels the tree never repaints (tree damage and
//!   surface damage are different sets);
//! - iTerm2/sixel pixels live in the TERMINAL over cells the model
//!   never changed, so the diff's equality check suppresses exactly the
//!   repaint that would erase them;
//! - a kitty placement is replaced by id (session lane), needing no
//!   cell repair.
//!
//! [`Driver::pre_image_pass`] runs before phase D and settles all of
//! it: vacated rects fold into the tree damage (clear + redraw from
//! truth) and, for cursor-paint byte channels, poison `prev` so the
//! diff re-emits cells the model believes unchanged. It also re-blits
//! mosaic placements whose cells any damage rect will repaint (study-2
//! review: a parked mosaic image used to corrode row by row as content
//! changed beneath it — the re-blit is wire-free because the diff
//! suppresses the byte-identical cells).

use crate::base::{Point, Rect, Size};
use crate::gfx::{choose_channel, Channel, ExternalSink};
use crate::render::{Cell, Surface};

use super::driver::Driver;

/// `ExternalSink` over the driver's pending-payload queue: the session
/// writes during phase D2, the queue empties through presenter custody
/// AFTER the cell diff (damage contract §6) — GFX3D's 6-line adapter.
pub(super) struct BufSink<'a>(pub(super) &'a mut Vec<(Vec<u8>, Point)>);

impl ExternalSink for BufSink<'_> {
    fn external_write(&mut self, bytes: &[u8], at: Point) {
        self.0.push((bytes.to_vec(), at));
    }
}

impl Driver {
    pub(super) fn steal_root_surface(&mut self) -> Surface {
        let mut store = self.overlays.store().borrow_mut();
        let i = store
            .index_of(crate::app::overlays::ROOT_LAYER_ID)
            .expect("root layer exists after Driver::new");
        std::mem::replace(
            store.layers[i].surface_mut(),
            Surface::new(Size::ZERO, Cell::EMPTY),
        )
    }

    pub(super) fn restore_root_surface(&mut self, surface: Surface) {
        let mut store = self.overlays.store().borrow_mut();
        if let Some(i) = store.index_of(crate::app::overlays::ROOT_LAYER_ID) {
            *store.layers[i].surface_mut() = surface;
        }
    }

    /// Placement-change bookkeeping, run BEFORE phase D (the tree draw)
    /// so this frame repaints truthfully:
    ///
    /// 1. retired slots release their session state (kitty deletes ride
    ///    the pending-bytes queue) and their rects vacate;
    /// 2. dirty slots whose rect or channel changed vacate their PRIOR
    ///    rect (the session still remembers it);
    /// 3. every vacated rect folds into `damage` (phase D clears +
    ///    redraws it from tree truth) and — when the terminal holds
    ///    pixels there the cell model cannot see (iTerm2/sixel) — the
    ///    matching `prev` region is poisoned so the diff re-emits cells
    ///    it believes unchanged;
    /// 4. repair scan: slots whose pixels a repaint will erase are
    ///    invalidated and re-marked dirty so phase D2 re-emits them.
    ///    MOSAIC placements repair against ANY damage rect (their
    ///    patches live in the root surface, so every clear+redraw
    ///    beneath them erases patches; the re-blit is wire-free — the
    ///    diff suppresses the byte-identical cells). iTerm2/sixel repair
    ///    only against rects THIS PASS vacated: a re-emission is the
    ///    full protocol payload, so beneath-repaint decay of those
    ///    channels stays a documented limit (backlog 0660's design
    ///    space). Kitty floats above cells and needs nothing.
    ///
    /// The pass runs no user code: its inputs (dirty flags, retirement
    /// queue, tree damage) were all written during phase U, so the
    /// frame-epoch rule (damage contract §2) holds — this is the same
    /// class of driver-owned translation as layout's geometry damage.
    pub(super) fn pre_image_pass(&mut self, damage: &mut Vec<Rect>) {
        let gfx_caps = self.caps.graphics();
        let next_channel = choose_channel(&gfx_caps);

        // Snapshot decisions first; nothing mutates under the borrow.
        let (retired, moved): (Vec<u64>, Vec<u64>) = {
            let mut store = self.overlays.store().borrow_mut();
            let retired: Vec<u64> = store.retired_images.drain(..).collect();
            let moved = store
                .images
                .iter()
                .filter(|e| e.dirty)
                .filter(|e| {
                    self.image_session
                        .slot_info(e.id)
                        .is_some_and(|(ch, rect)| ch != next_channel || rect != e.rect)
                })
                .map(|e| e.id)
                .collect();
            (retired, moved)
        };

        let mut vacated: Vec<(Channel, Rect)> = Vec::new();
        for key in moved {
            vacated.extend(self.image_session.slot_info(key));
        }
        for key in retired {
            vacated.extend(self.image_session.slot_info(key));
            let mut sink = BufSink(&mut self.pending_image_bytes);
            self.image_session.release(&mut sink, key, &gfx_caps);
        }

        for &(channel, rect) in &vacated {
            // Tree truth repaints the vacated cells this frame.
            damage.push(rect);
            if matches!(channel, Channel::Iterm2 | Channel::Sixel) {
                // The terminal shows image pixels over cells the model
                // never changed: only a poisoned prev makes the diff
                // re-emit them (equality would suppress the erase).
                self.poison_prev_rect(rect);
            }
        }
        if damage.is_empty() {
            // Nothing repaints this frame, so no placement can lose
            // pixels: the steady state costs zero work and zero allocs.
            return;
        }

        // Step 4: the repair scan (channel-aware, see the doc above).
        let repair: Vec<u64> = {
            let store = self.overlays.store().borrow();
            store
                .images
                .iter()
                .filter(|e| {
                    match self.image_session.slot_info(e.id) {
                        // Kitty placements float above cells; a cell
                        // repaint beneath them erases nothing.
                        Some((Channel::Kitty, _)) | None => false,
                        Some((Channel::Mosaic, _)) => damage.iter().any(|r| r.intersects(e.rect)),
                        Some((Channel::Iterm2 | Channel::Sixel, _)) => {
                            vacated.iter().any(|&(_, r)| r.intersects(e.rect))
                        }
                    }
                })
                .map(|e| e.id)
                .collect()
        };
        for key in repair {
            self.image_session.invalidate_slot(key);
            let mut store = self.overlays.store().borrow_mut();
            if let Some(e) = store.images.iter_mut().find(|e| e.id == key) {
                e.dirty = true;
            }
        }
    }

    /// Reconcile image overlays with the terminal through
    /// `gfx::ImageSession` (RT4-1). Runs with the root surface ALREADY
    /// stolen (mosaic patches blit into it); protocol bytes queue for
    /// the post-present bracket via `BufSink`. The session is what makes
    /// this cheap and leak-free: removed slots DELETE their kitty
    /// upload, moves re-place by id (no pixel retransmit), and only a
    /// CONTENT version bump pays a full transmit. Retired slots were
    /// already settled by [`Driver::pre_image_pass`] this frame.
    pub(super) fn render_images(&mut self, root_surface: &mut Surface) {
        /// One dirty image's sync inputs, snapshotted out of the store.
        struct ImageJob {
            key: u64,
            version: u64,
            rect: Rect,
            bitmap: crate::gfx::Bitmap,
        }
        let jobs: Vec<ImageJob> = {
            let mut store = self.overlays.store().borrow_mut();
            store
                .images
                .iter_mut()
                .filter(|e| e.dirty)
                .map(|e| {
                    e.dirty = false;
                    ImageJob {
                        key: e.id,
                        version: e.version,
                        rect: e.rect,
                        bitmap: e.bitmap.clone(),
                    }
                })
                .collect()
        };
        if jobs.is_empty() {
            return;
        }
        let gfx_caps = self.caps.graphics();
        let mut warnings: Vec<String> = Vec::new();
        let mut sink = BufSink(&mut self.pending_image_bytes);
        for job in jobs {
            match self.image_session.sync(
                &mut sink,
                job.key,
                job.version,
                &job.bitmap,
                job.rect,
                &gfx_caps,
            ) {
                crate::gfx::SyncOutcome::Cells(rendered) => {
                    warnings.extend(rendered.warnings);
                    if let crate::gfx::ImageOutput::Cells(patches) = rendered.output {
                        // Patch positions are already SCREEN cells (the
                        // pipeline blits its grid at rect.origin() —
                        // pipeline.rs render, Mosaic arm), so the blit
                        // origin here must be ZERO: adding the rect
                        // origin again double-offset every overlay
                        // image (study-2 finding; the old test's
                        // "painted" assertion was satisfied by the
                        // theme-cleared background and never caught it).
                        root_surface.blit_mosaic(
                            patches.iter().map(|p| (p.pos, p.ch, p.fg, p.bg)),
                            Point::ZERO,
                        );
                    }
                }
                // Bytes already in the sink; the ladder's degradation
                // labels still surface (charter: never silent).
                crate::gfx::SyncOutcome::Emitted(rendered) => warnings.extend(rendered.warnings),
                // Terminal still holds the right state, nothing owed.
                crate::gfx::SyncOutcome::Unchanged => {}
            }
        }
        // Forward each DISTINCT ladder warning once per run: images sync
        // every dirty frame, and a repeated `#FALLBACK` line per frame
        // would bury the notices lane it is meant to inform.
        for w in warnings {
            if self.image_notice_seen.insert(w.clone()) {
                self.pending_image_notices.push(w);
            }
        }
    }
}
