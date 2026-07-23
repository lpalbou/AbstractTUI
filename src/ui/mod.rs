//! Component model: declarative view tree, mounting, event routing
//! (capture -> target -> bubble), hit testing, focus management, keymaps
//! and shortcuts. Components are plain functions over reactive scopes
//! returning `View` blueprints; re-render is driven by signals, scoped
//! to the `Dyn` region that read them.
//!
//! Owner: REACT. Contract notes live on the types; the architectural
//! rationale is in `docs/design/reactive-ui.md`.
//!
//! ## Component pattern
//!
//! ```ignore
//! fn counter(cx: Scope, start: i64) -> View {
//!     let count = cx.signal(start);
//!     Element::new()
//!         .style(Style::row().gap(1))
//!         .focusable()
//!         .on_event(move |_ctx, ev| {
//!             if let UiEvent::Key(k) = ev {
//!                 if k.key == Key::Char('+') { count.update(|c| *c += 1); }
//!             }
//!         })
//!         .child(dyn_view(Style::default(), move || text(format!("count: {}", count.get()))))
//!         .build()
//! }
//! ```

mod access;
mod canvas;
mod click;
pub mod compose;
mod draw;
mod event;
mod focus;
mod mount;
mod tree;
mod view;

pub use access::{focus_affordance_visible, AccessEntry, AccessSnapshot, Role};
pub use canvas::{BufferCanvas, Canvas, ClippedCanvas, StyledCanvas, SurfaceCanvas};
pub use click::{
    event_time, set_event_time, ClickChain, DEFAULT_CLICK_TOLERANCE, DEFAULT_CLICK_WINDOW,
};
pub use compose::Callback;
pub use event::{
    EventCtx, Key, KeyChord, KeyEvent, Mods, MouseButton, MouseEvent, MouseKind, Phase, UiEvent,
};
pub(crate) use tree::publish_layer_origin;
pub use tree::{layer_origin, UiTree, ViewId};
pub use view::{
    dyn_view, dyn_view_scoped, styled_text, text, DrawFn, Element, HandlerFn, ShortcutFn, View,
};

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
