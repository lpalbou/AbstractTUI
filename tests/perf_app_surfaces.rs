//! Perf budgets for the APP-LAYER surfaces the 0.2.x waves shipped
//! (content/live-data, composer/selection, activation/select/diff) —
//! the sibling of tests/perf_budgets.rs (engine primitives), split per
//! the file budget. Same doctrine: `#[ignore]`d, release-only asserts,
//! run serially:
//!
//! ```sh
//! cargo test --test perf_app_surfaces --release -- --ignored --test-threads=1 --nocapture
//! ```
//!
//! Every measurement drives the REAL frame loop (`Driver::turn` against
//! `CaptureTerm` — the production pipeline, no tty) so the numbers
//! include dispatch, effects, layout, damage redraw, flatten, diff,
//! present. Byte counts are printed beside timings: the damage
//! contract's claim is not just "fast" but "emission proportional to
//! change", so the steady-state byte cost is asserted against the full
//! first-paint as the proportionality witness.
//!
//! Budgets carry slack over quiet-host medians (timing tests are
//! load-sensitive; the failure message names the measurement so drift
//! is visible). Debug builds print but refuse to assert.

use std::time::Duration;

use abstracttui::app::anchored::{Completion, CompletionCandidate};
use abstracttui::app::{App, Driver, RunConfig};
use abstracttui::term::Capabilities;
use abstracttui::testing::{sink, time_median, CaptureTerm, Measurement};
use abstracttui::ui::text;
use abstracttui::widgets::{CodeView, Feed, FeedItem, FeedState};

use abstracttui::prelude::*;

fn assert_budget(m: &Measurement, budget: Duration) {
    if cfg!(debug_assertions) {
        eprintln!("[debug build, budget not asserted] {}", m.report());
    } else {
        eprintln!("{}", m.report());
        m.assert_under(budget);
    }
}

fn fixed_caps() -> Capabilities {
    // Deterministic: host env must never leak into measurements.
    Capabilities::with(|c| {
        c.truecolor = true;
        c.colors_256 = true;
        c.sync_output_2026 = true;
    })
}

fn config() -> RunConfig {
    RunConfig {
        caps: Some(fixed_caps()),
        enter: None,
        probe: false,
    }
}

/// Drive turns until idle (bounded), so measurements start from a
/// settled frame.
fn settle(driver: &mut Driver, app: &mut App, term: &mut CaptureTerm) {
    for _ in 0..64 {
        if driver.turn(app, term).expect("turn").idle {
            return;
        }
    }
    panic!("loop failed to settle within 64 turns");
}

fn median_of(mut v: Vec<usize>) -> usize {
    v.sort_unstable();
    v.get(v.len() / 2).copied().unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Feed: steady-state token streaming (the transcript example's shape).
// ---------------------------------------------------------------------------

/// A 90x30 transcript: 40 finished messages of history, one OPEN
/// streaming answer, `Scroll::follow_tail` pinned. Each iteration is
/// one 60fps-style tick: one ~6-char token appended, one frame. The
/// claim under test: a token costs ONE open-block re-typeset plus
/// emission proportional to the rows that changed — never the
/// document, never the screen.
#[test]
#[ignore]
fn perf_feed_streaming_token_frame_90x30() {
    let size = Size::new(90, 30);
    let mut term = CaptureTerm::new(size);
    let mut app = App::new(size);
    let mut wiring = None;
    app.mount(|cx| {
        let feed = FeedState::new(cx);
        for i in 0..40 {
            feed.push(
                format!("h{i}"),
                FeedItem::markdown(format!(
                    "**turn {i}** — a finished message with `code` and *emphasis* riding \
                     a fairly long line that wraps at ninety columns"
                )),
            );
        }
        feed.push_stream("live");
        feed.stream_append("live", "# Streaming\n\nThe answer begins ");
        let follow = cx.signal(true);
        wiring = Some(feed.clone());
        Element::new()
            .style(LayoutStyle::column())
            .child(
                Element::new()
                    .style(LayoutStyle::column().grow(1.0))
                    .child(
                        Scroll::new(Feed::new(&feed).view(cx))
                            .follow_tail(follow)
                            .view(cx),
                    )
                    .build(),
            )
            .child(text(" status: streaming"))
            .build()
    })
    .expect("mount");
    let feed = wiring.expect("feed handle");
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);
    let full_paint = term.take_bytes().len();

    // Token mix: mostly words; a paragraph break every 32 tokens seals
    // the open block (StreamSession's freeze) exactly as real markdown
    // streams do.
    let mut frame_bytes: Vec<usize> = Vec::new();
    let m = time_median(
        "feed stream token->frame 90x30 (40-item history)",
        16,
        9,
        32,
        |i| {
            let token = if i % 32 == 31 { "\n\nnext " } else { "lorem " };
            feed.stream_append("live", token);
            let turn = driver.turn(&mut app, &mut term).expect("turn");
            assert!(turn.rendered, "a token must repaint");
            frame_bytes.push(term.take_bytes().len());
            sink(turn.emitted);
        },
    );
    let med = median_of(frame_bytes.clone());
    let max = frame_bytes.iter().copied().max().unwrap_or(0);
    eprintln!(
        "feed stream emission: median {med} B/frame, max {max} B/frame, first paint {full_paint} B"
    );
    // Damage proportionality: a steady token frame re-emits the open
    // block's rows (plus scroll bookkeeping), never the screen.
    assert!(
        med * 3 < full_paint,
        "token frames should emit a fraction of a full paint: {med} vs {full_paint}"
    );
    assert_budget(&m, Duration::from_millis(3));
}

// ---------------------------------------------------------------------------
// Select: popup open/close through the real loop.
// ---------------------------------------------------------------------------

/// A 100x30 settings screen; one open/close cycle = Enter (popup up,
/// 5 rows) + Escape (vacated region repaints from below). Bytes for
/// both halves are asserted against the full paint: opening damages
/// the popup rect, closing repaints only what it vacated.
#[test]
#[ignore]
fn perf_select_popup_open_close_100x30() {
    let size = Size::new(100, 30);
    let mut term = CaptureTerm::new(size);
    let mut app = App::new(size);
    app.mount(|cx| {
        let value = cx.signal(0usize);
        let mut col = Element::new()
            .style(LayoutStyle::column())
            .child(text("== settings =="))
            .child(
                Select::new(vec![
                    SelectOption::new("stable").hint("lts"),
                    SelectOption::new("beta"),
                    SelectOption::new("nightly").hint("daily"),
                    SelectOption::new("archive").disabled(true),
                    SelectOption::new("custom"),
                ])
                .value(value)
                .layout(LayoutStyle::default().w(24).h(1).shrink(0.0))
                .view(cx),
            );
        for i in 0..24 {
            col = col.child(text(format!(
                "content row {i} — static chrome under the popup"
            )));
        }
        col.build()
    })
    .expect("mount");
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);
    // Focus the trigger (first focusable) before the measured cycles.
    term.push_input(b"\t");
    settle(&mut driver, &mut app, &mut term);
    let full_paint = term.take_bytes().len();

    let mut open_bytes: Vec<usize> = Vec::new();
    let mut close_bytes: Vec<usize> = Vec::new();
    // One iteration = one full open+close round trip (two frames).
    let m = time_median("select popup open+close cycle 100x30", 4, 9, 16, |_| {
        term.push_input(b"\r"); // Enter on the focused trigger: open
        let turn = driver.turn(&mut app, &mut term).expect("open turn");
        assert!(turn.rendered, "open must paint the popup");
        open_bytes.push(term.take_bytes().len());
        term.push_input(b"\x1b[27u"); // kitty-encoded Escape: unambiguous close
        let turn = driver.turn(&mut app, &mut term).expect("close turn");
        assert!(turn.rendered, "close must repaint the vacated region");
        close_bytes.push(term.take_bytes().len());
        sink(turn.emitted);
    });
    let open_med = median_of(open_bytes);
    let close_med = median_of(close_bytes);
    eprintln!(
        "select popup emission: open median {open_med} B, close median {close_med} B, \
         first paint {full_paint} B"
    );
    assert!(
        open_med * 3 < full_paint,
        "popup open must stay damage-bounded: {open_med} vs {full_paint}"
    );
    assert!(
        close_med * 3 < full_paint,
        "popup close must repaint only the vacated region: {close_med} vs {full_paint}"
    );
    assert_budget(&m, Duration::from_millis(6)); // two frames per cycle
}

// ---------------------------------------------------------------------------
// Selection: drag extension over a full 200x60 screen.
// ---------------------------------------------------------------------------

/// The selection layer's worst realistic frame: a full-screen region
/// whose head moves one row per frame — old ∪ new row rects are
/// damaged, the compositor recomposes them, the inks re-patch, the
/// diff emits the delta. Time is the budget; bytes prove the diff only
/// pays for the rows that actually changed.
#[test]
#[ignore]
fn perf_selection_drag_full_screen_200x60() {
    let size = Size::new(200, 60);
    let mut term = CaptureTerm::new(size);
    let mut app = App::new(size);
    app.mount(|_cx| {
        let mut col = Element::new().style(LayoutStyle::column());
        for y in 0..60 {
            col = col.child(text(format!(
                "row {y:02} abcdefghij0123456789 abcdefghij0123456789 abcdefghij0123456789 \
                 abcdefghij0123456789 abcdefghij0123456789 abcdefghij0123456789"
            )));
        }
        col.build()
    })
    .expect("mount");
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    abstracttui::app::selection::selection().set_enabled(true);
    settle(&mut driver, &mut app, &mut term);
    let _ = term.take_bytes();

    // Anchor top-left, head at the bottom: the region spans the screen.
    term.push_input(b"\x1b[<0;1;1M");
    term.push_input(b"\x1b[<32;200;59M");
    driver.turn(&mut app, &mut term).expect("initial drag");
    let _ = term.take_bytes();

    let mut frame_bytes: Vec<usize> = Vec::new();
    let m = time_median("selection drag extend 1 row @ 200x60", 4, 9, 16, |i| {
        // Oscillate the head over the last row: every frame changes
        // exactly one row of a ~12,000-cell region.
        let y = if i % 2 == 0 { 60 } else { 59 };
        term.push_input(format!("\x1b[<32;200;{y}M").as_bytes());
        let turn = driver.turn(&mut app, &mut term).expect("drag turn");
        assert!(turn.rendered, "a drag move must repaint");
        frame_bytes.push(term.take_bytes().len());
        sink(turn.emitted);
    });
    let med = median_of(frame_bytes);
    eprintln!("selection drag emission: median {med} B/frame (one changed row of 200 cells)");
    assert_budget(&m, Duration::from_millis(5));
}

// ---------------------------------------------------------------------------
// TextArea + completion: per-keystroke cost with the dropdown open.
// ---------------------------------------------------------------------------

/// The transcript composer under fingers: a 1..4-row TextArea with a
/// '/' completion dropdown OPEN, in a 90x30 app. Each iteration is one
/// keystroke (insert or backspace) that re-filters the candidates and
/// re-renders the anchored panel. Charter: keystroke -> frame < 3 ms.
#[test]
#[ignore]
fn perf_textarea_keystroke_with_completion_open_90x30() {
    let size = Size::new(90, 30);
    let mut term = CaptureTerm::new(size);
    let mut app = App::new(size);
    let overlays = app.overlays();
    app.mount(move |cx| {
        let t = use_theme(cx).get().tokens;
        let state = TextAreaState::new(cx);
        let composer = TextArea::new()
            .state(&state)
            .rows(1, 4)
            .placeholder("message")
            .element(cx, &t)
            .autofocus()
            .build();
        let wrapped = Completion::new()
            .trigger('/', |q| {
                ["help", "theme", "clear", "history", "quit"]
                    .iter()
                    .filter(|c| c.starts_with(q))
                    .map(|c| {
                        CompletionCandidate::new(format!("/{c}"), format!("/{c} ")).detail("cmd")
                    })
                    .collect()
            })
            .max_visible(5)
            .attach(cx, &overlays, &state, composer);
        let mut col = Element::new().style(LayoutStyle::column());
        for i in 0..27 {
            col = col.child(text(format!("transcript row {i}")));
        }
        col.child(wrapped).build()
    })
    .expect("mount");
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);

    // Open the dropdown: '/' alone matches all five candidates.
    term.push_input(b"/");
    settle(&mut driver, &mut app, &mut term);
    let _ = term.take_bytes();

    let mut frame_bytes: Vec<usize> = Vec::new();
    let m = time_median(
        "keystroke->frame, completion open, 4-row composer",
        4,
        11,
        20,
        |i| {
            // 'h' narrows to help/history; backspace restores all five —
            // both keys re-filter and re-render the panel.
            term.push_input(if i % 2 == 0 { b"h" } else { b"\x7f" });
            let turn = driver.turn(&mut app, &mut term).expect("key turn");
            assert!(turn.rendered, "a keystroke must repaint");
            frame_bytes.push(term.take_bytes().len());
            sink(turn.emitted);
        },
    );
    let med = median_of(frame_bytes);
    eprintln!("composer keystroke emission: median {med} B/frame");
    assert_budget(&m, Duration::from_millis(3));
}

// ---------------------------------------------------------------------------
// CodeView: diff-tinted scroll (the app-managed scroll idiom).
// ---------------------------------------------------------------------------

/// A 400-line unified diff in a `CodeView::lang("diff")`, scrolled one
/// line per frame through the documented idiom (a scroll signal read
/// by a dyn region that rebuilds the widget with the new offset). The
/// pane re-tints its 38 visible lines per frame — the honest cost of
/// app-managed scrolling — and must stay comfortably interactive.
#[test]
#[ignore]
fn perf_codeview_diff_scroll_100x40() {
    let mut patch =
        String::from("diff --git a/render.rs b/render.rs\n@@ -1,200 +1,200 @@ fn frame()\n");
    for i in 0..400 {
        match i % 5 {
            0 => patch.push_str(&format!("-    let old_{i} = damage.union(rect_{i});\n")),
            1 => patch.push_str(&format!("+    let new_{i} = damage.fold(rect_{i});\n")),
            2 => patch.push_str(&format!("     ctx.present(frame_{i}); // context\n")),
            3 => patch.push_str(&format!("@@ -{i},4 +{i},4 @@ fn pass_{i}()\n")),
            _ => patch.push_str(&format!("     emit(run_{i});\n")),
        }
    }
    let total = CodeView::line_count(&patch) as i32;

    let size = Size::new(100, 40);
    let mut term = CaptureTerm::new(size);
    let mut app = App::new(size);
    let mut wiring = None;
    app.mount(|cx| {
        let offset = cx.signal(0i32);
        wiring = Some(offset);
        let src = patch.clone();
        Element::new()
            .style(LayoutStyle::column())
            .child(text(" patch review — j/k scroll"))
            .child(dyn_view(LayoutStyle::default().grow(1.0), move || {
                let t = current_theme().tokens;
                CodeView::new(src.clone())
                    .lang("diff")
                    .scroll_offset(offset.get())
                    .layout(LayoutStyle::default().grow(1.0))
                    .element(&t)
                    .build()
            }))
            .child(text(" status: reviewing"))
            .build()
    })
    .expect("mount");
    let offset = wiring.expect("offset signal");
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);
    let _ = term.take_bytes();

    let m = time_median("codeview diff scroll 1 line @ 100x40", 4, 9, 20, |i| {
        offset.set((i as i32 + 1) % (total - 40).max(1));
        let turn = driver.turn(&mut app, &mut term).expect("scroll turn");
        assert!(turn.rendered, "a scroll step must repaint");
        term.take_bytes();
        sink(turn.emitted);
    });
    assert_budget(&m, Duration::from_millis(5));
}

// ---------------------------------------------------------------------------
// Scroll guard × parked protocol image: the byte cost of correctness
// (MEDIA study-2 review). While a BYTE-channel image is live the driver
// takes the plain diff — terminals scroll protocol pixels WITH the text
// (the kitty spec mandates it), so a terminal-executed DECSTBM+SU would
// move the placement out from under the session's bookkeeping. This
// measures what the guard COSTS a feed-scroll app with one parked
// image. Byte counts are deterministic and asserted in every profile;
// the re-place-by-id upgrade that would restore the byte win is filed
// as backlog 0675.
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn perf_feed_scroll_with_parked_protocol_image_90x30() {
    let size = Size::new(90, 30);
    let mut term = CaptureTerm::new(size);
    let mut app = App::new(size);
    let mut wiring = None;
    app.mount(|cx| {
        let top = cx.signal(0usize);
        wiring = Some(top);
        dyn_view(LayoutStyle::default().grow(1.0), move || {
            let top = top.get();
            let mut col = Element::new().style(LayoutStyle::column());
            for i in 0..30usize {
                // Lines must be substantially DISTINCT (real transcript
                // rows are): identical tails would let the plain diff
                // skip most of every shifted row and understate the
                // guard's true cost.
                const WORDS: [&str; 8] = [
                    "polling", "render", "commit", "damage", "present", "layout", "effects",
                    "flushes",
                ];
                let n = top + i;
                col = col.child(text(format!(
                    "line {n:05} {} pass over {} queue with {} residue, budget {:04}",
                    WORDS[n % 8],
                    WORDS[(n / 3) % 8],
                    WORDS[(n * 7 + 1) % 8],
                    n * 37 % 9999
                )));
            }
            col.build()
        })
    })
    .expect("mount");
    let top = wiring.expect("top signal");
    let caps = Capabilities::with(|c| {
        c.truecolor = true;
        c.colors_256 = true;
        c.sync_output_2026 = true;
        c.kitty_graphics = true;
    });
    let cfg = RunConfig {
        caps: Some(caps),
        enter: None,
        probe: false,
    };
    let mut driver = Driver::new(&mut app, &mut term, cfg).expect("driver");
    settle(&mut driver, &mut app, &mut term);
    let full_paint = term.take_bytes().len();

    let scroll_frames = |driver: &mut Driver,
                         app: &mut App,
                         term: &mut CaptureTerm,
                         n: usize|
     -> (Vec<usize>, usize, usize) {
        let mut bytes_per_frame = Vec::with_capacity(n);
        let mut shifted = 0usize;
        let mut apc = 0usize;
        for _ in 0..n {
            top.update(|t| *t += 1);
            settle(driver, app, term);
            let bytes = term.take_bytes();
            if bytes.windows(3).any(|w| w == b"\x1b[r") {
                shifted += 1; // emit_shift's DECSTBM reset — the scroll signature
            }
            if bytes.windows(3).any(|w| w == b"\x1b_G") {
                apc += 1;
            }
            bytes_per_frame.push(bytes.len());
        }
        (bytes_per_frame, shifted, apc)
    };

    // Phase 1 — no image: the scroll optimization must engage.
    let (no_image, shifted, apc) = scroll_frames(&mut driver, &mut app, &mut term, 24);
    assert_eq!(
        shifted, 24,
        "phase 1: every scroll frame must use the shift"
    );
    assert_eq!(apc, 0, "phase 1: no image, no APC bytes");

    // Park a kitty image, then measure the same workload again.
    let overlays = app.overlays();
    let _img = overlays.image(
        Rect::new(70, 2, 16, 8),
        abstracttui::gfx::Bitmap::new(32, 24, Rgba::rgb(180, 60, 60)),
    );
    settle(&mut driver, &mut app, &mut term);
    let placed = term.take_bytes();
    assert!(
        placed.windows(3).any(|w| w == b"\x1b_G"),
        "precondition: the image went through the kitty byte channel"
    );

    // Phase 2 — parked byte image: the guard forces the plain diff.
    let (with_image, shifted, apc) = scroll_frames(&mut driver, &mut app, &mut term, 24);
    assert_eq!(
        shifted, 0,
        "phase 2: a live byte-channel image must force the plain diff"
    );
    assert_eq!(
        apc, 0,
        "phase 2: a PARKED image must add zero protocol bytes"
    );

    let p1 = median_of(no_image.clone());
    let p2 = median_of(with_image.clone());
    assert!(
        p2 > p1,
        "the plain diff must cost more than the shift: {p1} vs {p2}"
    );
    eprintln!(
        "scroll-guard cost @ 90x30 log scroll: no image {p1} B/frame (scrolled) | \
         parked kitty image {p2} B/frame (plain diff) | ratio {:.1}x | first paint {full_paint} B",
        p2 as f64 / p1 as f64
    );
}

// ---------------------------------------------------------------------------
// Startup: process spawn -> first painted frame, hello + dashboard.
// ---------------------------------------------------------------------------

/// Time-to-first-frame through a REAL pty: spawn the prebuilt example
/// binary (splash suppressed), poll its output, and take the first
/// instant the modeled screen shows painted content. Debug and release
/// binaries both report; only release asserts (a deliberately generous
/// ceiling — this guards against catastrophic startup regressions, not
/// millisecond drift).
#[cfg(unix)]
#[test]
#[ignore]
fn perf_startup_time_to_first_frame() {
    use abstracttui::testing::pty::spawn_in_pty;
    use abstracttui::testing::VtScreen;
    use std::time::Instant;

    /// Build one profile's examples (skip cleanly on failure — a
    /// non-compiling tree is a transient builder state, live_smoke's
    /// rule).
    fn build(profile_args: &[&str]) -> bool {
        std::process::Command::new(env!("CARGO"))
            .arg("build")
            .args(profile_args)
            .args(["--example", "hello", "--example", "dashboard"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Spawn `bin` under a pty and measure spawn -> first frame (>= 3
    /// non-blank modeled rows). Returns (first_frame_ms, total_bytes).
    fn measure(bin: &str) -> Option<(f64, usize)> {
        if !std::path::Path::new(bin).exists() {
            return None;
        }
        let t0 = Instant::now();
        let mut p = spawn_in_pty(bin, &[], 100, 30, &[("ABSTRACTTUI_NO_SPLASH", "1")]).ok()?;
        let mut first_frame = None;
        let deadline = Duration::from_secs(10);
        while first_frame.is_none() && t0.elapsed() < deadline {
            p.read_for(Duration::from_millis(5));
            if p.captured.windows(8).any(|w| w == b"\x1b[?1049h") {
                let mut vt = VtScreen::new(Size::new(100, 30));
                vt.feed(&p.captured);
                let painted = vt
                    .to_text()
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .count();
                if painted >= 3 {
                    first_frame = Some(t0.elapsed());
                }
            }
        }
        p.send(b"q");
        let _ = p.wait_with_deadline(Duration::from_secs(5));
        first_frame.map(|d| (d.as_secs_f64() * 1000.0, p.captured.len()))
    }

    let debug_ok = build(&[]);
    let release_ok = build(&["--release"]);
    let mut release_frames: Vec<(String, f64)> = Vec::new();
    for (profile, built) in [("debug", debug_ok), ("release", release_ok)] {
        if !built {
            eprintln!("startup {profile}: SKIPPED (profile did not build — transient tree state)");
            continue;
        }
        for name in ["hello", "dashboard"] {
            let bin = format!("target/{profile}/examples/{name}");
            // Two runs: the first exec of a fresh binary pays one-time
            // OS costs (signature validation, cold page cache); the
            // second is the number a user's Nth launch sees.
            let cold = measure(&bin);
            let warm = measure(&bin);
            match (cold, warm) {
                (Some((cold_ms, bytes)), Some((warm_ms, _))) => {
                    eprintln!(
                        "startup {profile} {name}: first frame at {cold_ms:.1} ms cold / \
                         {warm_ms:.1} ms warm ({bytes} B captured)"
                    );
                    if profile == "release" {
                        release_frames.push((name.to_string(), warm_ms));
                    }
                }
                _ => eprintln!("startup {profile} {name}: SKIPPED (binary missing or no frame)"),
            }
        }
    }
    if cfg!(debug_assertions) {
        eprintln!("[debug build, startup ceiling not asserted]");
        return;
    }
    assert!(
        !release_frames.is_empty(),
        "release startup measurement produced no frames"
    );
    for (name, ms) in &release_frames {
        assert!(
            *ms < 1500.0,
            "release {name} took {ms:.1} ms to first frame — catastrophic startup regression"
        );
    }
}
