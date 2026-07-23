//! Edge stroke planning + painting for [`GraphView`](crate::view::GraphView):
//! layout waypoints become `abstracttui::canvas` strokes (smoothed
//! beziers through interior waypoints, canonical-frame bowing for
//! parallel 2-point edges), arrowhead glyphs at the target border,
//! dotted/thick styles from `EdgeDesc::style`, and the broken-edge
//! honesty marker (cycle-reversed edges render dotted in their own
//! ink — visibly distinct, never silently normal).
//!
//! Planning is a BUILD-time act over the immutable `Layout` (pure
//! data); the draw closure only strokes the plan. Determinism: same
//! desc + layout, same plan, same dots.
//!
//! OWNER: CANVAS (view half of 0440).

use std::collections::HashMap;

use abstracttui::base::{Point, Rect, Rgba};
use abstracttui::canvas::DotCanvas;
use abstracttui::text::truncate_ellipsis;

use crate::desc::GraphDesc;
use crate::layout::Layout;

/// Stroke rendering class, derived from `EdgeDesc::style` (an opaque
/// hint; this view's vocabulary: a hint containing "dotted"/"dashed"
/// draws dotted, "thick"/"bold" draws thick, "open" suppresses the
/// arrowhead — mermaid's `---` link — and combines with the stroke
/// classes; anything else is solid). Cycle-broken edges are FORCED
/// dotted (the honesty marker).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum StrokeKind {
    Solid,
    Dotted,
    Thick,
}

/// One edge, planned for painting: cell-space geometry + classes.
pub(crate) struct PlannedEdge {
    /// Waypoints in content cells (the layout's, verbatim).
    pub points: Vec<Point>,
    /// Perpendicular bow in cells for 2-point edges sharing an
    /// unordered node pair (0 = straight). The bow axis is the
    /// CANONICAL pair frame (low index -> high index), so an a->b and
    /// a b->a bow to OPPOSITE sides — direction-aware legibility for
    /// force/grid parallels (the layered pass spreads anchors
    /// upstream; bowing composes with it).
    pub bow: f64,
    pub kind: StrokeKind,
    /// Honesty class: broken edges stroke in their own ink.
    pub broken: bool,
    /// Arrowhead cell + glyph at the target border. `None` when no
    /// direction is derivable (fully degenerate geometry).
    pub arrow: Option<(Point, char)>,
    /// Midpoint label (truncated at plan time).
    pub label: Option<(Point, String)>,
}

/// Max label width an edge may claim (cells).
const LABEL_BUDGET: i32 = 16;

/// Plan every layout edge for painting. Metadata (style/label) maps
/// through `EdgeLayout::desc_index` — NEVER positionally — so skipped
/// unresolvable edges cannot shift styles onto the wrong edge (the
/// cycle-1 attack item: desc_index is the only lawful join).
pub(crate) fn plan_edges(desc: &GraphDesc, layout: &Layout) -> Vec<PlannedEdge> {
    // Node id -> rect (arrow fallback geometry).
    let rect_of: HashMap<&str, Rect> = layout
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.rect))
        .collect();

    // Canonical-frame bow ordinals for 2-point edges sharing an
    // unordered endpoint pair. Maps are lookup-only (determinism:
    // iteration stays in layout-edge order).
    let mut pair_total: HashMap<(String, String), usize> = HashMap::new();
    for e in &layout.edges {
        if e.waypoints.len() == 2 && e.from != e.to {
            *pair_total.entry(pair_key(&e.from, &e.to)).or_insert(0) += 1;
        }
    }
    let mut pair_seen: HashMap<(String, String), usize> = HashMap::new();

    let mut planned = Vec::with_capacity(layout.edges.len());
    for e in &layout.edges {
        if e.waypoints.is_empty() {
            continue;
        }
        let meta = desc.edges.get(e.desc_index);
        let hint = meta.and_then(|m| m.style.as_deref()).unwrap_or("");
        let kind = if e.broken || hint.contains("dotted") || hint.contains("dashed") {
            StrokeKind::Dotted
        } else if hint.contains("thick") || hint.contains("bold") {
            StrokeKind::Thick
        } else {
            StrokeKind::Solid
        };
        // The arrowless vocabulary (mermaid `---`, cycle-3): an
        // "open" hint keeps the stroke and drops the head.
        let open = hint.contains("open");

        let bow = if e.waypoints.len() == 2 && e.from != e.to {
            let key = pair_key(&e.from, &e.to);
            let total = pair_total[&key];
            if total > 1 {
                let slot = pair_seen.entry(key).or_insert(0);
                let k = *slot;
                *slot += 1;
                // Centered ordinals, 2 cells apart, in the canonical
                // frame: edges running WITH the frame bow one way,
                // AGAINST it the other.
                let centered = 2.0 * (k as f64) - (total as f64 - 1.0);
                if canonical_forward(&e.from, &e.to) {
                    centered
                } else {
                    -centered
                }
            } else {
                0.0
            }
        } else {
            0.0
        };

        let arrow = if open {
            None
        } else {
            plan_arrow(&e.waypoints, rect_of.get(e.to.as_str()).copied())
        };
        let label = meta.and_then(|m| m.label.as_deref()).and_then(|text| {
            let mid = polyline_mid(&e.waypoints);
            let t = truncate_ellipsis(text, LABEL_BUDGET);
            if t.is_empty() {
                return None;
            }
            // Never overprint a card (cycle-3 attack item): the label
            // run [mid.x+1, mid.x+width] on row mid.y must not cross
            // any node rect — a label over a title is illegible both
            // ways. Strokes are fine to cross (labels paint last).
            let w = abstracttui::text::width(&t);
            let run = Rect::new(mid.x + 1, mid.y, w, 1);
            let clear = layout
                .nodes
                .iter()
                .all(|n| n.rect.intersect(run).is_empty());
            clear.then_some((mid, t))
        });

        planned.push(PlannedEdge {
            points: e.waypoints.clone(),
            bow,
            kind,
            broken: e.broken,
            arrow,
            label,
        });
    }
    planned
}

/// The label anchor: the middle waypoint for odd counts, the cell
/// midpoint of the two central waypoints otherwise — for a straight
/// 2-point edge that is the GEOMETRIC middle of the chord (the
/// cycle-2 `waypoints[len/2]` picked the TARGET anchor and printed
/// labels into the target card; caught by the cycle-3 attack tests).
fn polyline_mid(points: &[Point]) -> Point {
    let n = points.len();
    if n.is_multiple_of(2) {
        let (a, b) = (points[n / 2 - 1], points[n / 2]);
        Point::new((a.x + b.x) / 2, (a.y + b.y) / 2)
    } else {
        points[n / 2]
    }
}

fn pair_key(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_string(), b.to_string())
    } else {
        (b.to_string(), a.to_string())
    }
}

fn canonical_forward(from: &str, to: &str) -> bool {
    from <= to
}

/// Arrow glyph + cell from the last non-degenerate segment direction;
/// falls back to (target rect center - last waypoint) when every
/// waypoint coincides (the rank_gap=1 corridor case, where source and
/// target anchors legitimately share the single corridor cell).
fn plan_arrow(points: &[Point], to_rect: Option<Rect>) -> Option<(Point, char)> {
    let last = *points.last()?;
    let mut dir: Option<(i32, i32)> = None;
    for p in points.iter().rev().skip(1) {
        let d = (last.x - p.x, last.y - p.y);
        if d != (0, 0) {
            dir = Some(d);
            break;
        }
    }
    if dir.is_none() {
        if let Some(r) = to_rect {
            let c = (r.x + r.w / 2, r.y + r.h / 2);
            let d = (c.0 - last.x, c.1 - last.y);
            if d != (0, 0) {
                dir = Some(d);
            }
        }
    }
    let (dx, dy) = dir?;
    let glyph = if dx.abs() >= dy.abs() {
        if dx > 0 {
            '▶'
        } else {
            '◀'
        }
    } else if dy > 0 {
        '▼'
    } else {
        '▲'
    };
    Some((last, glyph))
}

/// Cell -> dot-space center (braille: 2x4 dots per cell).
fn dot(p: Point) -> (f32, f32) {
    (2.0 * p.x as f32 + 1.0, 4.0 * p.y as f32 + 2.0)
}

/// Paint the plan into `canvas` at `origin` (the edge layer's solved
/// rect). One dot grid per ink class (the 0420 cell-color rule):
/// normal strokes first, broken strokes blitted after so the honesty
/// ink wins shared cells. Arrowheads and labels are cell glyphs drawn
/// last (they win their cells over dots).
pub(crate) fn draw_edges<C: abstracttui::ui::StyledCanvas + ?Sized>(
    canvas: &mut C,
    origin: Point,
    size: (i32, i32),
    plan: &[PlannedEdge],
    edge_ink: Rgba,
    broken_ink: Rgba,
    label_ink: Rgba,
) {
    let mut normal = DotCanvas::braille(size.0, size.1);
    let mut broken = DotCanvas::braille(size.0, size.1);
    for e in plan {
        let grid = if e.broken { &mut broken } else { &mut normal };
        stroke(grid, e);
    }
    normal.blit(canvas, origin, edge_ink);
    broken.blit(canvas, origin, broken_ink);

    for e in plan {
        if let Some((cell, glyph)) = e.arrow {
            let ink = if e.broken { broken_ink } else { edge_ink };
            canvas.put(
                Point::new(origin.x + cell.x, origin.y + cell.y),
                glyph,
                ink,
                Rgba::TRANSPARENT,
            );
        }
        if let Some((cell, text)) = &e.label {
            canvas.print(
                Point::new(origin.x + cell.x + 1, origin.y + cell.y),
                text,
                label_ink,
                Rgba::TRANSPARENT,
            );
        }
    }
}

/// Stroke one edge into its dot grid.
fn stroke(grid: &mut DotCanvas, e: &PlannedEdge) {
    match e.kind {
        StrokeKind::Solid => stroke_path(grid, e, (0, 0)),
        StrokeKind::Thick => {
            // Three offset passes read as a heavy stroke at dot scale.
            for off in [(0, 0), (1, 0), (0, 1)] {
                stroke_path(grid, e, off);
            }
        }
        StrokeKind::Dotted => stroke_dotted(grid, e),
    }
}

/// Solid path: a bowed quadratic for 2-point edges with a bow,
/// midpoint-smoothed quadratics through interior waypoints, straight
/// Bresenham otherwise.
fn stroke_path(grid: &mut DotCanvas, e: &PlannedEdge, off: (i32, i32)) {
    let pts = &e.points;
    let d = |p: Point| {
        let (x, y) = dot(p);
        (x + off.0 as f32, y + off.1 as f32)
    };
    let di = |p: Point| {
        let (x, y) = d(p);
        (x.round() as i32, y.round() as i32)
    };
    match pts.len() {
        0 => {}
        1 => {
            let p = di(pts[0]);
            grid.set(p.0, p.1);
        }
        2 => {
            if e.bow != 0.0 {
                let (p0, p1) = (d(pts[0]), d(pts[1]));
                let c = clamp_control(grid, bow_control(p0, p1, e.bow));
                grid.bezier_quad(p0, c, p1, 0.25);
            } else {
                grid.line(di(pts[0]), di(pts[1]));
            }
        }
        _ => {
            // Midpoint smoothing: quad through each interior waypoint,
            // straight tails. Degenerate (collinear) inputs collapse
            // to the straight polyline naturally.
            let mut cur = d(pts[0]);
            for i in 1..pts.len() - 1 {
                let ctrl = d(pts[i]);
                let next = d(pts[i + 1]);
                let m = (0.5 * (ctrl.0 + next.0), 0.5 * (ctrl.1 + next.1));
                grid.bezier_quad(cur, ctrl, m, 0.25);
                cur = m;
            }
            let last = di(*pts.last().expect("len >= 3"));
            grid.line((cur.0.round() as i32, cur.1.round() as i32), last);
        }
    }
}

/// Clamp a bow control point into the grid's dot box. The quad lies
/// in the convex hull of {p0, c, p1}: with in-grid anchors, a clamped
/// control keeps the WHOLE curve visible — outer parallel bows on
/// short chords used to arc off-grid and render as clipped stubs
/// (cycle-3 attack item; deterministic either way, now also legible).
fn clamp_control(grid: &DotCanvas, c: (f32, f32)) -> (f32, f32) {
    (
        c.0.clamp(0.0, (grid.dots_w() - 1).max(0) as f32),
        c.1.clamp(0.0, (grid.dots_h() - 1).max(0) as f32),
    )
}

/// Control point for a bowed pair: the chord midpoint displaced
/// perpendicular to the CANONICAL frame by `bow` cells (dot-space
/// aspect corrected: 2 dots/cell in x, 4 in y). The apex of the quad
/// sits at half the control displacement, so double the offset.
fn bow_control(p0: (f32, f32), p1: (f32, f32), bow: f64) -> (f32, f32) {
    let (mx, my) = (0.5 * (p0.0 + p1.0), 0.5 * (p0.1 + p1.1));
    let (ex, ey) = (p1.0 - p0.0, p1.1 - p0.1);
    let len = (ex * ex + ey * ey).sqrt();
    if len <= f32::EPSILON {
        return (mx, my);
    }
    // Perpendicular in dot space; scale to ~bow CELLS of apex (cell =
    // ~3 dots on the diagonal-ish average; exactness is not the point,
    // legible separation is).
    let (px, py) = (-ey / len, ex / len);
    let amp = (bow as f32) * 2.0 * 3.0;
    (mx + px * amp, my + py * amp)
}

/// Dotted: sample the same geometry at every third dot of arc length,
/// lighting single dots. The phase carries across segments so the
/// pattern reads continuous.
fn stroke_dotted(grid: &mut DotCanvas, e: &PlannedEdge) {
    let pts = &e.points;
    if pts.is_empty() {
        return;
    }
    let mut phase = 0u32;
    if pts.len() == 2 && e.bow != 0.0 {
        let (p0, p1) = (dot(pts[0]), dot(pts[1]));
        let c = clamp_control(grid, bow_control(p0, p1, e.bow));
        sample_quad(grid, p0, c, p1, &mut phase);
        return;
    }
    if pts.len() >= 3 {
        let mut cur = dot(pts[0]);
        for i in 1..pts.len() - 1 {
            let ctrl = dot(pts[i]);
            let next = dot(pts[i + 1]);
            let m = (0.5 * (ctrl.0 + next.0), 0.5 * (ctrl.1 + next.1));
            sample_quad(grid, cur, ctrl, m, &mut phase);
            cur = m;
        }
        sample_segment(grid, cur, dot(pts[pts.len() - 1]), &mut phase);
        return;
    }
    sample_segment(
        grid,
        dot(pts[0]),
        dot(*pts.last().expect("non-empty")),
        &mut phase,
    );
}

/// Every third dot along a straight segment.
fn sample_segment(grid: &mut DotCanvas, a: (f32, f32), b: (f32, f32), phase: &mut u32) {
    let (ex, ey) = (b.0 - a.0, b.1 - a.1);
    let len = (ex * ex + ey * ey).sqrt();
    let steps = len.ceil().max(1.0) as i32;
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        if phase.is_multiple_of(3) {
            let (x, y) = (a.0 + ex * t, a.1 + ey * t);
            grid.set(x.round() as i32, y.round() as i32);
        }
        *phase += 1;
    }
}

/// Every third dot along a quadratic (uniform parameter steps sized by
/// the control polygon — bounded, deterministic).
fn sample_quad(
    grid: &mut DotCanvas,
    p0: (f32, f32),
    c: (f32, f32),
    p1: (f32, f32),
    phase: &mut u32,
) {
    let poly = dist(p0, c) + dist(c, p1);
    let steps = poly.ceil().clamp(1.0, 512.0) as i32;
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let u = 1.0 - t;
        if phase.is_multiple_of(3) {
            let x = u * u * p0.0 + 2.0 * u * t * c.0 + t * t * p1.0;
            let y = u * u * p0.1 + 2.0 * u * t * c.1 + t * t * p1.1;
            grid.set(x.round() as i32, y.round() as i32);
        }
        *phase += 1;
    }
}

fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    let (ex, ey) = (b.0 - a.0, b.1 - a.1);
    (ex * ex + ey * ey).sqrt()
}
