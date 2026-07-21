//! Driver image pass (RT4-1): root-surface steal/restore for phase D
//! and the `gfx::ImageSession` reconciliation over dirty/retired image
//! overlays — split from `driver.rs` (file budget); same `impl Driver`
//! surface, same phase story (mosaic cells pre-flatten, protocol bytes
//! through the post-present bracket).

use crate::base::{Point, Rect, Size};
use crate::gfx::ExternalSink;
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

    /// Reconcile image overlays with the terminal through
    /// `gfx::ImageSession` (RT4-1). Runs with the root surface ALREADY
    /// stolen (mosaic patches blit into it); protocol bytes queue for
    /// the post-present bracket via `BufSink`. The session is what makes
    /// this cheap and leak-free: removed slots DELETE their kitty
    /// upload, moves re-place by id (no pixel retransmit), and only a
    /// CONTENT version bump pays a full transmit.
    pub(super) fn render_images(&mut self, root_surface: &mut Surface) {
        /// One dirty image's sync inputs, snapshotted out of the store.
        struct ImageJob {
            key: u64,
            version: u64,
            rect: Rect,
            bitmap: crate::gfx::Bitmap,
        }
        let (retired, jobs): (Vec<u64>, Vec<ImageJob>) = {
            let mut store = self.overlays.store().borrow_mut();
            let retired: Vec<u64> = store.retired_images.drain(..).collect();
            let jobs = store
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
                .collect();
            (retired, jobs)
        };
        if retired.is_empty() && jobs.is_empty() {
            return;
        }
        let gfx_caps = self.caps.graphics();
        let mut sink = BufSink(&mut self.pending_image_bytes);
        for key in retired {
            self.image_session.release(&mut sink, key, &gfx_caps);
        }
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
                    if let crate::gfx::ImageOutput::Cells(patches) = rendered.output {
                        root_surface.blit_mosaic(
                            patches.iter().map(|p| (p.pos, p.ch, p.fg, p.bg)),
                            job.rect.origin(),
                        );
                    }
                }
                // Emitted -> bytes already in the sink; Unchanged -> the
                // terminal still holds the right state, nothing owed.
                crate::gfx::SyncOutcome::Emitted(_) | crate::gfx::SyncOutcome::Unchanged => {}
            }
        }
    }
}
