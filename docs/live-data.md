# Live data: background sources into the UI

How a networked, long-lived app gets data from a background thread into
signals — the ownership rule, the named bindings, bounded back-pressure,
and the recurring time source. The runnable companion is
[`examples/feed.rs`](../examples/feed.rs).

## The ownership rule (one sentence)

The reactive graph is single-threaded: background threads never touch
signals — the only sanctioned crossing is a closure posted to the UI
thread, and writing a signal from the wrong thread is a **named panic**
(it tells you to use this page's pattern), never silent aliasing.

Under the hood that crossing is `reactive::WakeHandle::post(f)`: the
closure crosses the thread boundary, wakes the event loop, and runs in
the next frame's phase U with full runtime access. Two guarantees come
with it:

- **Ordered delivery** — one producer's posts apply in emit order
  (FIFO; cross-producer order is lock-acquisition order).
- **One frame per burst** — any number of posts between two frames
  coalesce into one wake and one repaint; a post landing mid-frame
  applies in the next frame, exactly once.

And one cost rule: while a source is quiet, the app is byte-for-byte
idle — no polling, no timers, the loop parks in a blocking read.

## The bindings

You rarely post by hand. Three named helpers in `reactive::` cover the
shapes (all senders are `Clone + Send`; all signals die with the scope
that created them, after which senders turn **inert** — sends apply
nothing, count on `dead_sends()`, and are never unsafe):

| helper | signal shape | delivery | use for |
| --- | --- | --- | --- |
| `channel_source(cx)` | `Signal<Vec<T>>` | every value, in order, unbounded | low-rate event streams |
| `latest_source(cx, initial)` | `Signal<T>` | newest value; intermediates coalesce at the source | progress, telemetry, presence |
| `bounded_source(cx, capacity, policy)` | `Signal<Vec<T>>` window + `Signal<IngestStats>` | at most `capacity` retained; overflow per policy, counted | anything that can flood |

## Bounded ingestion and back-pressure honesty

`WakeHandle::post` is the **control lane**: unbounded by contract,
correct for low-rate messages. A flooding producer (chat hub, tool
output, tail -f) needs the **data lane**:

```rust
let (tx, events, stats) = bounded_source::<String>(
    cx,
    400,                        // the retained window, and the bound
    OverflowPolicy::DropOldest, // what overflow MEANS is the app's call
);
```

- `DropOldest` — ring: the newest tail survives (feeds, logs).
- `DropNewest` — the head survives (capture the first N).
- `OverflowPolicy::coalesce(fold)` — overflow merges into the newest
  survivor (progress updates that supersede each other).
- **There is no `Block`.** Blocking a producer against the UI thread
  inverts liveness: the producer inherits every UI stall (a held
  scrollbar, a suspended terminal, a modal) as unbounded latency on its
  own sockets and locks, and can no longer answer the cancellation the
  UI is about to send it. Producers that must not lose data pause their
  *reads* upstream (the transport pushes back); they never park on the
  UI.

Honesty is part of the contract: every dropped or coalesced value is
counted in `stats` (`IngestStats { delivered, dropped, coalesced }`),
updated atomically with the window. **Render `dropped` when it is
nonzero** — "1.2k shown · 34 dropped" is the labeled-degradation
convention; silent loss is the failure mode this lane exists to
prevent. Memory is bounded by construction (≤ 2×capacity across the
transit buffer and the window) and a burst costs one wake, one posted
drain, one frame — no matter how many values arrive.

Producer-side guidance: drain everything available per read and send
per item into the bounded lane (it batches internally), or batch into
few `post` closures on the raw lane. One closure per burst is the
intended cadence for high-rate sources.

## The recurring time source

Time is the zeroth data source. `reactive::interval` is the engine-owned
version of the self-rescheduling `after(..)` recursion, with the
cancellation story the recursion never has:

```rust
let handle = interval(cx, Duration::from_secs(1), move || {
    now.set(clock_text()); // runs on the UI thread, phase U
});
// handle.cancel() stops it early; scope disposal cancels it anyway —
// a closed pane's poller cannot keep ticking by accident.
```

Fixed-delay drift policy: the next deadline is *fire time + period*.
After a suspend of N periods it fires **once** and resumes cadence —
missed ticks coalesce, there are no catch-up storms. Between fires an
armed interval costs zero wakeups (the loop sleeps until the deadline);
timers never frame-pace.

## Worker lifecycle

Spawn producers with `reactive::spawn_worker(label, f)`: a worker
**panic** is posted back and surfaces as a labeled app error (`Driver`
turns it into `Err`), instead of a thread dying silently while the feed
just... stops. A clean return is not an error. Give workers a stop flag
and join them after `App::run` returns (the feed example's teardown),
so no thread outlives the terminal session.

## Copy-paste starting point

```rust
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use abstracttui::prelude::*;
use abstracttui::reactive::{bounded_source, spawn_worker, OverflowPolicy};

fn main() -> abstracttui::base::Result<()> {
    if !abstracttui::term::have_tty() {
        return Ok(());
    }
    let stop = Arc::new(AtomicBool::new(false));
    let mut app = App::new(Size::new(80, 24));
    let mut sender = None;
    app.mount(|cx| {
        let (tx, events, stats) =
            bounded_source::<String>(cx, 400, OverflowPolicy::DropOldest);
        sender = Some(tx);
        Element::new()
            .style(LayoutStyle::column())
            .child(dyn_view(LayoutStyle::default().grow(1.0), move || {
                let rows = events.get();
                let mut col = Element::new().style(LayoutStyle::column());
                for line in rows.iter().rev().take(20).rev() {
                    col = col.child(text(line.clone()));
                }
                col.build()
            }))
            .child(dyn_view(LayoutStyle::line(1), move || {
                let s = stats.get();
                text(match s.dropped {
                    0 => format!("{} events", s.delivered),
                    d => format!("{} events · {d} dropped", s.delivered),
                })
            }))
            .build()
    })?;
    let tx = sender.take().expect("mounted");
    let stop_w = stop.clone();
    let worker = spawn_worker("my-source", move || {
        while !stop_w.load(Ordering::Relaxed) {
            // read your socket/process/channel here, then:
            tx.send("hello from the background".to_string());
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    });
    let result = app.run();
    stop.store(true, Ordering::Relaxed);
    worker.join().ok();
    result
}
```

Swap the sleep for a real read loop and this is a networked app: the
transport (HTTP poll, WebSocket, subprocess pipe) is your choice — the
engine's job ends at the thread boundary, and this page is that
boundary's contract.

## Testing live-data apps

The headless harness works unchanged: drive `Driver::turn` against
`testing::CaptureTerm`, send from a joined thread between turns, and
assert on the rendered screen — one frame per burst, zero bytes while
quiet. `tests/wave_livedata.rs` pins exactly those claims and is a
gallery of the shapes.
