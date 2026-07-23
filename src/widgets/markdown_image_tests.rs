//! In-flow image tests: the LAZY-DECODE proof (typeset decodes
//! nothing; draw decodes visible images once), mosaic goldens through
//! the real pipeline, sizing/aspect math, and failure honesty.

use super::*;
use crate::base::Size;
use crate::gfx::{png_encode, Bitmap};
use crate::theme::default_theme;
use crate::widgets::test_util::{draw_into, row};
use crate::widgets::MarkdownView;

/// A temp PNG with a horizontal white→black split, `w`x`h` px.
fn temp_png(name: &str, w: u32, h: u32) -> std::path::PathBuf {
    let bmp = Bitmap::from_fn(
        w,
        h,
        |x, _| {
            if x < w / 2 {
                Rgba::WHITE
            } else {
                Rgba::BLACK
            }
        },
    );
    // The PROCESS id keeps concurrent test processes (parallel `cargo
    // test` invocations sharing one machine) from clobbering each
    // other's fixture files mid-test.
    let pid = std::process::id();
    let path = std::env::temp_dir().join(format!("abstracttui_mdimg_{pid}_{name}_{w}x{h}.png"));
    std::fs::write(&path, png_encode::encode(&bmp)).unwrap();
    path
}

#[test]
fn typeset_probes_sizes_without_decoding() {
    let t = default_theme().tokens;
    // Three images; typeset must decode NONE of them (0144 contract).
    let paths: Vec<_> = (0..3)
        .map(|i| temp_png(&format!("lazy{i}"), 8, 8))
        .collect();
    let doc: String = paths
        .iter()
        .map(|p| format!("![pic]({})\n\n", p.display()))
        .collect();
    let before = decode_count();
    let fold = super::super::doc::layout_doc(&doc, &t, 20);
    assert_eq!(decode_count(), before, "typeset must not decode");
    // 8x8 px at width 20: native 8 cols, 8/2 = 4 rows + caption.
    let image_rows = fold.rows.iter().filter(|r| r.image.is_some()).count();
    assert_eq!(image_rows, 3 * 4);
    for p in paths {
        let _ = std::fs::remove_file(p);
    }
}

#[test]
fn first_draw_decodes_visible_images_once_and_caches() {
    let t = default_theme().tokens;
    let a = temp_png("first", 8, 8);
    let b = temp_png("second", 8, 8);
    let doc = format!("![a]({})\n\n![b]({})", a.display(), b.display());
    let before = decode_count();
    // 4 image rows + caption + blank + ... — a 3-row window shows only
    // image A's slices.
    let c = draw_into(MarkdownView::new(&doc).element(&t), Size::new(20, 3));
    assert_eq!(decode_count(), before + 1, "only the VISIBLE image decodes");
    // Mosaic pixels actually landed: left half bright, right dark.
    let left = c.cell(crate::base::Point::new(1, 1)).unwrap();
    let right = c.cell(crate::base::Point::new(6, 1)).unwrap();
    assert!(left.2.r > 200, "{left:?}");
    assert!(right.2.r < 60, "{right:?}");
    // A second draw (fresh element, same doc): cache hit, no decode.
    let _ = draw_into(MarkdownView::new(&doc).element(&t), Size::new(20, 3));
    assert_eq!(decode_count(), before + 1, "rebuilds reuse the cache");
    // Scrolling image B into view pays exactly its one decode.
    let rows = MarkdownView::rows(&doc, &t, 20);
    let _ = draw_into(
        MarkdownView::new(&doc)
            .scroll_offset(rows as i32 - 2)
            .element(&t),
        Size::new(20, 3),
    );
    assert_eq!(decode_count(), before + 2, "image B decodes on first view");
    let _ = std::fs::remove_file(a);
    let _ = std::fs::remove_file(b);
}

#[test]
fn width_caps_and_aspect_are_preserved() {
    let t = default_theme().tokens;
    // 40x10 px at content width 20: cols cap to 20, rows keep aspect:
    // 20 * (10/40) / 2 = 2.5 -> 3 (rounded).
    let p = temp_png("wide", 40, 10);
    let doc = format!("![w]({})", p.display());
    let fold = super::super::doc::layout_doc(&doc, &t, 20);
    let image_rows = fold.rows.iter().filter(|r| r.image.is_some()).count();
    assert_eq!(image_rows, 3, "aspect-preserved rows at capped width");
    // Degenerate 1x600 px: the row guard keeps layout bounded.
    let tall = temp_png("tall", 1, 600);
    let doc = format!("![t]({})", tall.display());
    let fold = super::super::doc::layout_doc(&doc, &t, 20);
    let image_rows = fold.rows.iter().filter(|r| r.image.is_some()).count();
    assert!(
        image_rows as i32 <= MAX_IMAGE_ROWS,
        "row guard holds: {image_rows}"
    );
    let _ = std::fs::remove_file(p);
    let _ = std::fs::remove_file(tall);
}

#[test]
fn missing_file_degrades_to_labeled_notice_plus_alt_caption() {
    let t = default_theme().tokens;
    let doc = "![the alt text](/definitely/not/here.png)";
    let c = draw_into(MarkdownView::new(doc).element(&t), Size::new(72, 3));
    assert!(row(&c, 0).contains("image unavailable"), "{:?}", row(&c, 0));
    assert!(
        row(&c, 0).contains("/definitely/not/here.png"),
        "names the source: {:?}",
        row(&c, 0)
    );
    assert!(row(&c, 1).contains("the alt text"), "alt caption follows");
    // Never decodes, never panics — and no image rows were reserved.
    let fold = super::super::doc::layout_doc(doc, &t, 72);
    assert!(fold.rows.iter().all(|r| r.image.is_none()));
}

#[test]
fn undecodable_body_after_valid_header_fails_loudly_at_draw() {
    let t = default_theme().tokens;
    // A file with a VALID PNG header but a truncated body: probe
    // succeeds (rows reserved), decode fails at draw -> labeled row.
    let good = temp_png("truncate", 12, 12);
    let bytes = std::fs::read(&good).unwrap();
    let broken = std::env::temp_dir().join(format!(
        "abstracttui_mdimg_{}_broken.png",
        std::process::id()
    ));
    std::fs::write(&broken, &bytes[..40]).unwrap();
    let doc = format!("![broken pic]({})", broken.display());
    let fold = super::super::doc::layout_doc(&doc, &t, 20);
    assert!(fold.rows.iter().any(|r| r.image.is_some()), "probe passed");
    let c = draw_into(MarkdownView::new(&doc).element(&t), Size::new(40, 8));
    assert!(
        row(&c, 0).contains("broken pic") && row(&c, 0).contains('⌧'),
        "labeled decode failure with the alt text: {:?}",
        row(&c, 0)
    );
    let _ = std::fs::remove_file(good);
    let _ = std::fs::remove_file(broken);
}

/// R-3 (cycle-2 review): file identity must not be (size, mtime)
/// alone — the same-mtime-rewrite class. A rewrite via the
/// write-tmp-then-RENAME pattern with the SAME byte length and the
/// mtime pinned back to the original (1s-granularity filesystems,
/// `rsync -a`, `tar` all produce this shape) must still invalidate
/// both caches: the rename mints a new inode, and the signature folds
/// the platform file id in. Before the fix this test drew stale
/// pixels forever.
#[test]
#[cfg(unix)]
fn same_size_same_mtime_rename_rewrite_invalidates_the_caches() {
    let t = default_theme().tokens;
    let pid = std::process::id();
    let dir = std::env::temp_dir();
    let path = dir.join(format!("abstracttui_mdimg_{pid}_identity.png"));
    let tmp = dir.join(format!("abstracttui_mdimg_{pid}_identity.tmp.png"));

    // Two solid images with different pixels, then LENGTH-EQUALIZED by
    // padding the shorter one after IEND (the decoder stops at IEND —
    // trailing bytes are never read; if that ever changes, the decode
    // fails loudly and this test shows it). Keeping the size axis
    // constant is what makes the test discriminate the inode axis.
    let mut blue = png_encode::encode(&Bitmap::new(8, 4, Rgba::new(0, 0, 255, 255)));
    let mut red = png_encode::encode(&Bitmap::new(8, 4, Rgba::new(255, 0, 0, 255)));
    let widest = blue.len().max(red.len());
    blue.resize(widest, 0);
    red.resize(widest, 0);
    assert_eq!(
        blue.len(),
        red.len(),
        "precondition: equal lengths keep the size axis constant"
    );

    std::fs::write(&path, &blue).unwrap();
    let doc = format!("![p]({})", path.display());
    let draw = |doc: &str| draw_into(MarkdownView::new(doc).element(&t), Size::new(20, 3));

    let c = draw(&doc);
    let first = c.cell(crate::base::Point::new(2, 1)).unwrap();
    assert!(
        first.2.b > 200 && first.2.r < 60,
        "first decode shows blue: {first:?}"
    );
    let original_mtime = std::fs::metadata(&path).unwrap().modified().unwrap();

    // The rewrite: tmp + rename (mints a NEW inode), then pin the
    // mtime back to the original — size and mtime now both read
    // identical to the first file; only the inode differs.
    std::fs::write(&tmp, &red).unwrap();
    std::fs::rename(&tmp, &path).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(original_mtime)
        .unwrap();
    let meta = std::fs::metadata(&path).unwrap();
    assert_eq!(meta.len(), blue.len() as u64, "size axis unchanged");
    assert_eq!(
        meta.modified().unwrap(),
        original_mtime,
        "mtime axis unchanged — only the inode discriminates"
    );

    let before = decode_count();
    let c = draw(&doc);
    let after = c.cell(crate::base::Point::new(2, 1)).unwrap();
    assert!(
        after.2.r > 200 && after.2.b < 60,
        "the inode change must re-probe + re-decode: {after:?}"
    );
    assert_eq!(decode_count(), before + 1, "exactly one fresh decode");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn no_caption_row_for_empty_alt() {
    let t = default_theme().tokens;
    let p = temp_png("noalt", 4, 4);
    let doc = format!("![]({})", p.display());
    let fold = super::super::doc::layout_doc(&doc, &t, 20);
    // 4x4 -> 4 cols, 2 rows; no caption row after.
    assert_eq!(fold.rows.len(), 2, "{}", fold.rows.len());
    assert!(fold.rows.iter().all(|r| r.image.is_some()));
    let _ = std::fs::remove_file(p);
}
