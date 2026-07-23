//! App-facing overlay layers: z-ordered compositor layers above the root
//! UI, each with its own surface and one of three content modes —
//! MANUAL (paint through `with_surface`), DRAW (a closure re-run when
//! damaged, phase D, under the draw-purity guard) or TREE (a full
//! `UiTree` mount with its own reactivity, layout and event routing).
//! Image overlays ride the same store and route pixel-protocol bytes
//! through presenter custody (damage contract §6).
//!
//! ## Borrow discipline (the store's one rule)
//!
//! User code (draw closures, tree handlers, Dyn effects) may hold
//! `LayerHandle`s — so NOTHING user-visible runs while the store is
//! borrowed. Phases SWAP a layer's surface out (cheap: Vec steal),
//! release the store, run user code against the stolen surface, swap it
//! back. Handle mutations use `try_borrow_mut`: a mutation attempted
//! while the store is busy (i.e. from inside a draw closure — forbidden
//! by draw purity anyway) is a loud debug panic, a silent no-op in
//! release (the safe failure: a skipped layer op costs one stale frame,
//! a poisoned borrow costs the process).
//!
//! ## Damage + idle
//!
//! Layer moves/fades record their own frame damage (render::Layer);
//! surface writes self-damage; removing a layer damages the ROOT surface
//! under its last bounds (the vacated cells must repaint — a removed
//! layer can no longer speak for them). An overlay world with settled
//! animations, empty queues and no damage costs zero — the toast
//! acceptance test pins idle-zero-bytes after a full slide/fade/remove
//! cycle.

use std::cell::RefCell;
use std::rc::Rc;

use crate::base::{Point, Rect, Size};
use crate::gfx::Bitmap;
use crate::reactive::{request_frame, Scope};
use crate::render::{Cell, Layer, Surface};
use crate::ui::{StyledCanvas, SurfaceCanvas, UiTree, View};

/// The root layer's id: created by the driver, never removable, z = 0.
pub(crate) const ROOT_LAYER_ID: u64 = 0;

type DrawFn = Box<dyn FnMut(&mut dyn StyledCanvas, Rect)>;

pub(crate) enum OverlayContent {
    /// The driver-owned root UI layer.
    Root,
    /// Caller paints via `with_surface`/`damage` only.
    Manual,
    /// Repainted in phase D when flagged: full-surface repaint into a
    /// cleared (transparent) surface.
    Draw {
        paint: Option<DrawFn>,
        needs_paint: bool,
    },
    /// A mounted UI world of its own; `modal` routes ALL input here
    /// while visible (topmost modal wins). `on_outside` fires when a
    /// MODAL overlay swallows a mouse press outside its bounds — the
    /// menu/popup dismiss hook (the press still never reaches lower
    /// trees; dismiss-without-acting is the deliberate semantic).
    Tree {
        tree: UiTree,
        modal: bool,
        on_outside: Option<Box<dyn FnMut()>>,
    },
}

pub(crate) struct OverlayMeta {
    pub id: u64,
    pub content: OverlayContent,
}

pub(crate) struct ImageEntry {
    pub id: u64,
    pub rect: Rect,
    pub bitmap: Bitmap,
    /// CONTENT version for `gfx::ImageSession`: bumped by `set_bitmap`
    /// only — a moved image (same version, new rect) re-places by kitty
    /// id without retransmitting pixels (RT4-1).
    pub version: u64,
    /// Needs a session sync on the next frame.
    pub dirty: bool,
}

#[derive(Default)]
pub(crate) struct OverlayStore {
    /// Parallel arrays: `layers[i]` belongs to `meta[i]`. Contiguous
    /// `Vec<Layer>` because `Compositor::flatten` consumes `&mut [Layer]`
    /// (it z-sorts internally).
    pub layers: Vec<Layer>,
    pub meta: Vec<OverlayMeta>,
    pub images: Vec<ImageEntry>,
    /// Slot keys of removed image overlays, drained by the driver's
    /// image pass into `ImageSession::release` — kitty terminals hold
    /// uploads until told otherwise; dropping the entry alone would leak
    /// terminal-side pixel memory unboundedly (RT4-1).
    pub retired_images: Vec<u64>,
    next_id: u64,
}

impl OverlayStore {
    pub fn index_of(&self, id: u64) -> Option<usize> {
        self.meta.iter().position(|m| m.id == id)
    }

    /// Damage the root surface under `bounds` — used when a layer above
    /// it vanishes and can no longer speak for those cells.
    fn damage_root_under(&mut self, bounds: Rect) {
        if let Some(root) = self.index_of(ROOT_LAYER_ID) {
            let clip = self.layers[root].bounds();
            let rect = bounds.intersect(clip);
            if !rect.is_empty() {
                self.layers[root].surface_mut().add_damage(rect);
            }
        }
    }
}

/// Cloneable handle to the overlay world; owned by `App`, shared with
/// the driver and any component that creates overlays.
#[derive(Clone)]
pub struct Overlays {
    store: Rc<RefCell<OverlayStore>>,
}

impl Default for Overlays {
    fn default() -> Self {
        Overlays::new()
    }
}

impl Overlays {
    pub fn new() -> Overlays {
        Overlays {
            store: Rc::new(RefCell::new(OverlayStore::default())),
        }
    }

    fn create(&self, z: i32, bounds: Rect, content: OverlayContent) -> LayerHandle {
        let mut store = self.store.borrow_mut();
        store.next_id += 1;
        let id = store.next_id;
        let surface = Surface::new(bounds.size(), Cell::EMPTY);
        let mut layer = Layer::new(surface, bounds.origin(), z);
        // A fresh overlay must paint at least once.
        layer.surface_mut().damage_all();
        store.layers.push(layer);
        store.meta.push(OverlayMeta { id, content });
        drop(store);
        request_frame();
        LayerHandle {
            store: Rc::downgrade(&self.store),
            id,
        }
    }

    /// Manual layer: paint whenever you like through the handle.
    pub fn layer(&self, z: i32, bounds: Rect) -> LayerHandle {
        self.create(z, bounds, OverlayContent::Manual)
    }

    /// Draw-closure layer: `paint` re-runs (full surface, transparent
    /// base) whenever the handle is damaged. Runs in phase D under the
    /// draw-purity guard — pure over captured data, `get_untracked` for
    /// peeks, NO layer mutations from inside.
    pub fn layer_draw(
        &self,
        z: i32,
        bounds: Rect,
        paint: impl FnMut(&mut dyn StyledCanvas, Rect) + 'static,
    ) -> LayerHandle {
        self.create(
            z,
            bounds,
            OverlayContent::Draw {
                paint: Some(Box::new(paint)),
                needs_paint: true,
            },
        )
    }

    /// Full UI world on a layer: its own tree, reactivity and focus.
    /// `modal` routes every input event here while visible. The mount is
    /// owned by `cx` — disposing `cx` unmounts the tree; removing the
    /// layer removes the pixels (pair them via `LayerHandle::remove` +
    /// scope disposal, as `app::popups::Modal` does).
    ///
    /// RULING (0230): a MODAL tree owns the keyboard from frame one —
    /// it swallows every key, so its shortcuts must be live immediately.
    /// Opening one establishes initial focus via `UiTree::focus_init`
    /// (autofocus wins, else first focusable, else the content anchor);
    /// non-modal trees stay unfocused until the user clicks in (the
    /// cycle-5 key rule: only a FOCUSED non-modal overlay owns keys).
    pub fn layer_tree(
        &self,
        z: i32,
        bounds: Rect,
        modal: bool,
        cx: Scope,
        view: View,
    ) -> LayerHandle {
        let mut tree = UiTree::new(bounds.size());
        // The tree lays out LAYER-LOCAL; it must know its layer's
        // SCREEN origin so anchor captures inside it (select popups,
        // tooltips, completion carets) translate to screen cells.
        // Seeded BEFORE mount so mount-time focus delivery already
        // carries the truth; `LayerHandle::set_offset` keeps it in
        // sync when the layer moves (modal re-clamp, drawer slide).
        tree.set_layer_origin(bounds.origin());
        tree.mount(cx, view);
        if modal {
            tree.focus_init();
        }
        self.create(
            z,
            bounds,
            OverlayContent::Tree {
                tree,
                modal,
                on_outside: None,
            },
        )
    }

    /// Install (or replace) the outside-press callback on a MODAL tree
    /// overlay: fires when the modal swallows a mouse press outside its
    /// bounds — the dismiss hook menus/popups ride. No-op on non-tree
    /// layers (nothing to dismiss).
    pub fn on_outside_press(&self, handle: &LayerHandle, f: impl FnMut() + 'static) {
        let mut store = self.store.borrow_mut();
        if let Some(i) = store.index_of(handle.id) {
            if let OverlayContent::Tree { on_outside, .. } = &mut store.meta[i].content {
                *on_outside = Some(Box::new(f));
            }
        }
    }

    /// Flip a tree overlay's MODAL input routing (crate-internal: the
    /// drawer releases its trap the instant a close begins — the exit
    /// slide is cosmetics, and keys pressed during it belong to the app
    /// — and re-arms it when a reopen reverses the flight). No-op on
    /// non-tree layers and removed layers. Input dispatch snapshots its
    /// targets per event, so a flip lands on the NEXT event cleanly.
    pub(crate) fn set_tree_modal(&self, handle: &LayerHandle, want: bool) {
        let mut store = self.store.borrow_mut();
        if let Some(i) = store.index_of(handle.id) {
            if let OverlayContent::Tree { modal, .. } = &mut store.meta[i].content {
                *modal = want;
            }
        }
    }

    /// Highest z among live layers (the root layer's 0 when nothing
    /// else exists). THE additive engine delta backlog 0500 specifies
    /// for anchored popups: a panel opened over any modal stack
    /// allocates at `top_z() + 1`, so select-inside-modal-inside-modal
    /// layers correctly where a static z constant cannot.
    ///
    /// Read-only. Returns 0 while the store is mid-phase (layer ops are
    /// forbidden inside draw closures by draw purity anyway). Keep the
    /// `unwrap_or(0)`: it is deliberate defense-in-depth honesty, not a
    /// bug to "fix" into a panic — draw purity already forbids the only
    /// path that could observe it, so a soft 0 can never misplace a
    /// popup on a live path, while a panic here would turn a harmless
    /// impossible-by-doctrine read into a process kill (cycle-2 review
    /// F10, recorded so the intent survives refactors).
    pub fn top_z(&self) -> i32 {
        self.store
            .try_borrow()
            .map(|s| s.layers.iter().map(|l| l.z()).max().unwrap_or(0))
            .unwrap_or(0)
    }

    /// Register an image overlay: rendered through the gfx capability
    /// ladder each time it is dirty. Byte channels emit through
    /// presenter custody post-present; the mosaic fallback blits cells
    /// into the root layer pre-flatten. Repaints beneath a parked
    /// placement: mosaic re-blits (driver repair scan), kitty floats
    /// above cells unharmed; iTerm2/sixel pixels DECAY under
    /// beneath-repaints — re-emission would cost the full payload per
    /// frame, so that repair is deliberate design space (backlog 0660).
    pub fn image(&self, rect: Rect, bitmap: Bitmap) -> ImageHandle {
        let mut store = self.store.borrow_mut();
        store.next_id += 1;
        let id = store.next_id;
        store.images.push(ImageEntry {
            id,
            rect,
            bitmap,
            version: 1,
            dirty: true,
        });
        drop(store);
        request_frame();
        ImageHandle {
            store: Rc::downgrade(&self.store),
            id,
        }
    }

    /// Damage the ROOT layer under `rect` (frame coordinates): the cells
    /// there recomposite from below on the next frame. The repair for a
    /// layer that vacated a VISIBLE region by a route `remove()` cannot
    /// see — an overlay that moved off-screen (or shrank) in the SAME
    /// step it was removed, so its own bounds no longer cover where it
    /// was seen (the instant-close / resize-shrink class, 0585 cycle-3
    /// F1). `remove()` damages under the layer's CURRENT bounds; a mover
    /// must additionally name the region it is leaving behind.
    pub(crate) fn damage_root_under_rect(&self, rect: Rect) {
        if let Ok(mut store) = self.store.try_borrow_mut() {
            store.damage_root_under(rect);
        }
        request_frame();
    }

    // ---- driver plumbing (crate-internal) -------------------------------

    pub(crate) fn store(&self) -> &Rc<RefCell<OverlayStore>> {
        &self.store
    }

    /// Create or resize the root layer (driver enter / resize).
    pub(crate) fn ensure_root(&self, size: Size) {
        let mut store = self.store.borrow_mut();
        match store.index_of(ROOT_LAYER_ID) {
            Some(i) => {
                store.layers[i].surface_mut().resize(size, Cell::EMPTY);
                store.layers[i].surface_mut().damage_all();
            }
            None => {
                let mut layer = Layer::new(Surface::new(size, Cell::EMPTY), Point::ZERO, 0);
                layer.surface_mut().damage_all();
                store.layers.insert(0, layer);
                store.meta.insert(
                    0,
                    OverlayMeta {
                        id: ROOT_LAYER_ID,
                        content: OverlayContent::Root,
                    },
                );
            }
        }
    }

    /// Anything needing a frame? (pending tree work, unflushed draws,
    /// dirty images — layer damage itself is polled by the compositor.)
    pub(crate) fn has_pending_work(&self) -> bool {
        let store = self.store.borrow();
        store.images.iter().any(|i| i.dirty)
            || store.meta.iter().any(|m| match &m.content {
                OverlayContent::Draw { needs_paint, .. } => *needs_paint,
                OverlayContent::Tree { tree, .. } => tree.has_pending_work(),
                _ => false,
            })
    }

    // Input routing (dispatch, outside-press, pointer-cancel) lives in
    // a `#[path]` sibling (file-size split): see overlay_input.rs.
}

#[path = "overlay_input.rs"]
mod input;

impl Overlays {
    /// Drain zero-collapse diagnostics from every overlay tree (the
    /// root tree is drained separately by the driver).
    pub(crate) fn take_collapse_notices(&self) -> Vec<String> {
        let trees: Vec<UiTree> = {
            let store = self.store.borrow();
            store
                .meta
                .iter()
                .filter_map(|m| match &m.content {
                    OverlayContent::Tree { tree, .. } => Some(tree.handle()),
                    _ => None,
                })
                .collect()
        };
        let mut out = Vec::new();
        for mut tree in trees {
            out.extend(tree.take_collapse_notices());
        }
        out
    }

    /// Phase L for overlay trees.
    pub(crate) fn layout_all(&self) {
        let trees: Vec<UiTree> = {
            let store = self.store.borrow();
            store
                .meta
                .iter()
                .filter_map(|m| match &m.content {
                    OverlayContent::Tree { tree, .. } => Some(tree.handle()),
                    _ => None,
                })
                .collect()
        };
        for mut tree in trees {
            tree.layout();
        }
    }

    /// Phase D for overlay content. The caller already holds the
    /// draw-phase guard? NO — trees hold it themselves in draw_damaged;
    /// Draw closures get it here. Surfaces are SWAPPED OUT so user code
    /// never runs under the store borrow.
    pub(crate) fn draw_all(&self) {
        let ids: Vec<u64> = {
            let store = self.store.borrow();
            store
                .meta
                .iter()
                .filter(|m| m.id != ROOT_LAYER_ID)
                .map(|m| m.id)
                .collect()
        };
        for id in ids {
            // Steal: surface + whatever content needs running.
            enum Job {
                Tree(UiTree),
                Draw(DrawFn),
                Skip,
            }
            let (mut surface, job, bounds) = {
                let mut store = self.store.borrow_mut();
                let Some(i) = store.index_of(id) else {
                    continue;
                };
                if !store.layers[i].visible() {
                    continue; // invisible layers keep stale pixels cheaply
                }
                let bounds = store.layers[i].bounds();
                let job = match &mut store.meta[i].content {
                    OverlayContent::Tree { tree, .. } => {
                        if tree.has_pending_work() || tree.needs_layout() {
                            Job::Tree(tree.handle())
                        } else {
                            // No pending damage — check tree damage below
                            // anyway via take_damage (cheap when empty).
                            Job::Tree(tree.handle())
                        }
                    }
                    OverlayContent::Draw { paint, needs_paint } => {
                        if *needs_paint {
                            *needs_paint = false;
                            match paint.take() {
                                Some(p) => Job::Draw(p),
                                None => Job::Skip,
                            }
                        } else {
                            Job::Skip
                        }
                    }
                    _ => Job::Skip,
                };
                if matches!(job, Job::Skip) {
                    continue;
                }
                let surface = std::mem::replace(
                    store.layers[i].surface_mut(),
                    Surface::new(Size::ZERO, Cell::EMPTY),
                );
                (surface, job, bounds)
            };
            // User code runs HERE, store released.
            match job {
                Job::Tree(mut tree) => {
                    tree.layout();
                    let damage = tree.take_damage();
                    if !damage.is_empty() {
                        // Clear to transparent (compositor blends what the
                        // tree leaves unpainted), then repaint regions.
                        for &rect in &damage {
                            surface.fill_rect(rect, Cell::EMPTY);
                        }
                        let mut canvas = SurfaceCanvas::new(&mut surface);
                        tree.draw_damaged(&mut canvas, &damage);
                    }
                    let mut store = self.store.borrow_mut();
                    if let Some(i) = store.index_of(id) {
                        *store.layers[i].surface_mut() = surface;
                    }
                }
                Job::Draw(mut paint) => {
                    surface.clear(Cell::EMPTY);
                    {
                        let mut canvas = SurfaceCanvas::new(&mut surface);
                        let _guard = crate::reactive::enter_draw_phase();
                        // Draw-closure layers paint layer-local too:
                        // publish their screen origin like tree draws
                        // do, so `ui::layer_origin()` stays truthful.
                        let _origin = crate::ui::publish_layer_origin(bounds.origin());
                        paint(&mut canvas, Rect::from_size(bounds.size()));
                    }
                    let mut store = self.store.borrow_mut();
                    if let Some(i) = store.index_of(id) {
                        *store.layers[i].surface_mut() = surface;
                        if let OverlayContent::Draw { paint: slot, .. } = &mut store.meta[i].content
                        {
                            *slot = Some(paint);
                        }
                    }
                }
                Job::Skip => {}
            }
        }
    }
}
// Layer/image handles live in a `#[path]` sibling (file-size split).
#[path = "overlay_handles.rs"]
mod handles;
pub use handles::{ImageHandle, LayerHandle};

#[cfg(test)]
#[path = "overlay_tests.rs"]
mod tests;
