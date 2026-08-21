//! field-agora 0910, slice 1: WHY a consumer cannot build a
//! child-offset readback for `Scroll` itself.
//!
//! The field report (agora-tui's message pane) keyboard-selects through
//! a `Scroll` column of mixed-height `Disclosure` widgets and must keep
//! the selected card in view. `Scroll` exposes no "where did this child
//! land" seam, so the app maintains `offset_of_card` — a hand-rolled
//! height model (folded card = 1 row, expanded = 2 + capped body rows)
//! that recomputes the y-offset and writes the bound offset itself.
//!
//! The obvious app-side substitute is the engine's own idiom: attach a
//! paint closure to each child and record the solved rect it is handed
//! (this is exactly how `Scroll::extent_signal` works — `size_probe`
//! plus a signal, `scroll.rs`). It cannot work here, and the reason is
//! structural rather than incidental: the child an ensure-visible verb
//! needs to locate is BY DEFINITION the one outside the viewport, and
//! `draw.rs`'s cull skips any subtree whose solved rect misses the clip.
//! A culled child never paints, so its probe never fires.
//!
//! The exemption that would fix it already exists — `Element::
//! probe_when_culled`, which keeps a culled node's OWN paint alive and
//! still hands it the correct solved rect (added for first-app/0281's
//! measurement probe). It is `pub(crate)`.
//!
//! So these two tests are the same composition differing in one flag,
//! and they are each other's falsification: `a_culled_child_never_
//! reports_its_offset` is what an app can build, and
//! `probe_when_culled_reports_the_offset_of_an_off_screen_child` is
//! what the engine can. Whatever 0910 ships must keep the second true;
//! the first flips the day the seam becomes public, and that flip is
//! the point of the item.

use std::cell::Cell;
use std::rc::Rc;

use super::*;
use crate::base::{Rect, Size};
use crate::layout::Style as LayoutStyle;
use crate::theme::default_theme;
use crate::ui::{text, Element, MouseKind};
use crate::widgets::itest_util::{mount_widget, mouse, settle};

/// Records the solved rect a paint closure is handed, and how often.
type Probe = Rc<Cell<Option<Rect>>>;

/// The viewport is 4 rows; the probed child sits at row 12, far below
/// the fold. `culled_probe` picks which of the two spellings the probed
/// child uses.
const VIEW_H: i32 = 4;
const PROBED_ROW: i32 = 12;

fn column_with_a_probed_child(probe: Probe, culled_probe: bool) -> (View, i32) {
    let mut col = Element::new().style(LayoutStyle::column());
    for i in 0..20 {
        if i == PROBED_ROW {
            let p = probe.clone();
            let mut marked = Element::new()
                .style(LayoutStyle::column())
                .child(text(format!("row {i}")))
                .draw(move |_canvas, rect| p.set(Some(rect)));
            if culled_probe {
                marked = marked.probe_when_culled();
            }
            col = col.child(marked.build());
        } else {
            col = col.child(text(format!("row {i}")));
        }
    }
    (col.build(), 20)
}

/// What an app can build today, and why it is not enough: the probe on
/// an off-screen child never runs, so there is nothing to read the
/// child's offset out of. The control in the second half is what makes
/// this a finding rather than a broken probe.
#[test]
fn a_culled_child_never_reports_its_offset() {
    let t = &default_theme().tokens;
    let size = Size::new(12, VIEW_H);
    let probe: Probe = Rc::new(Cell::new(None));
    let (content, h) = column_with_a_probed_child(probe.clone(), false);
    let (_root, mut tree) = mount_widget(size, |cx| {
        Scroll::new(content)
            .content_size(10, h)
            .element(cx, t)
            .build()
    });
    settle(&mut tree, size);

    assert_eq!(
        probe.get(),
        None,
        "0910: a child below the fold is culled by draw.rs, so a paint \
         probe on it never fires — an app cannot learn where it landed, \
         which is exactly the child an ensure-visible verb must locate"
    );

    // CONTROL. The same probe, on the same child, once it is scrolled
    // into view: without this the assertion above would also pass on a
    // probe that never worked at all.
    mouse(&mut tree, MouseKind::ScrollDown, 2, 1);
    mouse(&mut tree, MouseKind::ScrollDown, 2, 1);
    mouse(&mut tree, MouseKind::ScrollDown, 2, 1);
    mouse(&mut tree, MouseKind::ScrollDown, 2, 1);
    settle(&mut tree, size);
    let seen = probe.get().expect(
        "control: the probe must fire once its child is scrolled into \
         view, or the None above proves nothing about culling",
    );
    assert!(
        seen.y >= 0 && seen.y < VIEW_H,
        "control: the probed child reports a rect inside the viewport \
         once visible, got {seen:?}"
    );
}

/// What the engine can already do and the app cannot reach: the cull
/// exemption keeps the probe alive on an off-screen child AND hands it
/// the true solved rect, which is the whole primitive 0910's read seam
/// would stand on.
#[test]
fn probe_when_culled_reports_the_offset_of_an_off_screen_child() {
    let t = &default_theme().tokens;
    let size = Size::new(12, VIEW_H);
    let probe: Probe = Rc::new(Cell::new(None));
    let (content, h) = column_with_a_probed_child(probe.clone(), true);
    let (_root, mut tree) = mount_widget(size, |cx| {
        Scroll::new(content)
            .content_size(10, h)
            .element(cx, t)
            .build()
    });
    settle(&mut tree, size);

    let seen = probe
        .get()
        .expect("probe_when_culled keeps a culled node's own paint alive");
    assert_eq!(
        seen.y, PROBED_ROW,
        "the culled probe reports the child's TRUE solved offset, not a \
         clamped or zeroed one — {seen:?}"
    );
    assert!(
        seen.y >= VIEW_H,
        "sanity: the probed child really is outside the {VIEW_H}-row \
         viewport, so this is the off-screen case and not a visible one"
    );
}
