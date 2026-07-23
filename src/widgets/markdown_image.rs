//! In-flow markdown images (0144): `![alt](path)` typeset as MOSAIC
//! rows inside the document flow.
//!
//! ## The laziness contract
//!
//! Typeset time reads ONLY the container header (`gfx::probe_dimensions`
//! over an incrementally-grown file prefix) to size the block — a
//! 100-image document decodes nothing at parse/typeset. Full decode +
//! mosaic rendering happen on the FIRST DRAW of any of the image's rows
//! (scrolling an image into view pays its decode, once), cached keyed
//! by (path, file signature, target cell size) across element rebuilds
//! (a search keystroke re-typesets the reader; it must not re-decode
//! the visible images). Decode failures degrade to the alt text with a
//! labeled notice — never silent, never fake pixels.
//!
//! ## Mosaic-only (the named open question)
//!
//! Pixel-protocol images (kitty/iTerm2/sixel) inside SCROLLABLE flowing
//! content are deliberately NOT attempted: protocol payloads bypass the
//! cell grid via `Presenter::external_write` and their
//! placement/eviction under partial visibility (a row-clipped image
//! half-scrolled off a viewport) is an unresolved engine-wide design
//! question owned by the damage contract (see 0144 §open design note
//! and `widgets::image`'s protocol-path seam). Mosaic cells are
//! cell-safe in ANY scroll context — that is the whole point.
//!
//! OWNER: READER (app-widgets wave 3).

use std::cell::RefCell;
use std::io::Read;
use std::rc::Rc;

use crate::base::{Point, Rect, Rgba};
use crate::gfx::mosaic::{self, MosaicGrid, MosaicMode};
use crate::gfx::{decode_image, probe_dimensions};
use crate::render::md::ImageBlock;
use crate::render::rich::{RichLine, Span};
use crate::render::Style;
use crate::theme::TokenSet;
use crate::ui::StyledCanvas;

use super::Row;

/// Layout guard: a pathological aspect ratio (1×10000 px) must not
/// explode the typeset row count; taller images shrink to keep their
/// aspect within this many rows.
const MAX_IMAGE_ROWS: i32 = 200;

/// Read-prefix ladder for header probing: most containers answer within
/// the first step; EXIF-heavy JPEGs within the second; the last step
/// reads the rest (still decode-free).
const PROBE_STEPS: [usize; 2] = [64 * 1024, 2 * 1024 * 1024];

/// One mosaic slice of an image block: `row` indexes into the decoded
/// grid; all slices share the handle (and therefore one decode).
#[derive(Clone)]
pub(crate) struct MdImageSlice {
    pub(crate) handle: Rc<MdImageHandle>,
    pub(crate) row: i32,
}

/// The shared per-image state: source, probed target size, and the
/// lazily-filled decode result.
pub(crate) struct MdImageHandle {
    src: String,
    alt: String,
    /// Target mosaic size in cells, fixed at typeset from the probe.
    cols: i32,
    rows: i32,
    state: RefCell<ImgState>,
}

enum ImgState {
    /// Not decoded yet (nothing drawn so far).
    Pending,
    Ready(Rc<MosaicGrid>),
    /// Decode failed after a successful probe: the labeled notice.
    Failed(String),
}

/// Typeset an image block at `width` (0144): probe → N mosaic rows +
/// an alt-text caption row; probe failure → a labeled notice row (+
/// caption). Never decodes.
pub(crate) fn push_image_rows(out: &mut Vec<Row>, image: &ImageBlock, width: i32, t: &TokenSet) {
    let faint = Style::new().fg(t.text_faint);
    match probe_file(&image.src) {
        Ok((px_w, px_h)) => {
            // Half-block geometry: 1 px per column, 2 px per row.
            // Native size capped to the content width, aspect kept.
            let width = width.max(1);
            let mut cols = (px_w as i32).clamp(1, width);
            let mut rows = ((cols as f64) * (px_h as f64) / (px_w as f64) / 2.0).round() as i32;
            rows = rows.max(1);
            if rows > MAX_IMAGE_ROWS {
                // Shrink, aspect-preserved, to the row guard.
                rows = MAX_IMAGE_ROWS;
                cols = ((rows as f64) * 2.0 * (px_w as f64) / (px_h as f64)).round() as i32;
                cols = cols.clamp(1, width);
            }
            let handle = Rc::new(MdImageHandle {
                src: image.src.clone(),
                alt: image.alt.clone(),
                cols,
                rows,
                state: RefCell::new(ImgState::Pending),
            });
            for row in 0..rows {
                out.push(Row {
                    line: RichLine::new(),
                    indent: 0,
                    ground: None,
                    quote: false,
                    rule: false,
                    image: Some(MdImageSlice {
                        handle: handle.clone(),
                        row,
                    }),
                });
            }
        }
        Err(reason) => {
            // Honest notice: the image is missing/unprobeable — labeled,
            // never a silent gap.
            out.push(Row::plain(RichLine::from_spans(vec![Span::new(
                format!("⌧ image unavailable: {} ({reason})", image.src),
                faint,
            )])));
        }
    }
    if !image.alt.is_empty() {
        out.push(Row::plain(RichLine::from_spans(vec![Span::new(
            image.alt.clone(),
            faint,
        )])));
    }
}

/// Draw one image slice row. First draw of any slice decodes + renders
/// the mosaic (cached); later draws blit cells.
pub(crate) fn draw_image_row(
    canvas: &mut dyn StyledCanvas,
    rect: Rect,
    y: i32,
    t: &TokenSet,
    indent: i32,
    slice: &MdImageSlice,
) {
    let handle = &slice.handle;
    // Resolve Pending exactly once (Ready and Failed are terminal).
    {
        let mut state = handle.state.borrow_mut();
        if matches!(*state, ImgState::Pending) {
            *state = match load_mosaic(&handle.src, handle.cols, handle.rows) {
                Ok(grid) => ImgState::Ready(grid),
                Err(reason) => ImgState::Failed(reason),
            };
        }
    }
    let state = handle.state.borrow();
    match &*state {
        ImgState::Ready(grid) => {
            let x0 = rect.x + indent;
            let cols = (grid.cols() as i32).min(rect.right() - x0);
            for col in 0..cols.max(0) {
                if let Some(cell) = grid.get(col as u32, slice.row as u32) {
                    canvas.put(Point::new(x0 + col, y), cell.ch, cell.fg, cell.bg);
                }
            }
        }
        ImgState::Failed(reason) => {
            // The first slice row carries the labeled failure; the rest
            // of the reserved rows stay blank (honest: the space was
            // sized by a header that lied about a decodable body).
            if slice.row == 0 {
                let label = if handle.alt.is_empty() {
                    &handle.src
                } else {
                    &handle.alt
                };
                canvas.print(
                    Point::new(rect.x + indent, y),
                    &format!("⌧ {label}: {reason}"),
                    t.text_faint,
                    Rgba::TRANSPARENT,
                );
            }
        }
        ImgState::Pending => unreachable!("resolved above"),
    }
}

// ---------------------------------------------------------------------------
// Probe + decode caches (thread-local: widgets are single-threaded by
// the engine's design; caches must survive ELEMENT REBUILDS, which a
// per-handle RefCell alone cannot)
// ---------------------------------------------------------------------------

/// Probe cache row: path, file signature, probed dims (None = not an
/// image / unprobeable at that signature).
type ProbeEntry = (String, u64, Option<(u32, u32)>);
/// Mosaic cache key: (path, file signature, cols, rows).
type MosaicKey = (String, u64, i32, i32);

thread_local! {
    /// path → (file signature, probed dims). Bounded LRU.
    static PROBE_CACHE: RefCell<Vec<ProbeEntry>> = const { RefCell::new(Vec::new()) };
    /// [`MosaicKey`] → mosaic grid. Bounded LRU.
    static MOSAIC_CACHE: RefCell<Vec<(MosaicKey, Rc<MosaicGrid>)>> =
        const { RefCell::new(Vec::new()) };
    /// Decode counter — the lazy-decode proof meter (tests assert it).
    static DECODE_COUNT: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

const PROBE_CACHE_CAP: usize = 128;
const MOSAIC_CACHE_CAP: usize = 32;

/// Total full decodes performed on this thread (test observability for
/// the 0144 laziness contract — the lazy-decode tests assert deltas).
#[cfg(test)]
pub(crate) fn decode_count() -> u64 {
    DECODE_COUNT.with(|c| c.get())
}

/// Cheap file identity: size + mtime nanos + the platform file id. A
/// rewritten file re-probes and re-decodes; an unchanged file never
/// re-reads.
///
/// mtime alone is NOT file identity (cycle-2 review R-3, the
/// JsonFileRunStore scan-memo class): a same-length rewrite on a
/// 1-second-granularity filesystem (HFS+, NFS, FAT) or under
/// mtime-preserving tooling (`rsync -a`, `tar`) would serve stale
/// pixels forever. On unix the device+inode pair folds in — the
/// write-tmp-then-rename pattern mints a NEW inode per rewrite, so
/// same-size same-mtime rewrites still invalidate. On windows the
/// creation time folds in (a rename-replace mints a new file; std's
/// volume/file-index accessors are unstable), which catches the same
/// rewrite pattern; an IN-PLACE same-size same-mtime overwrite remains
/// undetected there — documented degradation, cosmetic blast radius
/// (stale pixels in a doc reader, never wrong data).
fn file_signature(path: &str) -> Result<u64, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("unreadable: {e}"))?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let mut sig = meta.len() ^ mtime.rotate_left(17);
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        sig ^= meta.ino().rotate_left(31) ^ meta.dev().rotate_left(47);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        sig ^= meta.creation_time().rotate_left(31);
    }
    Ok(sig)
}

/// Header-only dimension probe with an incremental read ladder.
fn probe_file(path: &str) -> Result<(u32, u32), String> {
    let sig = file_signature(path)?;
    let cached = PROBE_CACHE.with(|c| {
        let cache = c.borrow();
        cache
            .iter()
            .find(|(p, s, _)| p == path && *s == sig)
            .map(|(_, _, dims)| *dims)
    });
    if let Some(dims) = cached {
        return dims.ok_or_else(|| "not a decodable image (PNG/JPEG headers only)".to_string());
    }
    let dims = probe_file_uncached(path);
    PROBE_CACHE.with(|c| {
        let mut cache = c.borrow_mut();
        cache.retain(|(p, _, _)| p != path);
        cache.push((path.to_string(), sig, dims));
        if cache.len() > PROBE_CACHE_CAP {
            cache.remove(0);
        }
    });
    dims.ok_or_else(|| "not a decodable image (PNG/JPEG headers only)".to_string())
}

fn probe_file_uncached(path: &str) -> Option<(u32, u32)> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut buf: Vec<u8> = Vec::new();
    for limit in PROBE_STEPS {
        if buf.len() < limit {
            let mut take = (&mut file).take((limit - buf.len()) as u64);
            if take.read_to_end(&mut buf).is_err() {
                return None;
            }
        }
        if let Some(dims) = probe_dimensions(&buf) {
            return Some(dims);
        }
        if buf.len() < limit {
            return None; // whole file read, still no header
        }
    }
    // Last step: the rest of the file (headers can hide behind huge
    // metadata segments; still no decode).
    if file.read_to_end(&mut buf).is_err() {
        return None;
    }
    probe_dimensions(&buf)
}

/// Full decode + mosaic render, LRU-cached by (path, signature, size).
fn load_mosaic(path: &str, cols: i32, rows: i32) -> Result<Rc<MosaicGrid>, String> {
    let sig = file_signature(path)?;
    let key = (path.to_string(), sig, cols, rows);
    let hit = MOSAIC_CACHE.with(|c| {
        let mut cache = c.borrow_mut();
        if let Some(pos) = cache.iter().position(|(k, _)| *k == key) {
            let entry = cache.remove(pos);
            let grid = entry.1.clone();
            cache.push(entry); // move-to-back: recently used
            Some(grid)
        } else {
            None
        }
    });
    if let Some(grid) = hit {
        return Ok(grid);
    }
    let bytes = std::fs::read(path).map_err(|e| format!("unreadable: {e}"))?;
    let bitmap = decode_image(&bytes).map_err(|e| format!("decode failed: {e}"))?;
    DECODE_COUNT.with(|c| c.set(c.get() + 1));
    let grid = Rc::new(mosaic::render(
        &bitmap,
        cols.max(1) as u32,
        rows.max(1) as u32,
        MosaicMode::HalfBlock,
    ));
    MOSAIC_CACHE.with(|c| {
        let mut cache = c.borrow_mut();
        cache.push((key, grid.clone()));
        if cache.len() > MOSAIC_CACHE_CAP {
            cache.remove(0);
        }
    });
    Ok(grid)
}

#[cfg(test)]
#[path = "markdown_image_tests.rs"]
mod tests;
