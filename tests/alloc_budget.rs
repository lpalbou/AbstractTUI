//! Allocation budget tests (REDTEAM, doctrine §4): a counting global
//! allocator installed for THIS test binary only, verifying the vision
//! charter's "no heap allocation in the diff/present hot path at steady
//! state".
//!
//! The `unsafe impl GlobalAlloc` below is the one deliberate exception
//! to the no-unsafe rule outside term FFI: the trait cannot be
//! implemented without the keyword. It is confined to this TEST binary,
//! never the library, and does nothing but forward to `System` and bump
//! relaxed counters — auditable in ten lines.
//!
//! Budget tests run single-threaded within the measured region and
//! assert DELTAS. Counters are PER-THREAD (final-audit hardening): the
//! measured regions never spawn threads, so thread-local attribution is
//! exact — and libtest's OWN harness threads (result printing, test
//! spawning) allocate concurrently under default parallelism, which
//! polluted process-wide counters nondeterministically. Per-thread
//! counting makes the binary green under ANY `--test-threads` value
//! while still catching every real hot-path allocation (they happen on
//! the measuring thread by construction).
//! Run: `cargo test --test alloc_budget` (debug is fine — allocation
//! counts are optimization-independent facts, unlike timings).

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use abstracttui::base::{Rgba, Size};
use abstracttui::render::{Cell as RenderCell, FrameDiff, PresentCaps, Presenter, Style, Surface};
use abstracttui::testing::VtScreen;

// ---------------------------------------------------------------------------
// The counting allocator (design: docs/design/testing.md §4)
// ---------------------------------------------------------------------------

// Const-initialized, no-drop thread locals: access never allocates and
// registers no TLS destructor, so bumping them inside the allocator is
// re-entrancy-safe. `try_with` guards the (theoretical) teardown window.
thread_local! {
    static TL_ALLOCS: Cell<u64> = const { Cell::new(0) };
    static TL_REALLOCS: Cell<u64> = const { Cell::new(0) };
    static TL_BYTES: Cell<u64> = const { Cell::new(0) };
}

struct CountingAlloc;

impl CountingAlloc {
    /// Snapshot the CALLING THREAD's counters.
    fn snapshot(&self) -> (u64, u64, u64) {
        (
            TL_ALLOCS.try_with(Cell::get).unwrap_or(0),
            TL_REALLOCS.try_with(Cell::get).unwrap_or(0),
            TL_BYTES.try_with(Cell::get).unwrap_or(0),
        )
    }
}

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = TL_ALLOCS.try_with(|c| c.set(c.get() + 1));
        let _ = TL_BYTES.try_with(|c| c.set(c.get() + layout.size() as u64));
        System.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout)
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let _ = TL_REALLOCS.try_with(|c| c.set(c.get() + 1));
        let _ =
            TL_BYTES.try_with(|c| c.set(c.get() + new_size.saturating_sub(layout.size()) as u64));
        System.realloc(ptr, layout, new_size)
    }
}

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc;

/// Counters are per-thread (see the module doc), so sibling tests can
/// no longer pollute a measured region — this lock is kept anyway so
/// measured regions run without CPU contention from siblings (doctrine
/// §4: measured regions run single-threaded), keeping the measurements
/// themselves quiet.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serial() -> std::sync::MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|poison| poison.into_inner())
}

/// Measure `f`'s allocation activity: (allocs, reallocs, bytes).
fn alloc_delta(f: impl FnOnce()) -> (u64, u64, u64) {
    let before = ALLOC.snapshot();
    f();
    let after = ALLOC.snapshot();
    (after.0 - before.0, after.1 - before.1, after.2 - before.2)
}

// ---------------------------------------------------------------------------
// Sanity: the counter counts.
// ---------------------------------------------------------------------------

#[test]
fn allocator_counts_and_measures_deltas() {
    let _serial = serial();
    let (a, _, bytes) = alloc_delta(|| {
        let v: Vec<u64> = Vec::with_capacity(100);
        std::hint::black_box(&v);
    });
    assert!(a >= 1, "one Vec allocation must be visible");
    assert!(bytes >= 800, "100 u64s = at least 800 bytes, saw {bytes}");
    let (a2, r2, _) = alloc_delta(|| {
        std::hint::black_box(42u64);
    });
    assert_eq!((a2, r2), (0, 0), "an empty region must measure zero");
}

// ---------------------------------------------------------------------------
// THE budget: diff + present steady state allocates nothing.
// ---------------------------------------------------------------------------

/// Build two full-screen frames that differ in every cell's fg color —
/// the animated-full-redraw shape from the charter budget.
fn styled_frame(size: Size, tick: u8) -> Surface {
    let mut s = Surface::new(size, RenderCell::default());
    for y in 0..size.h {
        let style = Style::new().fg(Rgba::rgb(tick, (y * 4) as u8, 255 - tick));
        // 20 chars x 10 columns of text per row.
        for chunk in 0..(size.w / 20).max(1) {
            s.draw_text(chunk * 20, y, "abcdefghij0123456789", style);
        }
    }
    s
}

/// FINDING RT2-1 (reviews/cycle2/redteam-findings.md): steady-state
/// diff+present measured 3,643 allocs/frame at first filing; RENDER's
/// same-cycle rework brought it to zero. This is now the permanent
/// acceptance test — a regression re-opens the finding.
#[test]
fn diff_present_steady_state_allocates_nothing() {
    let _serial = serial();
    let (d, p) = measure_stages();
    assert_eq!(
        (d.0, d.1),
        (0, 0),
        "steady-state DIFF allocated: {} allocs / {} reallocs / {} bytes",
        d.0,
        d.1,
        d.2
    );
    assert_eq!(
        (p.0, p.1),
        (0, 0),
        "steady-state PRESENT allocated: {} allocs / {} reallocs / {} bytes",
        p.0,
        p.1,
        p.2
    );
}

/// Always-on attribution twin: prints the per-stage allocation profile
/// (the RT2-R2 evidence) and pins only that the numbers never GROW past
/// the observed baseline — a ratchet until the real budget lands.
#[test]
fn diff_present_allocation_attribution_ratchet() {
    let _serial = serial();
    let (d, p) = measure_stages();
    eprintln!(
        "alloc attribution: diff = {} allocs/{} reallocs/{} B; present = {} allocs/{} reallocs/{} B",
        d.0, d.1, d.2, p.0, p.1, p.2
    );
    // Observed at filing time (2026-07-20): diff ~3600 allocs (one per
    // Run pushed? one per row interval?), present ~tens. Ratchet with
    // headroom; shrinking these to zero closes RT2-R2.
    assert!(d.0 <= 8_000, "diff allocation REGRESSED: {} allocs", d.0);
    assert!(p.0 <= 2_000, "present allocation REGRESSED: {} allocs", p.0);
}

fn measure_stages() -> ((u64, u64, u64), (u64, u64, u64)) {
    let size = Size::new(200, 60);
    let caps = PresentCaps::FULL;
    let mut diff = FrameDiff::new();
    let mut presenter = Presenter::new();
    let mut out: Vec<u8> = Vec::new();

    let frames: Vec<Surface> = (0..6).map(|i| styled_frame(size, i * 40)).collect();

    // Warmup: two full cycles populate diff scratch and the byte buffer.
    for w in [0usize, 1, 2] {
        let runs = diff.compute_full(&frames[w], &frames[w + 1]);
        out.clear();
        presenter.emit(runs, &frames[w + 1], &caps, &mut out);
    }

    // Attribution: measure the stages separately on frame 3->4.
    let prev = &frames[3];
    let next = &frames[4];
    let mut d = (0, 0, 0);
    let mut runs_len = 0;
    let d1 = alloc_delta(|| {
        let runs = diff.compute_full(prev, next);
        runs_len = runs.len();
    });
    d.0 += d1.0;
    d.1 += d1.1;
    d.2 += d1.2;
    let runs = diff.compute_full(prev, next);
    out.clear();
    let p = alloc_delta(|| {
        presenter.emit(runs, next, &caps, &mut out);
    });
    assert!(runs_len > 0, "the measured frames really did change");
    (d, p)
}

/// RT2-8 (CLOSED cycle 3): no-change frames allocated ~16/row at filing;
/// RENDER's fix landed and this is now the permanent acceptance test —
/// identical frames cost zero allocations and zero bytes, forever.
#[test]
fn presenter_no_change_frame_emits_and_allocates_nothing() {
    let _serial = serial();
    let size = Size::new(80, 24);
    let frame = styled_frame(size, 7);
    let caps = PresentCaps::FULL;
    let mut diff = FrameDiff::new();
    let mut presenter = Presenter::new();
    let mut out: Vec<u8> = Vec::new();

    // Warm.
    let runs = diff.compute_full(&frame, &frame);
    presenter.emit(runs, &frame, &caps, &mut out);
    out.clear();

    let (allocs, reallocs, _) = alloc_delta(|| {
        let runs = diff.compute_full(&frame, &frame);
        presenter.emit(runs, &frame, &caps, &mut out);
    });
    assert_eq!((allocs, reallocs), (0, 0), "identical frames must be free");
    assert!(out.is_empty(), "identical frames must emit zero bytes");
}

// ---------------------------------------------------------------------------
// Companion guard: the VT model itself is cheap enough to referee with
// (its feed path may allocate for glyph strings — measured, bounded).
// ---------------------------------------------------------------------------

#[test]
fn vt_model_feed_allocation_is_bounded() {
    let _serial = serial();
    let mut screen = VtScreen::new(Size::new(200, 60));
    let mut frame_bytes = Vec::new();
    for y in 1..=60 {
        frame_bytes.extend_from_slice(format!("\x1b[{y};1H\x1b[38;2;1;2;3m").as_bytes());
        frame_bytes.extend_from_slice("x".repeat(200).as_bytes());
    }
    screen.feed(&frame_bytes); // warm the grid's cell strings
    let (allocs, _, bytes) = alloc_delta(|| {
        screen.feed(&frame_bytes);
    });
    // The model is allowed to allocate (String per printed cell today),
    // but a full 200x60 frame re-feed must stay under ~2 allocations per
    // cell — a regression here would make property tests dominate CI.
    let cells = 200 * 60;
    assert!(
        allocs <= 2 * cells,
        "VT model allocation blew up: {allocs} allocs / {bytes} bytes for {cells} cells"
    );
}

// ---------------------------------------------------------------------------
// JPEG decode: a hostile input must never trigger absurd allocation. The
// decoder's pixel guard is supposed to fire BEFORE any plane/bitmap Vec
// is sized from attacker-controlled dimensions. We assert the guard path
// allocates a bounded, tiny amount (the marker walk's small parses), not
// gigabytes for a claimed 65535x65535 image.
// ---------------------------------------------------------------------------

#[test]
fn jpeg_dimension_bomb_allocates_within_budget() {
    let _serial = serial();
    use abstracttui::gfx::jpeg;
    use abstracttui::testing::jpeg_build::FlatJpeg;

    // A valid flat JPEG, then patch its SOF dims to 65535x65535 — 4.29 G
    // pixels, ~17 GB of RGBA if the guard ever failed to fire.
    let mut bytes = FlatJpeg::grayscale(16, 16).build();
    let sof = bytes
        .windows(2)
        .position(|w| w[0] == 0xFF && (w[1] == 0xC0 || w[1] == 0xC1))
        .expect("SOF present");
    // SOF body: len(2) precision(1) h(2) w(2); patch h and w to 0xFFFF.
    for i in 0..4 {
        bytes[sof + 5 + i] = 0xFF;
    }

    let (allocs, _, alloc_bytes) = alloc_delta(|| {
        let r = jpeg::decode(&bytes);
        assert!(r.is_err(), "dimension bomb must be rejected");
        std::hint::black_box(&r);
    });
    // The rejection path parses a couple of small segments and formats an
    // error string; a few KB at most. Anything in the megabytes means the
    // guard fired AFTER a plane allocation.
    assert!(
        alloc_bytes < 64 * 1024,
        "dimension-bomb rejection allocated {alloc_bytes} bytes in {allocs} allocs — guard fired too late"
    );
}

#[test]
fn gltf_animation_sampling_is_allocation_free_per_frame() {
    let _serial = serial();
    use abstracttui::three::animation::{Animation, Interpolation, NodePose, Track, TrackValues};
    use abstracttui::three::Vec3;

    // A multi-track animation (translation + rotation + scale over 8
    // keys) driving 4 nodes — the per-frame work a playing model does.
    let mut tracks = Vec::new();
    for node in 0..4 {
        let times: Vec<f32> = (0..8).map(|k| k as f32 * 0.5).collect();
        tracks.push(Track {
            node,
            times: times.clone(),
            values: TrackValues::Translation(
                (0..8).map(|k| [k as f32, node as f32, 0.0]).collect(),
            ),
            interpolation: Interpolation::Linear,
        });
        tracks.push(Track {
            node,
            times: times.clone(),
            values: TrackValues::Rotation((0..8).map(|_| [0.0, 0.0, 0.0, 1.0]).collect()),
            interpolation: Interpolation::Linear,
        });
    }
    let anim = Animation::new(None, tracks);
    let rest = NodePose {
        translation: Vec3::ZERO,
        rotation: [0.0, 0.0, 0.0, 1.0],
        scale: Vec3::new(1.0, 1.0, 1.0),
    };
    let mut poses = vec![rest; 4]; // pre-grown output scratch

    // Warm (any lazy init happens here).
    anim.sample(1.0, &mut poses);

    // Steady state: sampling at arbitrary times must touch ZERO heap.
    let (allocs, reallocs, _) = alloc_delta(|| {
        for i in 0..240 {
            let t = (i as f32) * 0.01;
            anim.sample(t, &mut poses);
        }
    });
    assert_eq!(
        (allocs, reallocs),
        (0, 0),
        "animation sampling allocated on the hot path: {allocs} allocs, {reallocs} reallocs over 240 frames"
    );
}

// ---------------------------------------------------------------------------
// Idle honesty for the 0.2.x app surfaces: a mounted Feed (streaming
// item open), an ARMED interval (not yet due), a PARKED Select popup,
// and a PARKED byte-channel image (study-2 image review) — the
// always-mounted shapes of a modern transcript app — must cost literal
// zero on idle turns: zero bytes, zero allocations, zero reallocations
// on the UI thread. The byte half is pinned elsewhere (adv_app,
// wave_livedata, adv_selection, adv_image_lifecycle); this is the
// allocation half, re-verified on the CURRENT tree with the new
// widgets in play. The parked kitty placement pins that a terminal-held
// image costs nothing while nothing changes: `Driver::pre_image_pass`
// never runs on idle turns (no frame), and a rendered frame with a
// clean placement early-outs it allocation-free.
// ---------------------------------------------------------------------------

#[test]
fn idle_turns_with_feed_interval_parked_popup_and_parked_image_allocate_nothing() {
    use abstracttui::app::{App, Driver, RunConfig};
    use abstracttui::prelude::*;
    use abstracttui::reactive::interval;
    use abstracttui::testing::CaptureTerm;
    use abstracttui::ui::text;
    use abstracttui::widgets::{Feed, FeedItem, FeedState};
    use std::time::Duration;

    let _serial = serial();
    let size = Size::new(60, 16);
    let mut term = CaptureTerm::new(size);
    let mut app = App::new(size);
    let start = std::time::Instant::now();
    app.mount(|cx| {
        // A feed with history and an OPEN streaming item (live but quiet).
        let feed = FeedState::new(cx);
        for i in 0..8 {
            feed.push(
                format!("h{i}"),
                FeedItem::markdown(format!("**msg {i}** body")),
            );
        }
        feed.push_stream("live");
        feed.stream_append("live", "streaming answer paused mid-");
        // An armed interval: bounds the SLEEP, never the frames.
        let ticks = cx.signal(0u32);
        interval(cx, Duration::from_secs(3600), move || {
            ticks.update(|t| *t += 1);
        });
        let follow = cx.signal(true);
        Element::new()
            .style(LayoutStyle::column())
            .child(
                Select::new(vec![
                    SelectOption::new("stable"),
                    SelectOption::new("beta"),
                    SelectOption::new("nightly"),
                ])
                .layout(LayoutStyle::default().w(20).h(1).shrink(0.0))
                .view(cx),
            )
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
            .child(text(" status"))
            .build()
    })
    .expect("mount");
    let cfg = RunConfig {
        caps: Some(abstracttui::term::Capabilities::with(|c| {
            c.truecolor = true;
            c.colors_256 = true;
            // Byte channel for the parked image below: the placement
            // must live in TERMINAL state (kitty), not the cell model.
            c.kitty_graphics = true;
        })),
        enter: None,
        probe: false,
    };
    let mut driver = Driver::new(&mut app, &mut term, cfg).expect("driver");
    // Injected clock, frozen: the interval stays armed-but-not-due for
    // every measured turn.
    let now = std::rc::Rc::new(std::cell::Cell::new(start));
    let clock = now.clone();
    driver.set_clock(move || clock.get());
    // Park a protocol image (top-right, off the popup): transmitted
    // once during setup, then held by the terminal.
    let overlays = app.overlays();
    let _img = overlays.image(
        Rect::new(44, 2, 12, 6),
        abstracttui::gfx::Bitmap::new(16, 12, Rgba::rgb(200, 40, 40)),
    );
    // Settle the mount, focus the trigger (Tab: first focusable), then
    // park the Select popup open (Enter) and settle again.
    for _ in 0..64 {
        if driver.turn(&mut app, &mut term).expect("turn").idle {
            break;
        }
    }
    term.push_input(b"\t\r");
    for _ in 0..64 {
        if driver.turn(&mut app, &mut term).expect("turn").idle {
            break;
        }
    }
    assert!(
        term.screen().to_text().contains("nightly"),
        "precondition: the popup is open and parked:\n{}",
        term.screen().to_text()
    );
    let setup_bytes = term.take_bytes();
    assert!(
        setup_bytes.windows(3).any(|w| w == b"\x1b_G"),
        "precondition: the image went through the kitty byte channel"
    );

    // 16 idle turns: not one byte, not one allocation.
    let (allocs, reallocs, bytes) = alloc_delta(|| {
        for _ in 0..16 {
            let turn = driver.turn(&mut app, &mut term).expect("idle turn");
            assert!(turn.idle, "turn must report idle");
            assert!(!turn.rendered, "idle turn rendered");
        }
    });
    assert_eq!(
        (allocs, reallocs),
        (0, 0),
        "idle turns allocated with the new mounts parked: \
         {allocs} allocs / {reallocs} reallocs / {bytes} B over 16 turns"
    );
    assert!(term.bytes().is_empty(), "idle turns wrote bytes");
}

#[test]
fn jpeg_hostile_corpus_allocation_is_bounded() {
    let _serial = serial();
    use abstracttui::gfx::jpeg;
    use abstracttui::testing::jpeg_build::FlatJpeg;

    // A batch of small-but-pathological inputs (deep trees, dangling
    // refs, truncations). None declares large dimensions, so total
    // allocation across the batch must stay modest — no hidden
    // amplification from a mutated count or table.
    let base = FlatJpeg::grayscale(16, 16).with_flat_code_len(16).build();
    let (allocs, _, alloc_bytes) = alloc_delta(|| {
        for cut in (2..base.len()).step_by(3) {
            let _ = std::hint::black_box(jpeg::decode(&base[..cut]));
        }
    });
    // ~ base.len()/3 decode attempts, each parsing a 16x16 frame at most.
    let attempts = (base.len() - 2).div_ceil(3);
    assert!(
        alloc_bytes < attempts as u64 * 128 * 1024,
        "hostile-corpus decode allocated {alloc_bytes} bytes over {attempts} attempts ({allocs} allocs)"
    );
}
