//! Programmatic open for the select family (backlog 0296): command-
//! summoned pickers (`/theme`, `/model` typed into a composer) need to
//! open a face's popup WITHOUT a trigger-row gesture. [`SelectHandle`]
//! is that verb — a cloneable handle an app keeps next to the face it
//! built:
//!
//! ```ignore
//! let picker = SelectHandle::new();
//! Select::new(theme_options)
//!     .value(theme_ix)
//!     .handle(&picker)
//!     .view(cx);
//! // later, in a command handler:
//! if !picker.open() { /* face unmounted / not painted yet / disabled */ }
//! ```
//!
//! ## The anchor contract (honesty note)
//!
//! A popup needs an anchor rect; gestures capture it from
//! `EventCtx::current_rect`, which does not exist outside dispatch. A
//! programmatic open anchors at the trigger's LAST-PAINTED rect,
//! recorded by the face's draw pass. Consequences, deliberate and
//! documented rather than papered over:
//!
//! - **One frame after mount**: a face that has never painted has no
//!   rect — `open()` returns `false`. Mount-then-open flows should open
//!   on the frame after the face first renders (e.g. from a one-shot
//!   `reactive::after(Duration::ZERO, ..)` posted when the face mounts).
//! - **Same-turn layout moves**: `open()` called before this frame's
//!   layout re-solve anchors at the previous frame's rect. Popups
//!   already dismiss on viewport resize (`DismissReason::Resize`), and
//!   an anchor one frame stale on an unmoved trigger is exact.
//!
//! Disposal safety: the wiring dies with the face's scope (dyn_view
//! regeneration, unmount) — `open()` on a dead face returns `false`,
//! never panics, never opens a popup over a stale tree. A handle wired
//! by a NEWER build of the same face wins over the dying wire (the
//! generation guard below), so dyn_view regenerations rewire cleanly in
//! either disposal order. One face per handle: a second `.handle(&h)`
//! wire simply replaces the first.
//!
//! OWNER: SELECT (0500 family).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::base::Rect;
use crate::reactive::Scope;

/// What a wired face gives the handle: "open now if you can; report
/// whether the popup is open after the attempt".
type OpenFn = Rc<dyn Fn() -> bool>;

#[derive(Default)]
struct HandleInner {
    open: Option<OpenFn>,
    /// Wire generation: each `wire` call bumps it, and the scope-cleanup
    /// unwire only clears ITS OWN generation — a dyn_view regeneration
    /// that wires the new face before the old scope's cleanup runs must
    /// not lose the fresh wire.
    generation: u64,
}

/// Cloneable programmatic-open handle for `Select` / `Combobox` /
/// `MultiSelect` (backlog 0296). Build one, pass it to a face via
/// `.handle(&h)`, call [`SelectHandle::open`] from command handlers or
/// shortcuts. See the module docs for the anchor contract.
#[derive(Clone, Default)]
pub struct SelectHandle {
    inner: Rc<RefCell<HandleInner>>,
}

impl SelectHandle {
    pub fn new() -> SelectHandle {
        SelectHandle::default()
    }

    /// Open the wired face's popup at the trigger's last-painted rect.
    /// Returns `true` when the popup is open after the call (an
    /// already-open popup counts); `false` when it cannot open — face
    /// not mounted (or unmounted), disabled, never painted, no options.
    /// Call from the app thread in event/command context (phase U), the
    /// same place gesture handlers run.
    pub fn open(&self) -> bool {
        // Clone the closure out of the borrow before running it: the
        // open path mounts popup trees (arbitrary user code), which must
        // never execute under this handle's RefCell borrow.
        let open = self.inner.borrow().open.clone();
        match open {
            Some(open) => open(),
            None => false,
        }
    }

    /// Face-side wiring (crate-internal): installs the open closure and
    /// severs it when `cx` (the face's build scope) dies. The generation
    /// guard keeps a stale cleanup from severing a newer wire.
    pub(crate) fn wire(&self, cx: Scope, open: impl Fn() -> bool + 'static) {
        let generation = {
            let mut inner = self.inner.borrow_mut();
            inner.generation += 1;
            inner.open = Some(Rc::new(open));
            inner.generation
        };
        let weak = Rc::downgrade(&self.inner);
        cx.on_cleanup(move || {
            if let Some(inner) = weak.upgrade() {
                let mut inner = inner.borrow_mut();
                if inner.generation == generation {
                    inner.open = None;
                }
            }
        });
    }
}

/// Shared face plumbing: the cell a face's outer element writes its
/// rect into at draw time — the last-laid-out rect programmatic opens
/// anchor at (`None` until the first paint).
pub(crate) fn anchor_cell() -> Rc<Cell<Option<Rect>>> {
    Rc::new(Cell::new(None))
}
