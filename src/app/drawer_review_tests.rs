//! Wave-8 cross-review pins (TABS on 0585) — `#[path]` sibling of
//! `drawer_tests.rs` for the file-size budget; shares its rig through
//! `super::`.

use std::time::Duration;

use crate::base::{Rgba, Size};
use crate::reactive::{create_root, flush_effects};
use crate::ui::text;

use crate::app::overlays::Overlays;

use super::super::{Drawer, DrawerEdge};
use super::{rig, settle};

/// REVIEW (wave 8, TABS on 0585 — verified LATENT defect, fixed): the
/// documented token contract is AT-OPEN ("a mid-open theme switch
/// lands at the next open", drawer_view.rs:5-6), but the resize
/// re-clamp repainted the scrim with the CURRENT theme's overlay
/// token — a theme switch followed by a resize while open would mint
/// a mixed-theme drawer (new veil under an old-token panel). Latent
/// today only because every REGISTRY theme shares one overlay value
/// (`Rgba::BLACK.with_alpha(OVERLAY_ALPHA)`, registry.rs:286);
/// runtime-registered themes (`theme::register`) may diverge, so the
/// veil cell is now captured at open (`Mount::veil`) and reused by
/// the re-clamp. The divergent theme here is a leaked clone — the
/// exact shape a custom registered theme would take.
#[test]
fn scrim_repaint_on_resize_keeps_the_at_open_veil_token() {
    let size = Size::new(80, 24);
    let overlays = rig(size);
    let before = crate::app::current_theme();
    let at_open = before.tokens.overlay;
    let (root, handle) = create_root(|cx| {
        Drawer::new(DrawerEdge::Right)
            .motion(Duration::ZERO)
            .overlays(&overlays)
            .install(cx, |_| text("veiled"))
    });
    handle.open();
    settle();
    let scrim_bg = |overlays: &Overlays| {
        let store = overlays.store();
        let store = store.borrow();
        let z = DrawerEdge::Right.scrim_z();
        let layer = store
            .layers
            .iter()
            .find(|l| l.z() == z)
            .expect("scrim layer");
        layer.surface().get(1, 1).expect("veil cell").bg
    };
    assert_eq!(scrim_bg(&overlays), at_open, "painted at open");

    // A theme whose overlay genuinely differs (custom-theme shape).
    let mut custom = before.clone();
    custom.tokens.overlay = Rgba::WHITE.with_alpha(90);
    assert_ne!(custom.tokens.overlay, at_open);
    let divergent: &'static crate::theme::Theme = Box::leak(Box::new(custom));
    crate::app::set_theme(divergent);
    crate::app::viewport::publish_viewport(Size::new(70, 20)); // resize
    flush_effects(); // the re-clamp repaints the (resized) scrim
    assert_eq!(
        scrim_bg(&overlays),
        at_open,
        "the re-clamp keeps the AT-OPEN veil token (documented rule), \
         never the current theme's"
    );
    crate::app::set_theme(before);
    root.dispose();
}
