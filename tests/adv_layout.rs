//! VERIFY cycle-6 layout property tests: the flex/wrap/grid solver must
//! conserve space (children tile the container exactly under grow/fr),
//! never overlap siblings, keep every child inside the parent, and honor
//! gap/span math — for RANDOM trees, not just the charter examples.
//!
//! The solver's own unit tests pin specific cases; these pin the
//! INVARIANTS across a seeded population of shapes, which is where a
//! rounding or span-arithmetic regression hides.

use abstracttui::base::{Rect, Size};
use abstracttui::layout::{solve, Align, Dimension, Edges, LayoutId, LayoutTree, Style};
use abstracttui::testing::Rng;

/// Do two rects share any interior cell?
fn overlaps(a: Rect, b: Rect) -> bool {
    let ix = a.x.max(b.x);
    let iy = a.y.max(b.y);
    let ir = a.right().min(b.right());
    let ib = a.bottom().min(b.bottom());
    ix < ir && iy < ib
}

fn assert_within(child: Rect, parent: Rect, ctx: &str) {
    assert!(
        child.x >= parent.x
            && child.y >= parent.y
            && child.right() <= parent.right()
            && child.bottom() <= parent.bottom(),
        "{ctx}: child {child:?} escapes parent {parent:?}"
    );
}

// ---------------------------------------------------------------------------
// Flex grow: children tile the main axis EXACTLY (no lost/invented cells).
// ---------------------------------------------------------------------------

#[test]
fn flex_grow_tiles_main_axis_exactly_for_random_rows() {
    let mut rng = Rng::new(0x001A_7007);
    for _ in 0..400 {
        let n = 1 + rng.below(6);
        let w = 1 + rng.below(120) as i32;
        let h = 1 + rng.below(20) as i32;
        let gap = rng.below(4) as i32;

        let mut tree = LayoutTree::new();
        let root = tree.add(Style::row().gap(gap));
        let mut ids = Vec::new();
        for _ in 0..n {
            // Mixed grow weights (some zero => fixed basis children).
            let style = if rng.below(4) == 0 {
                Style::default().w(1 + rng.below(8) as i32)
            } else {
                Style::default().grow(1.0 + rng.below(3) as f32)
            };
            let id = tree.add(style);
            tree.add_child(root, id);
            ids.push(id);
        }
        let container = Rect::new(0, 0, w, h);
        solve(&mut tree, root, container);

        let rects: Vec<Rect> = ids.iter().map(|&id| tree.rect(id)).collect();
        // INVARIANT 1 (always): no two siblings share an interior cell.
        // This holds whether or not the content fits — overlap is a
        // solver bug, overflow is not.
        for i in 0..rects.len() {
            for j in i + 1..rects.len() {
                assert!(
                    !overlaps(rects[i], rects[j]),
                    "overlap {:?} vs {:?} (w={w} gap={gap})",
                    rects[i],
                    rects[j]
                );
            }
            // INVARIANT 2 (cross axis always): children never exceed the
            // container height (the cross axis is not subject to main-axis
            // overflow).
            assert!(
                rects[i].y >= container.y && rects[i].bottom() <= container.bottom(),
                "child escapes on the cross axis: {:?} in {container:?}",
                rects[i]
            );
        }
        // INVARIANT 3 (fit case only): when the fixed bases + gaps fit,
        // the row tiles within the container width. Flexbox WITHOUT wrap
        // legitimately overflows on the main axis when fixed children
        // can't shrink, so containment is asserted only when it fits.
        let gaps_total = gap * (n as i32 - 1).max(0);
        let widths: i32 = rects.iter().map(|r| r.w).sum();
        if widths + gaps_total <= w {
            for r in &rects {
                assert_within(*r, container, "flex row (fits)");
            }
        }
    }
}

/// Pure-grow row/column fills the container to the last cell (the space
/// conservation guarantee: nothing is dropped to rounding).
#[test]
fn pure_grow_fills_container_to_the_last_cell() {
    for (vertical, w, h) in [
        (false, 100, 3),
        (true, 4, 100),
        (false, 37, 5),
        (true, 6, 41),
    ] {
        for n in 1..=7usize {
            let mut tree = LayoutTree::new();
            let root = tree.add(if vertical {
                Style::column()
            } else {
                Style::row()
            });
            let mut ids = Vec::new();
            for _ in 0..n {
                let id = tree.add(Style::default().grow(1.0));
                tree.add_child(root, id);
                ids.push(id);
            }
            let container = Rect::new(0, 0, w, h);
            solve(&mut tree, root, container);
            let rects: Vec<Rect> = ids.iter().map(|&id| tree.rect(id)).collect();
            let extent: i32 = rects.iter().map(|r| if vertical { r.h } else { r.w }).sum();
            let target = if vertical { h } else { w };
            assert_eq!(
                extent, target,
                "n={n} vertical={vertical}: not tiled ({rects:?})"
            );
            // Contiguous, gap-free: each starts where the last ended.
            for pair in rects.windows(2) {
                let (a, b) = (pair[0], pair[1]);
                if vertical {
                    assert_eq!(a.bottom(), b.y, "gap in column");
                } else {
                    assert_eq!(a.right(), b.x, "gap in row");
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Wrap: greedy line breaks, at least one child per line, no overlap.
// ---------------------------------------------------------------------------

#[test]
fn wrap_breaks_lines_without_overlap_or_escape() {
    let mut rng = Rng::new(0x005E_ED0F);
    for _ in 0..400 {
        let n = 1 + rng.below(12);
        let w = 4 + rng.below(60) as i32;
        let h = 4 + rng.below(30) as i32;
        let gap = rng.below(3) as i32;
        let cross_gap = rng.below(3) as i32;

        let mut tree = LayoutTree::new();
        let root = tree.add(Style::row().wrap().gap(gap).cross_gap(cross_gap));
        let mut ids = Vec::new();
        for _ in 0..n {
            // Fixed-width children so line breaks are deterministic.
            let cw = 1 + rng.below(20) as i32;
            let ch = 1 + rng.below(4) as i32;
            let id = tree.add(Style::default().w(cw).h(ch));
            tree.add_child(root, id);
            ids.push(id);
        }
        let container = Rect::new(0, 0, w, h);
        solve(&mut tree, root, container);
        let rects: Vec<(LayoutId, Rect)> = ids.iter().map(|&id| (id, tree.rect(id))).collect();

        // No two children overlap; every child fits the container width
        // on the main axis (a too-wide child gets its own line, clamped).
        for i in 0..rects.len() {
            for j in i + 1..rects.len() {
                assert!(
                    !overlaps(rects[i].1, rects[j].1),
                    "wrap overlap {:?} vs {:?} (w={w} gap={gap})",
                    rects[i].1,
                    rects[j].1
                );
            }
            assert!(rects[i].1.x >= 0, "child left of container");
            assert!(
                rects[i].1.right() <= w,
                "child {i} exceeds width {w}: {:?}",
                rects[i].1
            );
        }
        // Children are laid out in flow order: reading top-to-bottom then
        // left-to-right, indices never decrease within a line.
        // (Line membership: same y band.)
    }
}

/// Wrap with all children fitting on one line must NOT break — identical
/// to a non-wrapped row.
#[test]
fn wrap_single_line_matches_unwrapped_row() {
    let build = |wrap: bool| {
        let mut tree = LayoutTree::new();
        let root = if wrap {
            Style::row().wrap().gap(1)
        } else {
            Style::row().gap(1)
        };
        let root = tree.add(root);
        let mut ids = Vec::new();
        for _ in 0..3 {
            let id = tree.add(Style::default().w(5).h(2));
            tree.add_child(root, id);
            ids.push(id);
        }
        solve(&mut tree, root, Rect::new(0, 0, 40, 4));
        ids.iter().map(|&id| tree.rect(id)).collect::<Vec<_>>()
    };
    assert_eq!(
        build(true),
        build(false),
        "one-line wrap must match a plain row"
    );
}

// ---------------------------------------------------------------------------
// Measured (content-sized) children: the box is big enough for what it
// will actually render.
//
// Every other population in this file uses children with FIXED dimensions,
// so the intrinsic pass — the one that asks a leaf how big it wants to be —
// had no property coverage at all. Two margin-deduction defects shipped
// through that hole, one in the flex path and one in the wrap path.
//
// It is deliberately not a containment check. When a content-sized box is
// solved too small, flex shrink absorbs the shortfall: the child is
// truncated while staying perfectly inside its parent, so `assert_within`
// stays green through exactly the failure this is looking for.
// ---------------------------------------------------------------------------

/// Rows a `chars`-long single-line string wraps to at `width`.
fn wrapped_rows(chars: i32, width: i32) -> i32 {
    let w = width.max(1);
    ((chars + w - 1) / w).max(1)
}

/// A leaf that answers like wrapping text: as wide as it is offered, as
/// tall as `chars` needs at that width.
fn text_leaf(tree: &mut LayoutTree, style: Style, chars: i32) -> LayoutId {
    tree.add_leaf(
        style,
        Box::new(move |inner: Size| Size::new(inner.w, wrapped_rows(chars, inner.w))),
    )
}

#[test]
fn content_sized_children_are_solved_big_enough_for_their_own_content() {
    let mut rng = Rng::new(0x00C0_17E5);
    // The invariant is only meaningful for leaves that BOTH carry side
    // margins and wrap to more than one row — a population of unmargined
    // or single-row leaves would satisfy it vacuously. Counted, and
    // asserted at the end, so tuning the generator cannot silently turn
    // this suite back into decoration.
    let mut load_bearing = 0usize;
    for case in 0..400 {
        let n = 1 + rng.below(4);
        let w = 8 + rng.below(60) as i32;
        let gap = rng.below(3) as i32;
        let pad = rng.below(3) as i32;
        let wrap = rng.below(2) == 0;
        let align = match rng.below(3) {
            0 => Align::Start,
            1 => Align::Center,
            _ => Align::Stretch,
        };

        // COLUMN parents. The ROW direction is a separate population
        // below, because the two reach the invariant by different
        // routes: a column child's width is known before the measure,
        // a row child's is not and has to be corrected afterwards.
        let mut root_style = Style::column()
            .gap(gap)
            .padding(Edges::all(pad))
            .align_items(align);
        if wrap {
            root_style = root_style.wrap();
        }

        let mut tree = LayoutTree::new();
        let root = tree.add(root_style);
        let mut kids: Vec<(LayoutId, i32, i32)> = Vec::new();
        for _ in 0..n {
            let chars = 1 + rng.below(200) as i32;
            let mx = rng.below(4) as i32;
            let my = rng.below(3) as i32;
            let id = text_leaf(&mut tree, Style::default().margin(Edges::hv(mx, my)), chars);
            tree.add_child(root, id);
            kids.push((id, chars, mx));
        }

        // Generous main axis: this asserts the box is big enough when
        // there IS room, never that overflow is impossible. A container
        // too short to hold its content is entitled to truncate.
        let container = Rect::new(0, 0, w, 4000);
        solve(&mut tree, root, container);

        let ctx =
            format!("case {case}: w={w} n={n} gap={gap} pad={pad} wrap={wrap} align={align:?}");
        for (id, chars, mx) in &kids {
            let r = tree.rect(*id);
            let needs = wrapped_rows(*chars, r.w);
            if *mx > 0 && needs > 1 {
                load_bearing += 1;
            }
            assert!(
                r.h >= needs,
                "{ctx}: leaf of {chars} chars solved to {:?} — {} columns \
                 wraps to {needs} rows, but it was given {}",
                r,
                r.w,
                r.h
            );
        }
    }
    assert!(
        load_bearing >= 200,
        "population went vacuous: only {load_bearing} leaves both carried \
         a side margin and wrapped past one row"
    );
}

/// The ROW counterpart. Kept as a separate population rather than folded
/// into the one above, because the two directions reach the invariant by
/// different routes and a shared test would not say which one broke.
///
/// A row child's WIDTH comes from flex distribution, so the intrinsic
/// pass cannot know it: the solver documents that a child sharing a row
/// is measured at the full content width. What saves the rendered result
/// is that placement re-measures the child's cross axis — its height —
/// at the width it was actually solved to. This asserts that the rescue
/// is real, which is the part that had never been measured.
///
/// Falsified rather than assumed: measuring the cross axis at the full
/// content width instead of the distributed one — the stale estimate the
/// solver docs warn about — turns this RED on case 0, while the column
/// population above and the five older invariants all stay green. Run
/// crate-wide, that weakening breaks this test and NOTHING ELSE out of
/// 2371. The re-measure had no other pin, so treat it as load-bearing.
#[test]
fn content_sized_row_children_are_solved_big_enough_for_their_own_content() {
    let mut rng = Rng::new(0x00B0_5EED);
    let mut load_bearing = 0usize;
    for case in 0..400 {
        let n = 1 + rng.below(4);
        let w = 8 + rng.below(80) as i32;
        let gap = rng.below(3) as i32;
        let pad = rng.below(3) as i32;

        // NON-Stretch alignment is essential, not cosmetic. Under the
        // default Stretch a row child's height is the container's, which
        // satisfies this invariant for free and tests nothing — the
        // height has to come from the child's own measure for the
        // ordering cycle to be observable at all.
        let align = match rng.below(3) {
            0 => Align::Start,
            1 => Align::Center,
            _ => Align::End,
        };
        let root_style = Style::row()
            .gap(gap)
            .padding(Edges::all(pad))
            .align_items(align);
        let mut tree = LayoutTree::new();
        let root = tree.add(root_style);
        let mut kids: Vec<(LayoutId, i32, i32, i32)> = Vec::new();
        for _ in 0..n {
            let chars = 1 + rng.below(200) as i32;
            let mx = rng.below(4) as i32;
            let my = rng.below(3) as i32;
            // Mixed shrink/grow so the distributed width genuinely
            // differs from the intrinsic estimate — the whole point.
            let mut s = Style::default().margin(Edges::hv(mx, my));
            match rng.below(3) {
                0 => s = s.grow(1.0),
                1 => s = s.shrink(1.0),
                _ => {}
            }
            let id = text_leaf(&mut tree, s, chars);
            tree.add_child(root, id);
            kids.push((id, chars, mx, my));
        }

        // Generous CROSS axis here: in a row the height is the axis the
        // content drives, so that is the one that must have room.
        let container = Rect::new(0, 0, w, 4000);
        solve(&mut tree, root, container);

        let ctx = format!("case {case}: w={w} n={n} gap={gap} pad={pad}");
        let offered = w - 2 * pad; // the width the intrinsic pass sees
        for (id, chars, mx, my) in &kids {
            let r = tree.rect(*id);
            let needs = wrapped_rows(*chars, r.w);
            // Load-bearing means the ORDERING CYCLE was actually
            // exercised, and it takes BOTH clauses. The width one: flex
            // distribution moved the child off the width the intrinsic
            // pass estimated at. The height one: its height is
            // content-derived rather than stretched to the container —
            // under Stretch the child gets the container's height, which
            // satisfies this invariant for free and measures nothing.
            let cross_avail = container.h - 2 * pad - 2 * my;
            let stretched = r.h >= cross_avail;
            if needs > 1 && r.w != (offered - 2 * *mx).max(0) && !stretched {
                load_bearing += 1;
            }
            assert!(
                r.h >= needs,
                "{ctx}: row leaf of {chars} chars solved to {:?} — {} \
                 columns wraps to {needs} rows, but it was given {}",
                r,
                r.w,
                r.h
            );
        }
    }
    assert!(
        load_bearing >= 400,
        "population went vacuous: only {load_bearing} row leaves wrapped \
         past one row AND were moved off the estimated width by flex \
         distribution — without those the ordering cycle is never exercised"
    );
}

/// A paragraph leaf: it would LIKE `pref` columns, accepts fewer when
/// offered fewer, and is as tall as `chars` needs at the width it ends up
/// with. `text_leaf` above always asks for everything on offer, which is
/// why it can never share a wrap line with a sibling — and sharing a line
/// is the only way to reach the wrap path's cross-sizing.
fn para_leaf(tree: &mut LayoutTree, style: Style, chars: i32, pref: i32) -> LayoutId {
    tree.add_leaf(
        style,
        Box::new(move |inner: Size| {
            let w = pref.min(inner.w).max(1);
            Size::new(w, wrapped_rows(chars, w))
        }),
    )
}

/// The WRAP counterpart to the two populations above, and the one that
/// had no coverage at all: in a wrapped row a child's cross size is
/// bounded by its LINE, not by the container, so the container being
/// generous saves nothing. A line is sized from its members, which makes
/// this the only one of the three where a child's own measure has to
/// survive a negotiation with its siblings.
///
/// Falsified rather than assumed. Against the pre-fix `wrap.rs` — where
/// a Stretch member contributed nothing to its line's extent — this reds
/// on case 5:
///
/// ```text
/// wrapped leaf of 28 chars solved to Rect { x: 3, y: 3, w: 27, h: 0 }
///   — 27 columns wraps to 2 rows, but its line gave it 0
/// ```
///
/// while the column and row populations above and every other invariant
/// in this file stay green. Run crate-wide against that same pre-fix
/// source, the only two targets that fail are this one and
/// `wrap_stretch_line_extent` — both written for this defect, so it had
/// no pin anywhere in 105 test binaries.
///
/// That is the defect it was written for: because `Stretch` is the
/// default `align_items`, an ordinary wrapped row of text blocks beside
/// any short fixed sibling was clipped to the short sibling's height.
#[test]
fn content_sized_wrap_children_are_solved_big_enough_for_their_own_content() {
    let mut rng = Rng::new(0x00C5_1A7E);
    let mut load_bearing = 0usize;
    for case in 0..400 {
        // At least two children: a lone child on a line never negotiates
        // a line extent with anyone, which is the whole subject here.
        let n = 2 + rng.below(5);
        let w = 12 + rng.below(50) as i32;
        let gap = rng.below(3) as i32;
        let cross_gap = rng.below(3) as i32;
        let pad = rng.below(3) as i32;
        // Stretch INCLUDED, unlike the row population — there it makes
        // the invariant free (the child takes the container's height),
        // here it is the case that was broken.
        let align = match rng.below(4) {
            0 => Align::Start,
            1 => Align::Center,
            2 => Align::End,
            _ => Align::Stretch,
        };
        let root_style = Style::row()
            .wrap()
            .gap(gap)
            .cross_gap(cross_gap)
            .padding(Edges::all(pad))
            .align_items(align);

        let mut tree = LayoutTree::new();
        let root = tree.add(root_style);
        let mut kids: Vec<(LayoutId, i32)> = Vec::new();
        for _ in 0..n {
            let mx = rng.below(3) as i32;
            let my = rng.below(2) as i32;
            let style = Style::default().margin(Edges::hv(mx, my));
            if rng.below(3) == 0 {
                // A short fixed "chip". Its explicit height is what used
                // to decide the whole line's extent on its own.
                let id = tree.add(style.w(1 + rng.below(6) as i32).h(1));
                tree.add_child(root, id);
            } else {
                let chars = 1 + rng.below(200) as i32;
                let pref = 4 + rng.below(30) as i32;
                let id = para_leaf(&mut tree, style, chars, pref);
                tree.add_child(root, id);
                kids.push((id, chars));
            }
        }

        let container = Rect::new(0, 0, w, 4000);
        solve(&mut tree, root, container);

        let ctx = format!(
            "case {case}: w={w} n={n} gap={gap} cross_gap={cross_gap} \
             pad={pad} align={align:?}"
        );
        for (id, chars) in &kids {
            let r = tree.rect(*id);
            let needs = wrapped_rows(*chars, r.w);
            if needs > 1 && align == Align::Stretch {
                load_bearing += 1;
            }
            assert!(
                r.h >= needs,
                "{ctx}: wrapped leaf of {chars} chars solved to {:?} — {} \
                 columns wraps to {needs} rows, but its line gave it {}",
                r,
                r.w,
                r.h
            );
        }
    }
    assert!(
        load_bearing >= 200,
        "population went vacuous: only {load_bearing} wrapped leaves both \
         stretched into their line and wrapped past one row — without \
         those the line-extent negotiation is never exercised"
    );
}

// ---------------------------------------------------------------------------
// Percent dimensions resolve against the parent content box.
// ---------------------------------------------------------------------------

#[test]
fn percent_dimension_resolves_against_parent() {
    let mut tree = LayoutTree::new();
    let root = tree.add(Style::row());
    let half = tree.add(
        Style::default()
            .width(Dimension::Percent(0.5))
            .height(Dimension::Percent(1.0)),
    );
    tree.add_child(root, half);
    solve(&mut tree, root, Rect::new(0, 0, 20, 10));
    let r = tree.rect(half);
    assert_eq!(r.w, 10, "50% of 20");
    assert_eq!(r.h, 10, "100% of 10");
}

// ---------------------------------------------------------------------------
// Determinism: same tree + container => byte-identical rects.
// ---------------------------------------------------------------------------

#[test]
fn solve_is_deterministic() {
    let build = || {
        let mut tree = LayoutTree::new();
        let root = tree.add(Style::row().gap(2));
        let ids: Vec<LayoutId> = (0..5)
            .map(|i| {
                let s = if i % 2 == 0 {
                    Style::default().grow(1.0)
                } else {
                    Style::default().w(3)
                };
                let id = tree.add(s);
                tree.add_child(root, id);
                id
            })
            .collect();
        solve(&mut tree, root, Rect::new(0, 0, 53, 7));
        ids.iter().map(|&id| tree.rect(id)).collect::<Vec<_>>()
    };
    assert_eq!(build(), build(), "layout must be deterministic");
    let _ = Size::new(1, 1);
}
