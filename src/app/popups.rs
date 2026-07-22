//! Modal and Toast: the two overlay primitives apps reach for first.
//!
//! Both ride `app::overlays` (they hold no engine privileges). They live
//! app-side rather than in `ui` because they NEED the overlay store —
//! and `ui` sits below `app` in the layer map (integrator ruling R4-1:
//! no upward imports, even textual ones).
//!
//! Style: modal panel ground is the `overlay` token over a focus-trapped
//! tree (Tab cycles inside; every input is swallowed while open — the
//! §3.2 modal pattern); toasts are `surface_raised` chips in the
//! top-right that slide+fade in via signal transitions, park for their
//! duration (zero frames while parked — the dismiss timer sleeps, it
//! does not poll), then slide out and REMOVE their layer (idle returns
//! to zero bytes; acceptance-pinned).

use std::time::Duration;

use crate::anim::Easing;
use crate::base::{Point, Rect, Size};
use crate::layout::{Dimension, Style as LayoutStyle};
use crate::reactive::{after, animate, Scope};
use crate::render::Style;
use crate::theme::TokenId;
use crate::ui::{Element, View};

use super::overlays::{LayerHandle, Overlays};
use super::theme::current_theme;

/// Overlay z bands: apps layer below 1000; modals stack above content,
/// toasts above modals. These bands order the STATIC primitives only:
/// the owned [`Popup`](super::anchored::Popup) allocates dynamically at
/// `Overlays::top_z() + 1` and may therefore transiently layer above a
/// live toast — deliberate, and safe, because toasts are passive
/// non-interactive draw layers while a popup is the key-owning surface
/// the user is operating (cycle-3 addendum amendment in
/// reviews/study/platform-on-appkits.md).
pub const MODAL_Z: i32 = 1000;
pub const TOAST_Z: i32 = 2000;

/// A centered, focus-trapped overlay panel. Input is fully owned while
/// open (mouse outside the panel is swallowed, keys route inside, Tab
/// cycles the panel's focusables). Close explicitly — dropping the
/// handle does NOT close (handles are `Clone`; lifetime is the app's
/// decision, not drop order's).
pub struct Modal {
    layer: LayerHandle,
    scope: Scope,
}

impl Modal {
    /// Open over `viewport` with a panel of `size`. `build` receives the
    /// modal's own scope: state created there dies on close.
    ///
    /// Layout guarantee (0240): inside the fixed panel, a declared fixed
    /// size is a PROMISE — content nodes with `width`/`height:
    /// Cells(n)` and no explicit minimum get `min = n`, so overflow
    /// pressure from a large middle (a long transcript in a `Scroll`)
    /// squeezes the flexible children instead of silently erasing the
    /// title/button/hint rows the modal exists to show. Opt out per
    /// node with an explicit `min_h(0)`/`min_w(0)` (or any explicit
    /// minimum). Blueprint-time: styles produced later by `dyn_view`
    /// build closures or `style_signal` are the author's own.
    pub fn open(
        overlays: &Overlays,
        cx: Scope,
        viewport: Size,
        size: Size,
        build: impl FnOnce(Scope) -> View,
    ) -> Modal {
        let scope = cx.child();
        let bounds = Rect::new(
            ((viewport.w - size.w) / 2).max(0),
            ((viewport.h - size.h) / 2).max(0),
            size.w.min(viewport.w),
            size.h.min(viewport.h),
        );
        let tokens = &current_theme().tokens;
        let ground = tokens.get(TokenId::Overlay);
        let ink = tokens.get(TokenId::Text);
        let mut content = build(scope);
        content.for_each_style_mut(&mut floor_declared_size);
        let panel = Element::new()
            .style(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0))
                    .padding(crate::layout::Edges::all(1)),
            )
            .role(crate::ui::Role::Dialog)
            .focus_trap()
            .draw(move |canvas, rect| {
                canvas.fill_styled(rect, ' ', &Style::new().fg(ink).bg(ground));
            })
            .child(content)
            .build();
        let layer = overlays.layer_tree(MODAL_Z, bounds, true, scope, panel);
        Modal { layer, scope }
    }

    pub fn layer(&self) -> &LayerHandle {
        &self.layer
    }

    /// Remove the panel and dispose its state. The vacated region
    /// repaints from the layers below. Idempotent (`&self`: a shortcut
    /// INSIDE the modal — Esc-to-close — holds a shared handle; layer
    /// removal and scope disposal both shrug at a second call).
    pub fn close(&self) {
        self.layer.remove();
        self.scope.dispose();
    }

    /// A second handle to the SAME modal (layer + scope are shared;
    /// closing either closes both). Lets a widget inside the modal —
    /// Esc shortcuts, close buttons — hold its own closer.
    pub fn share(&self) -> Modal {
        Modal {
            layer: self.layer.clone(),
            scope: self.scope,
        }
    }
}

/// The 0240 floor: a declared `Cells` extent with no explicit minimum
/// becomes its own minimum. Flex shrink respects minimums (the solver's
/// freeze loop), so under overflow the flexible children absorb the loss
/// and the fixed rows stay visible. `Some(_)` — including an explicit
/// `min_h(0)` — is the author's word and is never overridden.
fn floor_declared_size(style: &mut LayoutStyle) {
    if let Dimension::Cells(h) = style.height {
        if style.min_height.is_none() {
            style.min_height = Some(h);
        }
    }
    if let Dimension::Cells(w) = style.width {
        if style.min_width.is_none() {
            style.min_width = Some(w);
        }
    }
}

/// Fire-and-forget toast. Returns the layer handle for early dismissal
/// (`remove`); otherwise it manages its own lifecycle.
pub struct Toast;

impl Toast {
    /// ~200ms slide+fade in, parked for `duration`, slide+fade out,
    /// layer removed. All timing rides signal transitions + one one-shot
    /// timer — parked toasts cost zero wakeups.
    pub fn show(
        overlays: &Overlays,
        cx: Scope,
        viewport: Size,
        message: impl Into<String>,
        duration: Duration,
    ) -> LayerHandle {
        Self::show_with_motion(
            overlays,
            cx,
            viewport,
            message,
            duration,
            Duration::from_millis(200),
        )
    }

    /// `show` with an explicit slide/fade duration (tests drive frames
    /// on their own clock and want short flights).
    pub fn show_with_motion(
        overlays: &Overlays,
        cx: Scope,
        viewport: Size,
        message: impl Into<String>,
        duration: Duration,
        motion: Duration,
    ) -> LayerHandle {
        let message = message.into();
        let tokens = &current_theme().tokens;
        let ground = tokens.get(TokenId::SurfaceRaised);
        let ink = tokens.get(TokenId::Accent);
        let w = (crate::text::width(&message) + 2).min(viewport.w.max(1));
        // Rest position: top-right, one row down; entry slides from the
        // row above (off-screen-ish) while fading in.
        let rest = Point::new((viewport.w - w - 1).max(0), 1);
        let start = Point::new(rest.x, 0);
        let bounds = Rect::new(start.x, start.y, w, 1);

        let text = message.clone();
        let layer = overlays.layer_draw(TOAST_Z, bounds, move |canvas, rect| {
            let style = Style::new().fg(ink).bg(ground);
            canvas.fill_styled(rect, ' ', &style);
            canvas.print_styled(Point::new(rect.x + 1, rect.y), &text, &style);
        });

        // Progress 0 -> 1 (in), later 1 -> 0 (out); the follower drives
        // offset + opacity per animation frame.
        let progress = cx.signal(0.0f32);
        let eased = animate(cx, progress, Easing::EaseOut, motion);
        let closing = cx.signal(false);
        let handle = layer.clone();
        cx.effect_labeled("toast-motion", move || {
            let t = eased.get().clamp(0.0, 1.0);
            if !handle.is_alive() {
                return; // dismissed early
            }
            let y = start.y as f32 + (rest.y - start.y) as f32 * t;
            handle.set_offset(Point::new(rest.x, y.round() as i32));
            handle.set_opacity(t);
            if closing.get() && t <= f32::EPSILON {
                // Slide-out landed: the layer's job is done.
                handle.remove();
            }
        });
        progress.set(1.0);
        after(duration, move || {
            closing.set(true);
            progress.set(0.0);
        });
        layer
    }
}
