//! Plain-ASCII layout dumps: a debugging aid, not a renderer.
//!
//! The view half of the graph story (cycle 2) draws real cards and
//! strokes; this module exists so a [`Layout`] is inspectable in test
//! output and bug reports today. Cards render as `#` outlines with the
//! node id inside, edge polylines as `*` (broken cycle edges as `%`).

use crate::layout::Layout;

/// Widest layout the dump will draw before honestly refusing.
const MAX_SIDE: i32 = 512;

/// Render a layout as ASCII art (one string, newline-separated rows).
///
/// Layouts larger than a debugging dump can usefully show are refused
/// with a one-line label instead of megabytes of whitespace.
pub fn ascii(layout: &Layout) -> String {
    let (w, h) = (layout.bounds.w, layout.bounds.h);
    if w <= 0 || h <= 0 {
        return String::from("(empty layout)");
    }
    if w > MAX_SIDE || h > MAX_SIDE {
        return format!("(layout {w}x{h} exceeds the {MAX_SIDE}-cell ascii dump cap)");
    }
    let (w, h) = (w as usize, h as usize);
    let mut canvas = vec![vec![b' '; w]; h];

    let mut plot = |x: i32, y: i32, ch: u8| {
        if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h {
            canvas[y as usize][x as usize] = ch;
        }
    };

    // Edges first, nodes on top.
    for edge in &layout.edges {
        let ch = if edge.broken { b'%' } else { b'*' };
        for pair in edge.waypoints.windows(2) {
            line(pair[0].x, pair[0].y, pair[1].x, pair[1].y, |x, y| {
                plot(x, y, ch)
            });
        }
    }
    for node in &layout.nodes {
        let r = node.rect;
        for x in r.x..r.right() {
            plot(x, r.y, b'#');
            plot(x, r.bottom() - 1, b'#');
        }
        for y in r.y..r.bottom() {
            plot(r.x, y, b'#');
            plot(r.right() - 1, y, b'#');
        }
        // Node id inside the card (row 1 if the card has an interior,
        // else the border row), truncated to the card width.
        let label_y = if r.h > 2 { r.y + 1 } else { r.y };
        let max_len = (r.w - 2).max(1) as usize;
        for (i, b) in node.id.bytes().take(max_len).enumerate() {
            plot(r.x + 1 + i as i32, label_y, b);
        }
    }

    let mut out = String::with_capacity(h * (w + 1));
    for row in canvas {
        // Safety of from_utf8: the canvas holds ASCII bytes only.
        out.push_str(std::str::from_utf8(&row).unwrap_or(""));
        out.push('\n');
    }
    out
}

/// Integer Bresenham over the segment, endpoints inclusive.
fn line(x0: i32, y0: i32, x1: i32, y1: i32, mut plot: impl FnMut(i32, i32)) {
    let (dx, dy) = ((x1 - x0).abs(), -(y1 - y0).abs());
    let (sx, sy) = (if x0 < x1 { 1 } else { -1 }, if y0 < y1 { 1 } else { -1 });
    let (mut x, mut y, mut err) = (x0, y0, dx + dy);
    loop {
        plot(x, y);
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}
