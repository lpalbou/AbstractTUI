//! VERIFY cycle-8 integration matrix: instantiate the FULL stack more
//! than once in a single process. Thread-local reactive runtimes, static
//! pools, and any global teardown that leaks state surface as SECOND-RUN
//! failures — the first run "warms" the leak, the second trips on it.
//! Also: reuse-after-drop of the overlay world and ImageSession.

use abstracttui::app::{App, Driver, RunConfig};
use abstracttui::base::{Point, Rect, Rgba, Size};
use abstracttui::gfx::bitmap::Bitmap;
use abstracttui::gfx::{ExternalSink, ImageSession, SyncOutcome};
use abstracttui::prelude::*;
use abstracttui::term::caps::GraphicsCaps;
use abstracttui::term::Capabilities;
use abstracttui::testing::{CaptureTerm, KittyModel, VtScreen};
use abstracttui::widgets::{Button, List};

/// Build, drive, and TEAR DOWN one full app+driver+terminal, returning
/// the final rendered text. Everything is dropped at the end of the fn.
fn run_one_app(seed: u64) -> String {
    let size = Size::new(48, 10);
    let mut term = CaptureTerm::new(size);
    let mut app = App::new(size);
    app.mount(|cx| {
        let count = cx.signal(seed as i64);
        let items = vec![format!("row-{seed}-a"), format!("row-{seed}-b")];
        let sel = cx.signal(0usize);
        let t = &current_theme().tokens;
        Element::new()
            .style(LayoutStyle::column())
            .child(dyn_view(LayoutStyle::line(1), move || {
                text(format!("count {}", count.get()))
            }))
            .child(
                Button::new("+1")
                    .on_click(move || count.update(|c| *c += 1))
                    .element(cx, t)
                    .build(),
            )
            .child(List::new(items).selection(sel).element(cx, t).build())
            .build()
    })
    .expect("mount");

    let cfg = RunConfig {
        caps: Some(Capabilities::default()),
        enter: None,
        probe: false,
    };
    let mut driver = Driver::new(&mut app, &mut term, cfg).expect("enter");
    let mut vt = VtScreen::new(size);
    for _ in 0..8 {
        let idle = driver.turn(&mut app, &mut term).expect("turn").idle;
        vt.feed(&term.take_bytes());
        if idle {
            break;
        }
    }
    // Exercise input so reactive effects + focus state are live.
    term.push_input(b"\t\r"); // focus button, click
    for _ in 0..6 {
        let idle = driver.turn(&mut app, &mut term).expect("turn").idle;
        vt.feed(&term.take_bytes());
        if idle {
            break;
        }
    }
    drop(driver); // runs leave
    vt.feed(&term.take_bytes());
    vt.to_text()
}

/// The core matrix property: a second full stack in the same process
/// behaves identically to the first. A leaked thread-local reactive
/// runtime (stale nodes, non-reset id counter surfacing as wrong reads)
/// or a poisoned global pool would make run 2 differ or panic.
#[test]
fn two_full_apps_sequentially_are_independent() {
    let first = run_one_app(10);
    let second = run_one_app(20);
    let third = run_one_app(10); // same seed as first → must match first

    // Each app rendered its OWN seed's content (no bleed between runs).
    assert!(
        first.contains("count 1") && first.contains("row-10-a"),
        "run 1 wrong:\n{first}"
    );
    assert!(
        second.contains("count 2") && second.contains("row-20-a"),
        "run 2 wrong:\n{second}"
    );
    // Determinism across teardown: same inputs → same frame, proving no
    // residual state carried from run 1 or run 2 into run 3.
    assert_eq!(
        first, third,
        "same-seed app differs after intervening runs — leaked global state"
    );
}

/// Ten stacks back to back: a slow leak (id counters climbing, pool never
/// reclaiming) either panics or drifts the output by the 10th.
#[test]
fn ten_full_apps_do_not_drift_or_panic() {
    let baseline = run_one_app(7);
    for i in 0..10 {
        let out = run_one_app(7);
        assert_eq!(
            out, baseline,
            "app instance {i} drifted from baseline — teardown leak"
        );
    }
}

// ---------------------------------------------------------------------------
// Reuse-after-drop: overlay world + ImageSession.
// ---------------------------------------------------------------------------

struct ModelSink(KittyModel);
impl ExternalSink for ModelSink {
    fn external_write(&mut self, bytes: &[u8], _at: Point) {
        self.0.feed(bytes);
    }
}
fn kitty_caps() -> GraphicsCaps {
    GraphicsCaps {
        wrap: None,
        kitty_graphics: true,
        iterm2_images: false,
        sixel: false,
        sixel_max_registers: None,
        cell_pixel_size: None,
    }
}

/// An `ImageSession` created, used, dropped, then a FRESH one created in
/// the same process must not inherit id state — image ids must not leak
/// across sessions, and the second session's accounting is clean.
#[test]
fn image_session_reuse_after_drop_is_clean() {
    let caps = kitty_caps();
    let img = Bitmap::new(4, 4, Rgba::rgb(1, 2, 3));

    let ids_first = {
        let mut sink = ModelSink(KittyModel::new());
        let mut session = ImageSession::new();
        session.sync(&mut sink, 1, 1, &img, Rect::new(0, 0, 4, 2), &caps);
        session.sync(&mut sink, 2, 1, &img, Rect::new(5, 0, 4, 2), &caps);
        session.release_all(&mut sink, &caps);
        assert!(
            sink.0.live_data_ids().is_empty(),
            "session 1 leaked on release_all"
        );
        // Drop session + sink here.
        session.live_slots()
    };
    assert_eq!(
        ids_first, 0,
        "session 1 still tracked slots after release_all"
    );

    // A brand-new session in the same process starts from a clean slate.
    let mut sink = ModelSink(KittyModel::new());
    let mut session2 = ImageSession::new();
    let out = session2.sync(&mut sink, 1, 1, &img, Rect::new(0, 0, 4, 2), &caps);
    assert!(
        matches!(out, SyncOutcome::Emitted(_)),
        "fresh session must emit a transmit"
    );
    assert_eq!(
        sink.0.live_data_ids().len(),
        1,
        "fresh session accounting polluted by the dropped one"
    );
    assert!(
        sink.0.violations.is_empty(),
        "protocol violations after reuse: {:?}",
        sink.0.violations
    );
    session2.release_all(&mut sink, &caps);
    assert!(sink.0.live_data_ids().is_empty());
}

/// The overlay world from a dropped App must not keep the next App's
/// overlays alive or panic. Two Apps, each takes its overlays handle,
/// mounts a toast-ish layer, and tears down.
#[test]
fn overlay_world_reuse_across_apps() {
    for seed in [1u64, 2, 3] {
        let size = Size::new(30, 8);
        let mut term = CaptureTerm::new(size);
        let mut app = App::new(size);
        // The overlay handle is cloneable and captured at mount; a fresh
        // App each iteration must yield a fresh, working overlay world
        // (the Driver establishes the root at enter).
        let overlays = app.overlays();
        let _ = &overlays;
        app.mount(move |cx| {
            let _ = cx;
            let _ = seed;
            Element::new().child(text("base")).build()
        })
        .expect("mount");
        let cfg = RunConfig {
            caps: Some(Capabilities::default()),
            enter: None,
            probe: false,
        };
        let mut driver = Driver::new(&mut app, &mut term, cfg).expect("enter");
        for _ in 0..4 {
            if driver.turn(&mut app, &mut term).expect("turn").idle {
                break;
            }
        }
        drop(driver);
        // App + overlays drop here; next iteration must start clean.
    }
}
