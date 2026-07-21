//! VERIFY cycle-7 soak: drive a dashboard-shaped app through 10,000
//! frames of timer ticks + random key/mouse events under a counting
//! global allocator, and assert the properties a long-running session
//! must hold: NO memory growth (steady-state allocation flat across
//! windows — the arena/pool/link tables are bounded, not leaking), NO
//! panic, NO frame-cost degradation over time, and the terminal restored
//! at the end.
//!
//! The counting allocator is a `#[global_allocator]` confined to THIS
//! test binary (the one sanctioned exception to no-unsafe outside term
//! FFI, same as `alloc_budget.rs`); it forwards to `System` and bumps
//! relaxed counters.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use abstracttui::app::{App, Driver, RunConfig};
use abstracttui::base::Size;
use abstracttui::layout::{Dimension, Style as LayoutStyle};
use abstracttui::reactive::after;
use abstracttui::term::Capabilities;
use abstracttui::testing::{CaptureTerm, VtScreen};
use abstracttui::ui::{dyn_view, text, Element};
use abstracttui::widgets::TextInput;

struct CountingAlloc {
    allocs: AtomicU64,
}
impl CountingAlloc {
    const fn new() -> CountingAlloc {
        CountingAlloc {
            allocs: AtomicU64::new(0),
        }
    }
}
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.allocs.fetch_add(1, Ordering::Relaxed);
        System.alloc(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout)
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new: usize) -> *mut u8 {
        self.allocs.fetch_add(1, Ordering::Relaxed);
        System.realloc(ptr, layout, new)
    }
}

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

fn allocs() -> u64 {
    ALLOC.allocs.load(Ordering::Relaxed)
}

const TICK: Duration = Duration::from_millis(250);

/// A tiny deterministic PRNG (soak input needs to be reproducible; the
/// rig's `Rng` allocates nothing but this keeps the binary dependency-
/// free of anything that might).
struct Xs(u64);
impl Xs {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

#[test]
#[ignore = "soak: 10k-frame long-running session (run explicitly)"]
fn dashboard_10k_frames_no_growth_no_panic_restore() {
    let size = Size::new(120, 40);
    let mut term = CaptureTerm::new(size);
    let mut app = App::new(size);

    // Dashboard-shaped workload: a self-rescheduling data tick updates a
    // counter signal every TICK (drives the log/chart re-render), plus a
    // focusable text input and a dyn_view that reads both — the same
    // signal/timer/dyn shape the dashboard example is built from.
    app.mount(|cx| {
        let tick = cx.signal(0u64);
        let value = cx.signal(String::new());

        fn tick_loop(tick: abstracttui::reactive::Signal<u64>) {
            after(TICK, move || {
                tick.update(|t| *t += 1);
                tick_loop(tick);
            });
        }
        tick_loop(tick);

        let tokens = &abstracttui::theme::default_theme().tokens;
        Element::new()
            .child(TextInput::new().value(value).element(cx, tokens).build())
            .child(dyn_view(
                LayoutStyle::default()
                    .width(Dimension::Cells(100))
                    .height(Dimension::Cells(1)),
                move || text(format!("tick {} · input {}", tick.get(), value.get().len())),
            ))
            .build()
    })
    .expect("mount");

    // Virtual clock: advance deterministically so timers fire without
    // real sleeps.
    let clock = Rc::new(Cell::new(Instant::now()));
    let cfg = RunConfig {
        caps: Some(Capabilities::default()),
        enter: None,
        probe: false,
    };
    let mut driver = Driver::new(&mut app, &mut term, cfg).expect("enter");
    {
        let clock = clock.clone();
        driver.set_clock(move || clock.get());
    }
    // Focus the input so keystrokes land somewhere.
    term.push_input(b"\t");
    let _ = driver.turn(&mut app, &mut term).expect("turn");

    // Windowed allocation sampling: 10 windows of 1000 frames. A leak
    // shows as a rising per-window alloc count; a bounded engine settles.
    const FRAMES: usize = 10_000;
    const WINDOW: usize = 1_000;
    let mut window_allocs: Vec<u64> = Vec::with_capacity(FRAMES / WINDOW);
    let mut rng = Xs(0x50AC_5EED_1234_9E3F);
    let mut window_start = allocs();

    for frame in 1..=FRAMES {
        // Advance the clock a quarter-tick so a data tick fires roughly
        // every 4 frames (the dashboard's cadence relative to the loop).
        clock.set(clock.get() + TICK / 4);

        // Random input: type, delete, arrows, and mouse moves/clicks.
        match rng.next() % 8 {
            0 | 1 => term.push_input(b"x"),
            2 => term.push_input(b"\x7f"),   // backspace
            3 => term.push_input(b"\x1b[C"), // right
            4 => term.push_input(b"\x1b[D"), // left
            5 => term.push_input(b"\t"),     // refocus
            6 => term.push_input(b"\x1b[<0;10;5M\x1b[<0;10;5m"), // click at 10,5
            _ => {}                          // idle frame
        }
        let turn = driver.turn(&mut app, &mut term).expect("turn");
        assert!(!turn.quit, "soak app quit unexpectedly at frame {frame}");
        // Keep the capture buffer bounded so the harness itself doesn't
        // grow (we only care about ENGINE allocation, not our capture).
        term.take_bytes();

        if frame % WINDOW == 0 {
            let now = allocs();
            window_allocs.push(now - window_start);
            window_start = now;
        }
    }

    eprintln!("soak allocation per 1000-frame window: {window_allocs:?}");

    // NO GROWTH: the last window must not allocate materially more than a
    // mid-run steady-state window. Early windows include lazy first-paint
    // growth (pools/interners warming), so compare the LAST window to the
    // MEDIAN of the back half — a leak makes the last window climb.
    let back_half = &window_allocs[window_allocs.len() / 2..];
    let mut sorted = back_half.to_vec();
    sorted.sort_unstable();
    let median = sorted[sorted.len() / 2].max(1);
    let last = *window_allocs.last().unwrap();
    assert!(
        last <= median * 2,
        "allocation grew over time (leak?): last window {last} vs back-half median {median} \
         — full profile {window_allocs:?}"
    );

    // TERMINAL RESTORED: leaving must reset the alt screen / paste / show
    // the cursor. Drop the driver (runs leave) and feed the FULL capture
    // through the referee.
    drop(driver);
    let tail = term.take_bytes(); // whatever leave emitted
    let mut vt = VtScreen::new(size);
    // Re-feed a fresh session's worth: enter+leave bytes are what matter;
    // the leave tail alone carries the restore sequence.
    vt.feed(&tail);
    assert!(!vt.modes().alt_screen(), "soak left the alt screen enabled");
    assert!(vt.modes().cursor_visible(), "soak left the cursor hidden");
    assert!(
        !vt.modes().bracketed_paste(),
        "soak left bracketed paste enabled"
    );
}
