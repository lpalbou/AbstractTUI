//! dashboard — the flagship demo: a live operations board.
//!
//! Demonstrates: composed layout at density (header/sidebar/chart grid/
//! log tail/sortable table), braille line charts + sparklines + ramped
//! progress from the chart tokens, `reactive::interval` timers driving
//! a clock and deterministic data walks (damage economy: each
//! panel is its own Dyn — watch the log tick without the chart
//! repainting), Toast + focus-trapped Modal overlays, live theme cycling
//! through the one theme signal, real focus traversal.
//!
//! Keys: Tab focus · ↑↓ list/table · s sort · n toast · b brandmark ·
//! ? help · Ctrl+T theme · q quit. Gorgeous at 120x35, graceful at
//! 80x24, guarded below 40x10.
//!
//! The `b` flourish spins the three-planes mark in a mini `Viewport3D`
//! (truecolor terminals only). It rides the EXISTING 250 ms data tick —
//! zero additional timers — and is absent from the tree when off, so
//! the idle log-tick damage economy is byte-identical with the panel
//! closed.
//!
//! OWNER: DESIGN.

#[path = "../common/mod.rs"]
mod common;
mod data;
use data::*;

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use abstracttui::app::current_viewport;
use abstracttui::prelude::*;
use abstracttui::reactive::after;
use abstracttui::theme::themes;
use abstracttui::ui::Canvas;
use abstracttui::widgets::{ColWidth, Column, LineChart, Sparkline, TimeSeriesState, TitleAlign};

/// Data cadence: four ticks per second.
const TICK: Duration = Duration::from_millis(250);
/// History window for charts (samples).
const WINDOW: usize = 72;

fn main() -> abstracttui::base::Result<()> {
    // Diagnostic surface: `--caps` prints the capability report and
    // exits — works everywhere, no tty required.
    if std::env::args().any(|a| a == "--caps") {
        println!(
            "{}",
            abstracttui::term::Capabilities::detect_env().summary()
        );
        return Ok(());
    }
    if !abstracttui::term::have_tty() {
        println!("dashboard: needs an interactive terminal — skipping cleanly");
        return Ok(());
    }
    // The 3D flourish needs the color depth to sell the ramp; 256/16
    // stays chart-only (the toggle explains itself via toast).
    let mark_allowed = abstracttui::term::Capabilities::detect_env().truecolor;
    let mark_model = std::sync::Arc::new(brandmark_model());
    // Capture/demo determinism: an explicit start theme beats keypress
    // choreography in screenshot harnesses (docs cycle).
    if let Ok(id) = std::env::var("ABSTRACTTUI_START_THEME") {
        if !abstracttui::app::set_theme_by_id(&id) {
            eprintln!("dashboard: unknown ABSTRACTTUI_START_THEME {id:?} — using default");
        }
    }
    let mut app = App::new(Size::new(120, 35));
    if !mark_allowed {
        app.push_startup_notice("3d mark: off (needs truecolor — b explains)");
    }
    let quitter = app.quitter();
    let overlays = app.overlays();

    app.mount(move |cx| {
        let theme = use_theme(cx);
        // Durable state: mount-scoped, survives theme rebuilds.
        let tick = cx.signal(0u64);
        let clock = cx.signal(clock_text());
        let nav = cx.signal(0usize);
        let session = cx.signal(0usize);
        let sort = cx.signal((2usize, false)); // rx column, descending
        let toasts = cx.signal(0u32);
        let theme_ix = cx.signal(0usize);
        let show_mark = cx.signal(false);
        // Keyboard-ownership signals: the panes' focus rings follow the
        // widgets' REAL focus (§3.2's composition rule, wired via the
        // cycle-5 `focus_signal` plumbing).
        let nav_focus = cx.signal(false);
        let table_focus = cx.signal(false);
        // The viewport as a signal (cycle-5 `use_viewport`) — overlays
        // place against the live size, resize included.
        let viewport = use_viewport(cx);
        let help: Rc<RefCell<Option<Modal>>> = Rc::new(RefCell::new(None));

        // Traffic history rides the engine's ring (backlog 0190): the
        // hand-rolled WINDOW walk is gone — the tick pushes one sample
        // and `TimeSeriesState` owns retention, gap padding and the
        // axis span. Seeded with the deterministic back-history so the
        // first frame shows a full window (same still the old walk
        // produced); timestamps derive from the tick count, never wall
        // clocks, so the demo stays capture-friendly.
        let rx_hist = TimeSeriesState::new(cx, TICK, TICK * WINDOW as u32);
        let tx_hist = TimeSeriesState::new(cx, TICK, TICK * WINDOW as u32);
        for i in 0..WINDOW {
            let t = TICK * i as u32;
            rx_hist.push(t, rx_at((WINDOW - 1) as u64, i));
            tx_hist.push(t, tx_at((WINDOW - 1) as u64, i));
        }

        // Recurring timers via `reactive::interval` (cancellation rides
        // scope disposal; missed ticks coalesce instead of replaying
        // after a suspend). The full live-data pattern — background
        // producers feeding bounded ingestion into a Feed — lives in
        // `examples/feed.rs` and `docs/live-data.md`.
        {
            let (rx_hist, tx_hist) = (rx_hist.clone(), tx_hist.clone());
            interval(cx, TICK, move || {
                tick.update(|t| *t += 1);
                let now = WINDOW as u64 - 1 + tick.get_untracked();
                let at = TICK * now as u32;
                rx_hist.push(at, rx_at(now, WINDOW - 1));
                tx_hist.push(at, tx_at(now, WINDOW - 1));
            });
        }
        interval(cx, Duration::from_secs(1), move || clock.set(clock_text()));

        // Startup notices (REACT's reactive bridge): every notice —
        // including the ones the engine pushes AFTER mount (input-path
        // degradation) — arrives as a staggered auto-dismissing toast,
        // exactly once. `caps: …` is launch status, not a degradation:
        // it stays off the glass (`--caps` prints the full report).
        {
            let overlays = overlays.clone();
            let notices = use_startup_notices(cx);
            let seen = Rc::new(std::cell::Cell::new(0usize));
            cx.effect(move || {
                let list = notices.get();
                let start = seen.replace(list.len());
                let mut slot = 0u64;
                for notice in list.iter().skip(start) {
                    if notice.starts_with("caps:") {
                        continue;
                    }
                    let overlays = overlays.clone();
                    let notice = notice.clone();
                    after(Duration::from_millis(600 + 900 * slot), move || {
                        let vp = current_viewport();
                        Toast::show(&overlays, cx, vp, notice, Duration::from_secs(4));
                    });
                    slot += 1;
                }
            });
        }

        let open_help = {
            let overlays = overlays.clone();
            let help = help.clone();
            move || {
                if let Some(m) = help.borrow_mut().take() {
                    m.close();
                    return;
                }
                let help_inner = help.clone();
                let modal = Modal::open(
                    &overlays,
                    cx,
                    viewport.get_untracked(),
                    Size::new(46, 12),
                    {
                        let help_inner = help_inner.clone();
                        move |mcx| help_panel(mcx, help_inner)
                    },
                );
                *help.borrow_mut() = Some(modal);
            }
        };

        let notify = {
            let overlays = overlays.clone();
            move || {
                toasts.update(|n| *n += 1);
                let n = toasts.get_untracked();
                Toast::show(
                    &overlays,
                    cx,
                    viewport.get_untracked(),
                    format!("deploy #{n} rolled out"),
                    Duration::from_secs(2),
                );
            }
        };

        Element::new()
            .style(LayoutStyle::column())
            .shortcut(KeyChord::plain(Key::Char('q')), move |_| quitter.quit())
            .shortcut(KeyChord::plain(Key::Char('?')), {
                let open_help = open_help.clone();
                move |_| open_help()
            })
            .shortcut(KeyChord::plain(Key::Char('n')), move |_| notify())
            .shortcut(KeyChord::plain(Key::Char('s')), move |_| {
                sort.update(|(_col, asc)| *asc = !*asc)
            })
            .shortcut(KeyChord::new(Mods::CTRL, Key::Char('t')), move |_| {
                theme_ix.update(|i| *i = (*i + 1) % themes().len());
                set_theme_by_id(themes()[theme_ix.get_untracked()].id);
            })
            .shortcut(KeyChord::plain(Key::Char('b')), {
                let overlays = overlays.clone();
                move |_| {
                    if mark_allowed {
                        show_mark.update(|b| *b = !*b);
                    } else {
                        Toast::show(
                            &overlays,
                            cx,
                            viewport.get_untracked(),
                            "brandmark needs a truecolor terminal",
                            Duration::from_secs(2),
                        );
                    }
                }
            })
            // Theme generation: everything below rebuilds with fresh
            // tokens on switch; panel Dyns inside re-render independently.
            .child(dyn_view_scoped(LayoutStyle::default().grow(1.0), {
                let mark_model = mark_model.clone();
                move |_gcx| {
                    let t = theme.get().tokens;
                    let label = theme.get().label;
                    Element::new()
                        .style(LayoutStyle::column())
                        .child(header(&t, label, clock))
                        .child(body(
                            &t,
                            tick,
                            (rx_hist.clone(), tx_hist.clone()),
                            nav,
                            session,
                            sort,
                            nav_focus,
                            table_focus,
                            show_mark,
                            mark_model.clone(),
                        ))
                        .child(footer(&t))
                        .build()
                }
            }))
            // Small-terminal guard (absolute overlay, painted last,
            // no-op above the minimum).
            .child(dyn_view(guard_layout(), move || {
                let t = theme.get().tokens;
                Element::new()
                    .style(guard_layout())
                    .draw(move |canvas, rect| {
                        common::too_small(canvas, rect, common::MIN_SIZE, &t);
                    })
                    .build()
            }))
            .build()
    })?;

    // Spatial pane navigation (REACT's cycle-7 `focus_next_in`): Alt +
    // arrows hop focus by GEOMETRY between the focusable panes (nav
    // list, sessions table, …). Plain arrows stay with the focused
    // widget — they mean row selection inside lists/tables here.
    for (chord_key, dir, name) in [
        (Key::Left, Key::Left, "focus.pane.left"),
        (Key::Right, Key::Right, "focus.pane.right"),
        (Key::Up, Key::Up, "focus.pane.up"),
        (Key::Down, Key::Down, "focus.pane.down"),
    ] {
        let mut tree = app.tree().handle();
        let registered =
            app.actions()
                .register(name, Some(KeyChord::new(Mods::ALT, chord_key)), move || {
                    tree.focus_next_in(dir);
                });
        // The registry refuses name/chord collisions; hear about it
        // (its own doc calls silence a programming error).
        if !registered {
            eprintln!("dashboard: action {name} collided — pane nav key missing");
        }
    }
    app.run()
}

// ---------------------------------------------------------------------------
// Sections
// ---------------------------------------------------------------------------

fn header(t: &TokenSet, theme_label: &'static str, clock: Signal<String>) -> View {
    let (surface, accent, text_c, muted) = (t.surface, t.accent, t.text, t.text_muted);
    Element::new()
        .style(LayoutStyle::row().h(1).shrink(0.0).gap(1))
        .draw(move |canvas, rect| {
            canvas.fill(rect, ' ', text_c, surface);
            let mut x = rect.x + 1;
            x += canvas.print(Point::new(x, rect.y), "▲ AbstractTUI", accent, surface);
            x += canvas.print(Point::new(x, rect.y), "  ops dashboard", muted, surface);
            let _ = x;
        })
        .child(dyn_view(LayoutStyle::default().h(1).grow(1.0), move || {
            // Right-aligned clock rides its own Dyn: one damaged row
            // per second, nothing else repaints.
            styled_text_right(format!("{}  ·  {}", theme_label, clock.get()))
        }))
        .build()
}

/// Right-aligned single-line text (draw-based; layout keeps the slot).
fn styled_text_right(s: String) -> View {
    let t = current_theme().tokens;
    let muted = t.text_muted;
    let surface = t.surface;
    Element::new()
        .style(LayoutStyle::default().h(1).grow(1.0))
        .draw(move |canvas, rect| {
            let w = s.chars().count() as i32;
            let x = (rect.right() - w - 1).max(rect.x);
            canvas.print(Point::new(x, rect.y), &s, muted, surface);
        })
        .build()
}

#[allow(clippy::too_many_arguments)]
fn body(
    t: &TokenSet,
    tick: Signal<u64>,
    traffic: (TimeSeriesState, TimeSeriesState),
    nav: Signal<usize>,
    session: Signal<usize>,
    sort: Signal<(usize, bool)>,
    nav_focus: Signal<bool>,
    table_focus: Signal<bool>,
    show_mark: Signal<bool>,
    mark_model: std::sync::Arc<abstracttui::three::Model>,
) -> View {
    Element::new()
        .style(LayoutStyle::row().grow(1.0).gap(1).padding(Edges::hv(1, 0)))
        .child(sidebar(t, nav, nav_focus))
        .child(
            Element::new()
                .style(LayoutStyle::column().grow(1.0).gap(0))
                .child(
                    Element::new()
                        .style(LayoutStyle::row().grow(3.0).gap(1))
                        .child(traffic_panel(t, traffic.0, traffic.1))
                        .child(load_panel(t, tick))
                        .child(mark_panel(t, tick, show_mark, mark_model))
                        .build(),
                )
                .child(
                    Element::new()
                        .style(LayoutStyle::row().grow(2.0).gap(1))
                        .child(log_panel(t, tick))
                        .child(sessions_panel(t, tick, session, sort, table_focus))
                        .build(),
                )
                .build(),
        )
        .build()
}

fn sidebar(t: &TokenSet, nav: Signal<usize>, nav_focus: Signal<bool>) -> View {
    let tokens = *t;
    // The pane's ring follows the LIST's real keyboard focus (§3.2's
    // composition rule, wired via the cycle-5 `focus_signal`).
    dyn_view_scoped(LayoutStyle::column().w(20), move |scx| {
        let items: Vec<String> = [
            "overview", "traffic", "sessions", "logs", "alerts", "settings",
        ]
        .iter()
        .map(|s| format!("  {s}"))
        .collect();
        Block::new()
            .title("nav")
            .shadow(tokens.shadow_ground)
            .focused(nav_focus.get())
            .fill(tokens.surface)
            .layout(LayoutStyle::column().grow(1.0))
            .child(
                List::new(items)
                    .selection(nav)
                    .focus_signal(nav_focus)
                    .layout(LayoutStyle::default().grow(1.0))
                    .element(scx, &tokens)
                    .build(),
            )
            .element(&tokens)
            .build()
    })
}

fn traffic_panel(t: &TokenSet, rx_hist: TimeSeriesState, tx_hist: TimeSeriesState) -> View {
    let tokens = *t;
    Block::new()
        .title("traffic — rx/tx (MB/s)")
        .shadow(tokens.shadow_ground)
        .title_align(TitleAlign::Left)
        .fill(tokens.surface)
        .layout(LayoutStyle::column().grow(2.0))
        .child(dyn_view(LayoutStyle::default().grow(1.0), move || {
            // Tracked ring reads: this panel re-renders per push; the
            // relative time axis labels the span the ring covers.
            let span = rx_hist.span();
            Element::new()
                .style(LayoutStyle::column().grow(1.0))
                .child(legend(&tokens))
                .child(
                    LineChart::new(vec![rx_hist.samples(), tx_hist.samples()])
                        .range(0.0, 100.0)
                        .time_axis(span)
                        .layout(LayoutStyle::default().grow(1.0))
                        .element(&tokens)
                        .build(),
                )
                .build()
        }))
        .element(t)
        .build()
}

fn legend(t: &TokenSet) -> View {
    let (c0, c1, muted) = (t.chart(0), t.chart(1), t.text_muted);
    Element::new()
        .style(LayoutStyle::default().h(1).shrink(0.0))
        .draw(move |canvas, rect| {
            let mut x = rect.x + 1;
            x += canvas.print(Point::new(x, rect.y), "── rx", c0, Rgba::TRANSPARENT);
            x += canvas.print(Point::new(x, rect.y), "   ", muted, Rgba::TRANSPARENT);
            canvas.print(Point::new(x, rect.y), "── tx", c1, Rgba::TRANSPARENT);
        })
        .build()
}

fn load_panel(t: &TokenSet, tick: Signal<u64>) -> View {
    let tokens = *t;
    Block::new()
        .title("load")
        .shadow(tokens.shadow_ground)
        .fill(tokens.surface)
        .layout(LayoutStyle::column().w(34))
        .child(dyn_view(LayoutStyle::column().grow(1.0), move || {
            let now = tick.get();
            let mut col = Element::new().style(LayoutStyle::column().gap(0));
            for (slot, (name, f)) in [
                ("cpu", cpu_at as fn(u64, usize) -> f32),
                ("mem", mem_at),
                ("io", io_at),
            ]
            .iter()
            .enumerate()
            {
                let cur = f(now, 0);
                let hist: Vec<f32> = (0..WINDOW / 2).map(|i| f(now, i)).collect();
                col = col
                    .child(metric_row(&tokens, name, cur))
                    .child(
                        Progress::new(cur)
                            .ramp(true)
                            .layout(LayoutStyle::default().h(1).shrink(0.0))
                            .element(&tokens)
                            .build(),
                    )
                    .child(
                        Sparkline::new(hist)
                            .slot(slot + 2)
                            .range(0.0, 1.0)
                            .layout(LayoutStyle::default().h(1).shrink(0.0))
                            .element(&tokens)
                            .build(),
                    );
            }
            col.build()
        }))
        .element(t)
        .build()
}

fn metric_row(t: &TokenSet, name: &'static str, value: f32) -> View {
    let (text_c, muted) = (t.text, t.text_muted);
    let label = name.to_string();
    let pct = format!("{:>4.0}%", value * 100.0);
    Element::new()
        .style(LayoutStyle::default().h(1).shrink(0.0))
        .draw(move |canvas, rect| {
            canvas.print(
                Point::new(rect.x + 1, rect.y),
                &label,
                muted,
                Rgba::TRANSPARENT,
            );
            let x = (rect.right() - pct.chars().count() as i32 - 1).max(rect.x);
            canvas.print(Point::new(x, rect.y), &pct, text_c, Rgba::TRANSPARENT);
        })
        .build()
}

fn log_panel(t: &TokenSet, tick: Signal<u64>) -> View {
    let tokens = *t;
    Block::new()
        .title("events")
        .shadow(tokens.shadow_ground)
        .fill(tokens.surface)
        .layout(LayoutStyle::column().grow(1.0).basis(Dimension::Cells(0)))
        .child(dyn_view(LayoutStyle::default().grow(1.0), move || {
            let now = tick.get();
            Element::new()
                .style(LayoutStyle::default().grow(1.0))
                .draw(move |canvas, rect| draw_log_tail(canvas, rect, &tokens, now))
                .build()
        }))
        .element(t)
        .build()
}

fn draw_log_tail(canvas: &mut dyn Canvas, rect: Rect, t: &TokenSet, now: u64) {
    // One line every other tick; the tail always fills the pane.
    let total = now / 2;
    let rows = rect.h.max(0) as u64;
    for row in 0..rows {
        let idx = match (total + row).checked_sub(rows - 1) {
            Some(i) => i,
            None => continue,
        };
        let (level, color, msg) = log_line(t, idx);
        let y = rect.y + row as i32;
        let ts = format!("{:02}:{:02}", (idx / 120) % 60, (idx / 2) % 60);
        let mut x = rect.x + 1;
        x += canvas.print(Point::new(x, y), &ts, t.text_faint, Rgba::TRANSPARENT);
        x += canvas.print(
            Point::new(x, y),
            &format!(" {level:<5} "),
            color,
            Rgba::TRANSPARENT,
        );
        // Clamp to the pane: draw closures see the whole canvas, so a
        // long message would collide with the border cell (rect
        // discipline is the widget's job — cycle-7 span-clipping rule).
        let avail = (rect.right() - 1 - x).max(0) as usize;
        if msg.chars().count() <= avail {
            canvas.print(Point::new(x, y), msg, t.text, Rgba::TRANSPARENT);
        } else {
            let fitted: String = msg.chars().take(avail.saturating_sub(1)).collect();
            canvas.print(Point::new(x, y), &(fitted + "…"), t.text, Rgba::TRANSPARENT);
        }
    }
}

fn sessions_panel(
    t: &TokenSet,
    tick: Signal<u64>,
    session: Signal<usize>,
    sort: Signal<(usize, bool)>,
    table_focus: Signal<bool>,
) -> View {
    let tokens = *t;
    // Scoped generation per rebuild: the table's internal signals die
    // with each data refresh instead of accumulating (dyn_view_scoped is
    // REACT's blessed recipe). The Block rides inside so its ring follows
    // the TABLE's real focus (cycle-5 focus_signal).
    dyn_view_scoped(
        LayoutStyle::column().grow(1.6).basis(Dimension::Cells(0)),
        move |scx| {
            let now = tick.get() / 8; // table refreshes every 2s
            let (col, asc) = sort.get();
            let mut rows = session_rows(now);
            rows.sort_by(|a, b| {
                let ord = match col {
                    2 | 3 => {
                        let pa: f32 = a[col].trim_end_matches(" MB/s").parse().unwrap_or(0.0);
                        let pb: f32 = b[col].trim_end_matches(" MB/s").parse().unwrap_or(0.0);
                        pa.partial_cmp(&pb).unwrap_or(std::cmp::Ordering::Equal)
                    }
                    _ => a[col].cmp(&b[col]),
                };
                if asc {
                    ord
                } else {
                    ord.reverse()
                }
            });
            let table = Table::new(vec![
                Column::new("host", ColWidth::Cells(12)),
                Column::new("region", ColWidth::Cells(8)),
                Column::new("rx", ColWidth::Flex(1.0)),
                Column::new("tx", ColWidth::Flex(1.0)),
                Column::new("state", ColWidth::Cells(8)),
            ])
            .rows(rows)
            .selection(session)
            .focus_signal(table_focus)
            .sorted(col, asc)
            .on_sort_requested(move |c| {
                sort.update(|(col, asc)| {
                    if *col == c {
                        *asc = !*asc;
                    } else {
                        *col = c;
                        *asc = false;
                    }
                })
            })
            .layout(LayoutStyle::default().grow(1.0))
            .element(scx, &tokens)
            .build();
            Block::new()
                .title("sessions — s toggles sort")
                .shadow(tokens.shadow_ground)
                .focused(table_focus.get())
                .fill(tokens.surface)
                .layout(LayoutStyle::column().grow(1.0))
                .child(table)
                .element(&tokens)
                .build()
        },
    )
}

/// The `b` flourish: the three-planes mark spinning in a mini viewport.
/// Rides the EXISTING data tick (no extra timers); absent from the tree
/// when off, so the idle damage economy is untouched.
fn mark_panel(
    t: &TokenSet,
    tick: Signal<u64>,
    show: Signal<bool>,
    model: std::sync::Arc<abstracttui::three::Model>,
) -> View {
    use abstracttui::three::Light;
    use abstracttui::three::Vec3;
    use abstracttui::widgets::Viewport3D;
    let tokens = *t;
    dyn_view(LayoutStyle::default(), move || {
        if !show.get() {
            return Element::new().style(LayoutStyle::default().w(0)).build();
        }
        let spin = tick.get() as f32 * 0.06;
        Block::new()
            .title("mark")
            .fill(tokens.bg)
            .layout(LayoutStyle::column().w(26))
            .child(
                Viewport3D::new(model.clone())
                    .orbit(0.5, 0.3, 1.15)
                    .spin(spin)
                    .light(Light {
                        direction: Vec3::new(-0.35, -0.6, -0.7),
                        ambient: 0.55,
                        diffuse: 0.5,
                    })
                    .background(tokens.bg)
                    .layout(LayoutStyle::default().grow(1.0))
                    .element(&tokens)
                    .build(),
            )
            .element(&tokens)
            .build()
    })
}

fn footer(t: &TokenSet) -> View {
    let tokens = *t;
    Element::new()
        .style(LayoutStyle::default().h(1).shrink(0.0))
        .draw(move |canvas, rect| {
            common::key_legend(
                canvas,
                rect,
                &tokens,
                &[
                    ("tab", "focus"),
                    ("alt+←→", "panes"),
                    ("s", "sort"),
                    ("n", "toast"),
                    ("b", "mark"),
                    ("?", "help"),
                    ("ctrl+t", "theme"),
                    ("q", "quit"),
                ],
            );
        })
        .build()
}

fn help_panel(mcx: Scope, slot: Rc<RefCell<Option<Modal>>>) -> View {
    let t = current_theme().tokens;
    let close = move |_: &mut abstracttui::ui::EventCtx| {
        if let Some(m) = slot.borrow_mut().take() {
            m.close();
        }
    };
    let _ = mcx;
    Element::new()
        .style(LayoutStyle::column().gap(1).padding(Edges::all(1)))
        .shortcut(KeyChord::plain(Key::Escape), close.clone())
        .shortcut(KeyChord::plain(Key::Char('?')), close)
        .child(Logo::new().element(&t).build())
        .child(text("Tab / Shift+Tab   move keyboard focus"))
        .child(text("↑ ↓ PgUp PgDn     drive the list and table"))
        .child(text("s                 toggle sort direction"))
        .child(text("n                 pop a toast"))
        .child(text("Ctrl+T            cycle theme"))
        .child(text("Esc or ?          close this panel"))
        .build()
}
