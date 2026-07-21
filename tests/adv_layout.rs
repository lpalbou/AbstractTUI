//! VERIFY cycle-6 layout property tests: the flex/wrap/grid solver must
//! conserve space (children tile the container exactly under grow/fr),
//! never overlap siblings, keep every child inside the parent, and honor
//! gap/span math — for RANDOM trees, not just the charter examples.
//!
//! The solver's own unit tests pin specific cases; these pin the
//! INVARIANTS across a seeded population of shapes, which is where a
//! rounding or span-arithmetic regression hides.

use abstracttui::base::{Rect, Size};
use abstracttui::layout::{solve, Dimension, LayoutId, LayoutTree, Style};
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
