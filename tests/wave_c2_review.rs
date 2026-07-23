//! Wave-3 cycle-2 cross-review discriminating probes
//! (reviews/wave3/review-cycle2.md). Edges the three lanes' own suites
//! left open, pinned through PUBLIC API only:
//!
//! - CONTENT2: sync-rebuild × `selected_key` interaction; the
//!   one-writer SELF-HEAL (C-1 demand landed: the old characterization
//!   pin flipped deliberately); NaN fingerprints (C-2, doc-closed —
//!   the cost pin stands); `TimeSeries` cadence-boundary edges
//!   (uniform gap padding after C-4's fix, jitter phantom gaps — C-5).
//! - READER: streamed-vs-batch equivalence amplified with fresh seeds
//!   and a hostile table corpus (CRLF, code-span pipes, `\|`,
//!   alignment-row lookalikes); CRLF⇄LF parse equality; wide-glyph
//!   (CJK/emoji) match cell ranges; slug dedup vs literal `-N`
//!   headings; image cache invalidation on file rewrite.
//! - INPUTAV: the 0293 fidelity flip WHILE a key is physically held
//!   (honest `is_down` gap + repeat recovery, through the real driver);
//!   the physical-fact rule where the SELECTION layer consumes the
//!   key (`c` copy) — key state must still observe it.
//!
//! Same harness posture as wave_r2_review.rs (helper duplication across
//! integration files is the house style — each is its own crate).
//!
//! OWNER: REVIEWER (wave 3, cycle 2).

use std::time::Duration;

use abstracttui::app::{key_state, use_key_state, App, Driver, KeyFidelity, RunConfig};
use abstracttui::base::{Point, Size};
use abstracttui::layout::Style as LayoutStyle;
use abstracttui::reactive::{flush_effects, run_due_timers, Signal};
use abstracttui::render::md::{self, DocBlock, DocStreamSession, MdStyles};
use abstracttui::term::Capabilities;
use abstracttui::testing::CaptureTerm;
use abstracttui::theme::default_theme;
use abstracttui::ui::{text, BufferCanvas, Element, Key, UiTree};
use abstracttui::widgets::{Feed, FeedItem, FeedState, MarkdownView, SyncSpec, TimeSeries};

// ---------------------------------------------------------------------------
// Shared harness
// ---------------------------------------------------------------------------

fn tokens() -> abstracttui::theme::TokenSet {
    default_theme().tokens
}

/// One full settle for `UiTree` harnesses (the feed recipe: effects →
/// layout → draw discovers width → due timers run the geometry fixup →
/// effects → layout → draw).
fn settle_tree(tree: &mut UiTree, size: Size) -> BufferCanvas {
    flush_effects();
    tree.layout();
    let mut c = BufferCanvas::new(size);
    tree.draw(&mut c);
    run_due_timers(std::time::Instant::now());
    flush_effects();
    tree.layout();
    let mut c = BufferCanvas::new(size);
    tree.draw(&mut c);
    c
}

/// Drive driver turns until idle (bounded).
fn settle_driver(driver: &mut Driver, app: &mut App, term: &mut CaptureTerm) {
    for _ in 0..64 {
        if driver.turn(app, term).expect("turn").idle {
            return;
        }
    }
    panic!("loop failed to settle within 64 turns");
}

/// Tiny deterministic LCG for chunk randomization (no deps).
struct Rng(u64);
impl Rng {
    /// `step`, not `next`: clippy's `should_implement_trait` flags a
    /// bare `next(&mut self)` as an Iterator lookalike.
    fn step(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn below(&mut self, n: usize) -> usize {
        (self.step() % n.max(1) as u64) as usize
    }
}

// ---------------------------------------------------------------------------
// CONTENT2 — FeedState::sync × selected_key, one-writer, fingerprints
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Msg {
    id: String,
    rev: u64,
    text: String,
}

fn msg(id: &str, text: &str) -> Msg {
    Msg {
        id: id.into(),
        rev: 0,
        text: text.into(),
    }
}

type SyncRig = (
    abstracttui::reactive::RootScope,
    UiTree,
    Signal<Vec<Msg>>,
    FeedState,
    Signal<Option<String>>,
);

/// The wires captured out of the mount closure (clippy::type_complexity
/// — INTEGRATOR, warnings sweep; alias only, zero behavior).
type SyncWires = (Signal<Vec<Msg>>, FeedState, Signal<Option<String>>);

/// Mount a synced feed with a bound `selected_key` signal.
fn mount_synced_with_selection(size: Size) -> SyncRig {
    use std::cell::RefCell;
    use std::rc::Rc;
    let holder: Rc<RefCell<Option<SyncWires>>> = Rc::new(RefCell::new(None));
    let h = holder.clone();
    let mut tree = UiTree::new(size);
    let (root, ()) = abstracttui::reactive::create_root(|cx| {
        let items: Signal<Vec<Msg>> = cx.signal(Vec::new());
        let sel: Signal<Option<String>> = cx.signal(None);
        let feed = FeedState::new(cx);
        feed.sync(
            cx,
            items,
            SyncSpec::new(
                |m: &Msg| m.id.clone(),
                |m| m.rev,
                |m| FeedItem::text(m.text.clone()),
            ),
        );
        *h.borrow_mut() = Some((items, feed.clone(), sel));
        let view = Element::new()
            .style(LayoutStyle::default().w(24).h(10))
            .child(Feed::new(&feed).selected_key(sel).view(cx))
            .build();
        tree.mount(cx, view);
    });
    let (items, feed, sel) = holder.borrow().clone().expect("state captured");
    let _ = settle_tree(&mut tree, size);
    (root, tree, items, feed, sel)
}

/// Rows whose background carries the selection tint.
fn selected_rows(c: &BufferCanvas, size: Size) -> Vec<i32> {
    let t = tokens();
    (0..size.h)
        .filter(|&y| {
            (0..size.w).any(|x| {
                c.cell(Point::new(x, y))
                    .is_some_and(|(_, _, bg)| bg == t.selection_bg)
            })
        })
        .collect()
}

/// A sync REBUILD (reorder) keeps the selection highlight glued to the
/// selected KEY (it moves with the item); a rebuild that drops the key
/// clears the highlight instead of tinting a stranger.
#[test]
fn sync_rebuild_preserves_selection_highlight_by_key() {
    let size = Size::new(24, 10);
    let (root, mut tree, items, feed, sel) = mount_synced_with_selection(size);
    items.set(vec![msg("a", "alpha"), msg("b", "beta"), msg("c", "gamma")]);
    sel.set(Some("b".into()));
    let c = settle_tree(&mut tree, size);
    let before = selected_rows(&c, size);
    assert!(!before.is_empty(), "precondition: b is highlighted");
    assert!(
        c.row_text(before[0]).contains("beta"),
        "highlight sits on b"
    );

    // Reorder = the rebuild path (clear + re-push). The selected key
    // survives the rebuild, so the highlight must follow it to its new
    // band — rebuilds must not orphan app selection state.
    items.set(vec![msg("b", "beta"), msg("a", "alpha"), msg("c", "gamma")]);
    let c = settle_tree(&mut tree, size);
    let after = selected_rows(&c, size);
    assert!(
        !after.is_empty(),
        "rebuild must keep the selected key highlighted"
    );
    assert!(
        c.row_text(after[0]).contains("beta"),
        "highlight follows the KEY, not the row index: {:?}",
        c.row_text(after[0])
    );
    assert_eq!(
        feed.row_of("b"),
        Some(after[0]),
        "row_of agrees with pixels"
    );

    // The selected item leaves the source: rebuild drops the key and
    // the highlight disappears (unknown keys highlight nothing).
    items.set(vec![msg("a", "alpha"), msg("c", "gamma")]);
    let c = settle_tree(&mut tree, size);
    assert!(
        selected_rows(&c, size).is_empty(),
        "a vanished key must not tint anything"
    );
    root.dispose();
}

/// Finding C-1, DEMAND LANDED (this pin flipped deliberately, per its
/// own comment): the one-writer contract is now GUARDED — every item
/// mutation bumps a feed-internal counter, and a drain that finds the
/// counter moved past the bridge's own record takes the rebuild path.
/// A manual `push` onto a synced feed is evicted at the very next
/// drain (append-only included — the old permanent-stray shape), and
/// feed order equals source order again.
#[test]
fn one_writer_violation_self_heals_at_the_next_drain() {
    let size = Size::new(24, 10);
    let (root, mut tree, items, feed, _sel) = mount_synced_with_selection(size);
    items.set(vec![msg("a", "alpha")]);
    flush_effects();

    // The violation: the app writes past the bridge.
    feed.push("stray", FeedItem::text("stray row"));
    assert_eq!(feed.len(), 2, "precondition: the stray landed");

    // ONE append-only drain later (the fast path before the guard) the
    // stray is gone and the mirror equals the source.
    items.update(|v| v.push(msg("b", "beta")));
    let c = settle_tree(&mut tree, size);
    let dump: Vec<String> = (0..size.h).map(|y| c.row_text(y)).collect();
    assert!(
        !dump.iter().any(|r| r.contains("stray row")),
        "the self-heal rebuild evicts the stray at the next drain: {dump:#?}"
    );
    assert_eq!(feed.len(), 2, "feed holds exactly the 2 mirrored items");

    // The order-divergence shape (a foreign key the source appends
    // later used to replace-in-place at the old index): heal restores
    // source order.
    feed.push("z", FeedItem::text("premature zeta"));
    items.update(|v| {
        v.push(msg("y", "yotta"));
        v.push(msg("z", "zeta"));
    });
    let c = settle_tree(&mut tree, size);
    let rows: Vec<i32> = ["a", "b", "y", "z"]
        .iter()
        .map(|k| feed.row_of(k).expect("mirrored key"))
        .collect();
    assert!(
        rows.windows(2).all(|w| w[0] < w[1]),
        "feed order equals source order after the heal: {rows:?}"
    );
    let dump: Vec<String> = (0..size.h).map(|y| c.row_text(y)).collect();
    assert!(
        !dump.iter().any(|r| r.contains("premature")),
        "the source's render won: {dump:#?}"
    );
    root.dispose();
}

/// Finding C-2 (doc-closed): a NaN fingerprint compares unequal to
/// itself, so the item re-renders on EVERY drain — never a panic,
/// never wrong pixels, but the "render runs only on change" promise
/// silently degrades to "every source change". The contract is now
/// DOCUMENTED in `SyncSpec::new` ("float fingerprints must compare by
/// bits — `to_bits`"); the behavior itself deliberately stands (a
/// user-supplied `PartialEq` cannot be fixed engine-side without
/// rejecting float fingerprints wholesale). Pin the safe half
/// (correct pixels, no panic) and count the cost.
#[test]
fn nan_fingerprint_rerenders_every_drain_but_stays_correct() {
    use std::cell::Cell;
    use std::rc::Rc;
    let size = Size::new(24, 8);
    let renders: Rc<Cell<usize>> = Rc::new(Cell::new(0));
    let r = renders.clone();
    let mut tree = UiTree::new(size);
    let holder: Rc<std::cell::RefCell<Option<Signal<Vec<Msg>>>>> =
        Rc::new(std::cell::RefCell::new(None));
    let h = holder.clone();
    let (root, ()) = abstracttui::reactive::create_root(|cx| {
        let items: Signal<Vec<Msg>> = cx.signal(vec![msg("a", "alpha")]);
        let feed = FeedState::new(cx);
        feed.sync(
            cx,
            items,
            SyncSpec::new(
                |m: &Msg| m.id.clone(),
                |_m| f32::NAN, // the footgun under test
                move |m| {
                    r.set(r.get() + 1);
                    FeedItem::text(m.text.clone())
                },
            ),
        );
        *h.borrow_mut() = Some(items);
        let view = Element::new()
            .style(LayoutStyle::default().w(24).h(8))
            .child(Feed::new(&feed).view(cx))
            .build();
        tree.mount(cx, view);
    });
    let items = holder.borrow().unwrap();
    flush_effects();
    let base = renders.get();
    // Three untouched drains: `a` never changed, yet NaN != NaN makes
    // every drain re-render it.
    for _ in 0..3 {
        items.update(|_| {});
        flush_effects();
    }
    assert_eq!(
        renders.get(),
        base + 3,
        "NaN fingerprints re-render per drain (the documented-cost gap, C-2)"
    );
    let c = settle_tree(&mut tree, size);
    assert!(c.row_text(0).contains("alpha"), "pixels stay correct");
    root.dispose();
}

// ---------------------------------------------------------------------------
// CONTENT2 — TimeSeries cadence-boundary edges
// ---------------------------------------------------------------------------

const MS: fn(u64) -> Duration = Duration::from_millis;

/// Finding C-4, DEMAND LANDED (this pin flipped deliberately): the old
/// `missed >= capacity` restart was a display discontinuity — one slot
/// more of silence flipped "window of hole" into "a single dot at span
/// zero", contradicting the module's own gap claim. Padding is now
/// capped at `capacity - 1` per push, so the gap display is UNIFORM at
/// every pause length: at, past, and far past the window, the ring
/// shows a full window of hole ending in the fresh sample.
#[test]
fn timeseries_pause_padding_is_uniform_at_and_past_the_window() {
    let window_of_hole = |pause_to: Duration| {
        let mut ts = TimeSeries::with_slots(MS(100), 4);
        ts.push(MS(0), 1.0);
        ts.push(pause_to, 2.0);
        (ts.samples(), ts.span())
    };
    // missed == capacity - 1, == capacity, and >> capacity all land
    // the SAME shape: [NAN × 3, v], span = 3 slots. No boundary left.
    for pause_to in [MS(400), MS(500), Duration::from_secs(3600)] {
        let (s, span) = window_of_hole(pause_to);
        assert_eq!(s.len(), 4, "pause to {pause_to:?}: {s:?}");
        assert!(
            s[0].is_nan() && s[1].is_nan() && s[2].is_nan(),
            "pause to {pause_to:?} shows a window of hole: {s:?}"
        );
        assert_eq!(s[3], 2.0);
        assert_eq!(span, MS(300), "span stays honest at {pause_to:?}");
    }
}

/// Jitter around slot boundaries: a producer pushing at wall-clock
/// `interval == cadence` WITH JITTER coalesces two samples into one
/// slot (latest wins — sample loss) and pads the skipped neighbor with
/// NAN (a phantom pause the chart draws as a hole). Producers must
/// either drive a jitter-free clock (the dashboard's interval) or pick
/// `cadence` comfortably above their push jitter — finding C-5's doc
/// ask lives next to this pin.
#[test]
fn timeseries_jittered_pushes_at_cadence_produce_phantom_gaps() {
    let mut ts = TimeSeries::with_slots(MS(100), 8);
    ts.push(MS(0), 1.0); // slot 0
    ts.push(MS(101), 2.0); // slot 1
    ts.push(MS(199), 3.0); // slot 1 again (jitter): 2.0 is LOST
    ts.push(MS(305), 4.0); // slot 3: slot 2 pads NAN (phantom pause)
    let s = ts.samples();
    assert_eq!(s.len(), 4);
    assert_eq!((s[0], s[1], s[3]), (1.0, 3.0, 4.0), "{s:?}");
    assert!(
        s[2].is_nan(),
        "the skipped slot reads as a pause that never happened: {s:?}"
    );
}

// ---------------------------------------------------------------------------
// READER — table parser hostility + streamed-vs-batch amplification
// ---------------------------------------------------------------------------

fn styles() -> MdStyles {
    MdStyles::default()
}

/// Hostile-table corpus the lane's own fixed corpus does not cover:
/// code-span pipes, escaped pipes at cell edges, alignment lookalikes,
/// CRLF line endings, wide glyphs in cells, tables abutting every
/// extended block kind.
fn hostile_docs() -> Vec<String> {
    vec![
        // Code-span pipes split cells (GFM rule: only `\|` protects).
        "| a `x|y` b | c |\n|---|---|\n| `1|2` | 3 |\n".into(),
        // Escaped pipes at cell boundaries + trailing backslash.
        "| a \\| b | c\\\\ |\n|:-:|--:|\n| \\| | x\\ |\n".into(),
        // Alignment-row lookalike with the WRONG cell count: no table.
        "prose with | one pipe\n|---|---|---|\nmore prose\n".into(),
        // Alignment row alone (no header): prose, never a table.
        "|---|---|\n| a | b |\n".into(),
        // Delimiter-lookalike BODY row stays a body row.
        "| h1 | h2 |\n|---|---|\n| --- | --- |\n| x | y |\n".into(),
        // CRLF everywhere, table included.
        "# t\r\n\r\n| a | b |\r\n|---|---|\r\n| 1 | 2 |\r\n\r\ntail\r\n".into(),
        // Wide glyphs + ZWJ emoji inside cells.
        "| 名前 | 値 |\n|---|---|\n| 👍🏽 ok | 字字 |\n".into(),
        // Table interrupted by an image line, then a task.
        "| a |\n|---|\n| r1 |\n![alt](x.png)\n- [x] done\n".into(),
        // Paragraph joint straight into a table header.
        "prose line one\nprose | two\n| h | i |\n|---|---|\n| 1 | 2 |\n".into(),
        // Header candidate never resolved (EOF right after).
        "| lonely | header |\n".into(),
        // Header + delimiter split by a blank line: no table.
        "| a | b |\n\n|---|---|\n".into(),
        // Fence guarding a would-be table.
        "```\n| a | b |\n|---|---|\n```\n| c |\n|---|\n".into(),
        // Tab-indented pipe rows and empty cells.
        "|  | b |  |\n|---|---|---|\n| | | |\n".into(),
        // A one-column table (minimal delimiter).
        "| only |\n|---|\n| row |\n".into(),
    ]
}

/// CRLF sources parse to the SAME blocks as their LF twins — the
/// batch parser strips `\r\n` uniformly (`str::lines`), and the stream
/// seal must agree (covered by the amplified equivalence below).
#[test]
fn crlf_tables_parse_identically_to_lf() {
    let styles = styles();
    let lf = "# h\n\n| a | b |\n|---|--:|\n| 1 | 2 |\n\n- [x] done\n\ntail";
    let crlf = lf.replace('\n', "\r\n");
    assert_eq!(
        md::parse_doc(lf, &styles),
        md::parse_doc(&crlf, &styles),
        "CRLF and LF sources must yield identical blocks"
    );
    assert!(
        md::parse_doc(lf, &styles)
            .iter()
            .any(|b| matches!(b, DocBlock::Table(_))),
        "precondition: the table parsed at all"
    );
}

/// GFM cell-splitting truth pins: a pipe inside a code span still
/// splits (only `\|` protects), and a prose line + delimiter-shaped
/// successor with MATCHING cell count opens a table (the GFM behavior,
/// deliberate — pinned so nobody "fixes" it into drift).
#[test]
fn code_span_pipes_split_cells_and_matching_lookalikes_open_tables() {
    let styles = styles();
    let blocks = md::parse_doc("| a `x|y` b |\n|---|---|\n", &styles);
    let DocBlock::Table(t) = &blocks[0] else {
        panic!("expected a table: {blocks:?}");
    };
    assert_eq!(t.columns(), 2, "the code-span pipe SPLITS (GFM rule)");

    // Prose with one pipe + a 2-cell delimiter: 2 header cells match.
    let blocks = md::parse_doc("use a | for pipes\n|---|---|\nrest\n", &styles);
    assert!(
        matches!(blocks[0], DocBlock::Table(_)),
        "matching lookalike opens a table (GFM): {blocks:?}"
    );

    // Mismatched cell count: stays prose.
    let blocks = md::parse_doc("use a | for pipes\n|---|---|---|\n", &styles);
    assert!(
        !blocks.iter().any(|b| matches!(b, DocBlock::Table(_))),
        "cell-count mismatch must not open a table: {blocks:?}"
    );
}

/// The 0142 equivalence contract amplified: fresh seeds, per-char
/// chunking, line chunking and randomized chunkings over the hostile
/// corpus — `finish()` must equal `parse_doc` of the whole source for
/// EVERY cut. (The lane's own test rig runs a fixed corpus at one
/// seed; this widens it — cheap and deterministic.)
#[test]
fn doc_stream_equivalence_holds_over_hostile_corpus_and_fresh_seeds() {
    let styles = styles();
    let mut rng = Rng(0xC2_2026_0723);
    for (di, doc) in hostile_docs().into_iter().enumerate() {
        let expected = md::parse_doc(&doc, &styles);
        let char_idx: Vec<usize> = {
            let mut v: Vec<usize> = doc.char_indices().map(|(i, _)| i).collect();
            v.push(doc.len());
            v
        };
        let n = char_idx.len() - 1;
        // Chunkings: per-char, whole, and 6 randomized cuts per doc.
        let mut chunkings: Vec<Vec<String>> = Vec::new();
        chunkings.push(
            (0..n)
                .map(|i| doc[char_idx[i]..char_idx[i + 1]].to_string())
                .collect(),
        );
        chunkings.push(vec![doc.clone()]);
        for _ in 0..6 {
            let mut chunks = Vec::new();
            let mut i = 0;
            while i < n {
                let take = 1 + rng.below(5).min(n - i - 1);
                chunks.push(doc[char_idx[i]..char_idx[i + take]].to_string());
                i += take;
            }
            chunkings.push(chunks);
        }
        for (ci, chunks) in chunkings.into_iter().enumerate() {
            let mut session = DocStreamSession::new(styles.clone());
            for chunk in &chunks {
                session.append(chunk);
            }
            assert_eq!(
                session.finish(),
                expected,
                "doc {di} chunking {ci}: streamed != batch\nsource: {doc:?}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// READER — wide-glyph match cells, slug dedup, image cache invalidation
// ---------------------------------------------------------------------------

/// Match cells for WIDE content: a CJK query's cell range spans the
/// glyph's two columns; a ZWJ emoji match covers the whole cluster; an
/// end-of-row wide match closes at the row's true end column.
#[test]
fn wide_glyph_matches_report_two_column_cells() {
    let t = tokens();
    // "本" inside CJK text: starts after one wide glyph (2 cols).
    let m = MarkdownView::find("日本語 words", &t, 40, "本", false);
    assert_eq!(m.len(), 1);
    assert_eq!(
        m[0].cells,
        (2, 4),
        "one wide glyph before, one wide glyph matched: {m:?}"
    );
    // ZWJ emoji (👍🏽, one cluster, 2 columns): byte range covers the
    // whole cluster, cells cover both columns.
    let doc = "ok 👍🏽 done";
    let m = MarkdownView::find(doc, &t, 40, "👍🏽", false);
    assert_eq!(m.len(), 1);
    assert_eq!(m[0].cells.1 - m[0].cells.0, 2, "{m:?}");
    assert_eq!(&doc[m[0].bytes.0..m[0].bytes.1], "👍🏽");
    // Match ENDING the row: end column == row's end.
    let m = MarkdownView::find("abc 字", &t, 40, "字", false);
    assert_eq!(m.len(), 1);
    assert_eq!(m[0].cells, (4, 6), "{m:?}");
}

/// Slug dedup vs literal `-N` headings, both orders (the GitHub probe
/// rule: generated suffixes skip slugs literal headings already took,
/// and literals colliding with generated ones probe deeper).
#[test]
fn slug_dedup_survives_literal_suffix_collisions_both_orders() {
    // Literal "x-1" sits between two "x" duplicates.
    let ids: Vec<String> = md::outline("# x\n\n# x-1\n\n# x\n\n# x\n")
        .into_iter()
        .map(|h| h.anchor_id)
        .collect();
    assert_eq!(ids, ["x", "x-1", "x-2", "x-3"]);

    // Generated "x-1" lands first; the literal "x-1" then probes on.
    let ids: Vec<String> = md::outline("# x\n\n# x\n\n# x-1\n")
        .into_iter()
        .map(|h| h.anchor_id)
        .collect();
    assert_eq!(ids, ["x", "x-1", "x-1-1"]);
}

/// Rewriting an image file (new content, new mtime/size) invalidates
/// both caches: the next draw re-probes and re-decodes — pixels follow
/// the file. (The mtime+size signature's same-mtime hole is finding
/// R-3; this pins the reachable half of the contract.)
#[test]
fn image_rewrite_invalidates_the_decode_cache() {
    use abstracttui::base::Rgba;
    use abstracttui::gfx::{png_encode, Bitmap};
    let t = tokens();
    let pid = std::process::id();
    let path = std::env::temp_dir().join(format!("abstracttui_c2rev_{pid}_swap.png"));
    let write = |color: Rgba, w: u32| {
        std::fs::write(&path, png_encode::encode(&Bitmap::new(w, 4, color))).unwrap();
    };
    write(Rgba::WHITE, 8);
    let doc = format!("![p]({})", path.display());
    let size = Size::new(20, 3);
    let draw = |doc: &str| {
        let mut tree = UiTree::new(size);
        let (root, ()) = abstracttui::reactive::create_root(|cx| {
            let view = Element::new()
                .style(LayoutStyle::default().w(20).h(3))
                .child(MarkdownView::new(doc).element(&t).build())
                .build();
            tree.mount(cx, view);
        });
        let c = settle_tree(&mut tree, size);
        root.dispose();
        c
    };
    let c = draw(&doc);
    let bright = c.cell(Point::new(2, 0)).unwrap();
    assert!(bright.2.r > 200, "first decode shows white: {bright:?}");

    // Rewrite with different content + dimensions (mtime advances on
    // any modern fs; the size axis changes too — both signature
    // ingredients move).
    std::thread::sleep(Duration::from_millis(15));
    write(Rgba::BLACK, 10);
    let c = draw(&doc);
    let dark = c.cell(Point::new(2, 0)).unwrap();
    assert!(
        dark.2.r < 60,
        "a rewritten file must re-probe + re-decode: {dark:?}"
    );
    let _ = std::fs::remove_file(&path);
}

// ---------------------------------------------------------------------------
// INPUTAV — fidelity flip mid-hold; physical-fact rule vs the
// selection layer's key claims
// ---------------------------------------------------------------------------

/// The 0293 probe upgrade lands WHILE a key is physically held (its
/// press arrived on the legacy wire). The service must stay honest
/// through the flip: no fabricated hold at the flip moment, the first
/// kitty REPEAT proves the hold (without a new press edge), the wire
/// release ends it.
#[test]
fn fidelity_flip_mid_hold_recovers_via_repeat_without_faking() {
    let mut app = App::new(Size::new(30, 4));
    app.mount(|cx| {
        let _ = use_key_state(cx);
        Element::new().child(text("flip rig")).build()
    })
    .expect("mount");
    let mut term = CaptureTerm::new(Size::new(30, 4));
    let cfg = RunConfig {
        caps: Some(Capabilities::with(|c| {
            c.truecolor = true;
            c.colors_256 = true;
        })),
        enter: None,
        probe: true, // the 0293 shape: proof arrives mid-session
    };
    let mut driver = Driver::new(&mut app, &mut term, cfg).expect("driver");
    settle_driver(&mut driver, &mut app, &mut term);
    let ks = key_state();
    assert_eq!(ks.fidelity_untracked(), KeyFidelity::Degraded);

    // Legacy press: the user pushes 'w' down and HOLDS it.
    term.push_input(b"w");
    driver.turn(&mut app, &mut term).expect("legacy press turn");
    assert!(ks.pressed(Key::Char('w')), "press edge honest on legacy");
    assert!(!ks.is_down(Key::Char('w')), "no faked hold on Degraded");

    // The probe answers mid-hold: kitty keyboard proof + DA1 sentinel.
    term.push_input(b"\x1b[?1u\x1b[?62c");
    driver.turn(&mut app, &mut term).expect("probe fold turn");
    assert_eq!(
        ks.fidelity_untracked(),
        KeyFidelity::Full,
        "flags follow the probe (0293) and fidelity follows the flags"
    );
    // Honest gap: the hold predates the protocol — never fabricated.
    assert!(
        !ks.is_down(Key::Char('w')),
        "the flip must not invent a hold it never observed"
    );

    // The held key's first kitty REPEAT proves the hold (no press edge).
    term.push_input(b"\x1b[119;1:2u");
    driver.turn(&mut app, &mut term).expect("repeat turn");
    assert!(ks.is_down(Key::Char('w')), "repeat proves the hold");
    assert!(
        !ks.pressed(Key::Char('w')),
        "recovery must not synthesize a press edge (capture surfaces \
         would auto-start)"
    );

    // The wire release ends it.
    term.push_input(b"\x1b[119;1:3u");
    driver.turn(&mut app, &mut term).expect("release turn");
    assert!(!ks.is_down(Key::Char('w')));
    assert!(ks.released(Key::Char('w')));
}

/// The physical-fact rule against the SELECTION layer's key claims:
/// a selection is visible only MID-DRAG (0290: every copy ends the
/// gesture), and during the drag `c` is consumed by the layer (copy +
/// clear) — the key-state tap must still observe the press edge.
#[test]
fn key_state_observes_keys_the_selection_layer_consumes() {
    let mut app = App::new(Size::new(30, 6));
    app.mount(|cx| {
        let _ = use_key_state(cx);
        let mut col = Element::new().style(LayoutStyle::column());
        for i in 0..6 {
            col = col.child(text(format!("row {i} selectable text")));
        }
        col.build()
    })
    .expect("mount");
    let mut term = CaptureTerm::new(Size::new(30, 6));
    let cfg = RunConfig {
        caps: Some(Capabilities::with(|c| {
            c.truecolor = true;
            c.colors_256 = true;
        })),
        enter: None,
        probe: false,
    };
    let mut driver = Driver::new(&mut app, &mut term, cfg).expect("driver");
    abstracttui::app::selection::selection().set_enabled(true);
    settle_driver(&mut driver, &mut app, &mut term);

    // Anchor + drag, button STILL HELD: the region is visible now
    // (release would copy-and-clear it — the 0290 rule).
    term.push_input(b"\x1b[<0;2;1M\x1b[<32;12;2M");
    driver.turn(&mut app, &mut term).expect("drag turn");
    let sel = abstracttui::app::selection::selection();
    assert!(
        sel.is_active(),
        "precondition: a selection is visible mid-drag"
    );

    // `c` copies AND clears — the layer consumes the key…
    let ks = key_state();
    term.push_input(b"c");
    driver.turn(&mut app, &mut term).expect("copy turn");
    assert!(
        ks.pressed(Key::Char('c')),
        "key state must observe keys the selection layer consumes \
         (the physical-fact rule)"
    );
    assert!(!sel.is_active(), "proof the layer consumed it: copy+clear");
}
