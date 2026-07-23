//! Driver half of the screenshot capability (control-plane/0370): the
//! capture surface over the composed frame + the phase-U drain serving
//! [`super::screenshot::request_screenshot`] callbacks.
//!
//! Split from `driver.rs` (file-size budget), the `driver_images` /
//! `driver_suspend` sibling pattern.

use crate::gfx::Channel;
use crate::render::Screenshot;

use super::driver::Driver;

impl Driver {
    /// The screen as **last presented**: a pure read of the composed
    /// frame (the flatten target phase P presents from) — no re-render,
    /// no damage side effects, callable between turns at any time.
    /// Before the first rendered frame the capture is honestly blank.
    ///
    /// Byte-channel image placements (kitty / iTerm2 / sixel) are
    /// stamped as [`Screenshot::pixel_regions`] from the live session
    /// bookkeeping: the terminal shows pixels there that the cell plane
    /// cannot represent, and the capture says so instead of pretending
    /// the cells beneath are the picture. Mosaic images ARE cells and
    /// capture as themselves.
    pub fn screenshot(&self) -> Screenshot {
        let mut shot = Screenshot::from_surface(&self.frame);
        let store = self.overlays.store().borrow();
        for img in store.images.iter() {
            if let Some((channel, rect)) = self.image_session.slot_info(img.id) {
                if channel != Channel::Mosaic {
                    shot.add_pixel_region(rect);
                }
            }
        }
        shot
    }

    /// Phase-U drain: serve every pending [`request_screenshot`]
    /// callback with the last presented frame. Runs BEFORE the frame
    /// decision, so a request from this turn's own key handler is served
    /// this turn — with the screen as the user saw it when the key
    /// landed (this turn's render, if any, happens after).
    ///
    /// [`request_screenshot`]: super::screenshot::request_screenshot
    pub(super) fn serve_screenshot_requests(&self) {
        for cb in super::screenshot::take_screenshot_requests() {
            cb(self.screenshot());
        }
    }
}
