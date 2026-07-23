//! The overlay handles: [`LayerHandle`] (layer mutations, surface +
//! tree access, removal) and [`ImageHandle`] (parked protocol images).
//! `#[path]` sibling of overlays.rs (file-size split) — both types
//! re-export from `app::overlays` unchanged.
//!
//! OWNER: OVERLAY.

use std::cell::RefCell;
use std::rc::Weak;

use crate::base::{Point, Rect};
use crate::gfx::Bitmap;
use crate::reactive::request_frame;
use crate::render::{Blend, CellShader, ColorTransform, Layer, Surface};
use crate::ui::UiTree;

use super::{ImageEntry, OverlayContent, OverlayStore};
/// Handle to one overlay layer. `Clone`; all mutations request a frame.
/// Weak-backed: outliving the app is safe (ops become no-ops).
#[derive(Clone)]
pub struct LayerHandle {
    pub(super) store: Weak<RefCell<OverlayStore>>,
    pub(super) id: u64,
}

impl LayerHandle {
    fn with_layer<R>(&self, f: impl FnOnce(&mut Layer) -> R) -> Option<R> {
        let store = self.store.upgrade()?;
        let mut store = match store.try_borrow_mut() {
            Ok(s) => s,
            Err(_) => {
                // The store is mid-phase (drawing): mutating layers from
                // draw closures violates draw purity. Loud in debug.
                if cfg!(debug_assertions) {
                    panic!(
                        "abstracttui overlays: LayerHandle mutation while the overlay store \
                         is busy — layer ops are forbidden inside draw closures (draw is pure; \
                         mutate from effects/handlers instead)"
                    );
                }
                return None;
            }
        };
        let i = store.index_of(self.id)?;
        let r = f(&mut store.layers[i]);
        drop(store);
        request_frame();
        Some(r)
    }

    pub fn set_offset(&self, offset: Point) {
        self.with_layer(|l| l.set_origin(offset));
    }

    pub fn set_opacity(&self, opacity: f32) {
        self.with_layer(|l| l.set_opacity(opacity));
    }

    pub fn set_visible(&self, visible: bool) {
        self.with_layer(|l| l.set_visible(visible));
    }

    pub fn set_blend(&self, blend: Blend) {
        self.with_layer(|l| l.set_blend(blend));
    }

    pub fn set_color_transform(&self, transform: ColorTransform) {
        self.with_layer(|l| l.set_color_transform(transform));
    }

    pub fn set_shader(&self, shader: Option<Box<dyn CellShader>>) {
        self.with_layer(|l| l.set_shader(shader));
    }

    /// Advance the layer's shader clock — an animated shader is an
    /// ANIMATION: drive this from `reactive::animate`/frame tasks so it
    /// is billed as frame requests (§4).
    pub fn set_shader_t(&self, t: f32) {
        self.with_layer(|l| l.set_shader_t(t));
    }

    /// Paint directly (manual layers). The surface is layer-local;
    /// writes self-damage.
    pub fn with_surface<R>(&self, f: impl FnOnce(&mut Surface) -> R) -> Option<R> {
        self.with_layer(|l| f(l.surface_mut()))
    }

    /// Request a repaint: full-surface for Draw layers, damage-all for
    /// manual/tree layers.
    pub fn damage(&self) {
        let Some(store) = self.store.upgrade() else {
            return;
        };
        let mut store = match store.try_borrow_mut() {
            Ok(s) => s,
            Err(_) => return,
        };
        if let Some(i) = store.index_of(self.id) {
            if let OverlayContent::Draw { needs_paint, .. } = &mut store.meta[i].content {
                *needs_paint = true;
            }
            store.layers[i].surface_mut().damage_all();
        }
        drop(store);
        request_frame();
    }

    pub fn bounds(&self) -> Option<Rect> {
        let store = self.store.upgrade()?;
        let store = store.try_borrow().ok()?;
        let i = store.index_of(self.id)?;
        Some(store.layers[i].bounds())
    }

    /// The UI tree mounted on this layer (tree layers only): a handle
    /// onto the LIVE tree — shared core, so focus moves, dispatches and
    /// inspection act on the real thing. `None` for manual/draw layers,
    /// after removal, or while the store is mid-phase.
    pub fn tree(&self) -> Option<UiTree> {
        let store = self.store.upgrade()?;
        let store = store.try_borrow().ok()?;
        let i = store.index_of(self.id)?;
        match &store.meta[i].content {
            OverlayContent::Tree { tree, .. } => Some(tree.handle()),
            _ => None,
        }
    }

    pub fn is_alive(&self) -> bool {
        self.bounds().is_some()
    }

    /// Remove the layer; the vacated region repaints from below (root
    /// damage). Safe to call twice.
    pub fn remove(&self) {
        let Some(store) = self.store.upgrade() else {
            return;
        };
        let mut store = match store.try_borrow_mut() {
            Ok(s) => s,
            Err(_) => return,
        };
        if let Some(i) = store.index_of(self.id) {
            let bounds = store.layers[i].bounds();
            store.layers.remove(i);
            store.meta.remove(i);
            store.damage_root_under(bounds);
        }
        drop(store);
        request_frame();
    }
}

/// Handle to a registered image overlay.
#[derive(Clone)]
pub struct ImageHandle {
    pub(super) store: Weak<RefCell<OverlayStore>>,
    pub(super) id: u64,
}

impl ImageHandle {
    fn with_entry(&self, f: impl FnOnce(&mut ImageEntry)) {
        let Some(store) = self.store.upgrade() else {
            return;
        };
        let mut store = match store.try_borrow_mut() {
            Ok(s) => s,
            Err(_) => return,
        };
        if let Some(entry) = store.images.iter_mut().find(|e| e.id == self.id) {
            f(entry);
            entry.dirty = true;
        }
        drop(store);
        request_frame();
    }

    /// New pixels: bumps the CONTENT version (full retransmit on
    /// protocol channels — the pixels really changed).
    pub fn set_bitmap(&self, bitmap: Bitmap) {
        self.with_entry(|e| {
            e.bitmap = bitmap;
            e.version += 1;
        });
    }

    /// New placement, same pixels: version UNCHANGED — kitty re-places
    /// by id (tiny escape), only id-less channels re-emit.
    pub fn set_rect(&self, rect: Rect) {
        self.with_entry(|e| e.rect = rect);
    }

    pub fn remove(&self) {
        let Some(store) = self.store.upgrade() else {
            return;
        };
        let mut store = match store.try_borrow_mut() {
            Ok(s) => s,
            Err(_) => return,
        };
        if let Some(i) = store.images.iter().position(|e| e.id == self.id) {
            let rect = store.images[i].rect;
            let key = store.images[i].id;
            store.images.remove(i);
            // The session must free the terminal-side upload (kitty) —
            // the driver drains this on its next image pass (RT4-1).
            store.retired_images.push(key);
            store.damage_root_under(rect);
        }
        drop(store);
        request_frame();
    }
}
