//! Wave 13 MODAL-BG shared fixture: the gateway console's exact screen
//! shape rebuilt from engine widgets, plus the drive/oracle harness.
//! `#[path]` sibling of tests/wave_modal_bg.rs (file-size rule).
//!
//! Console anatomy mirrored here (console-tui/src/ui/mod.rs + review.rs
//! at abstracttui 0.2.21, read-only):
//! - root column: header line(1) · blank line(1) · PageHost in a
//!   grow(1.0) dyn wrapper (6 pages, review active) · goal line ·
//!   footer column(2) — all chrome `shrink(0.0)`;
//! - review page: journal Block(grow 1.0, padding 1) whose dyn content
//!   is EITHER a one-line empty-state OR a Scroll of rows · live-test
//!   Block(shrink 0.0) [teaching line(1) · result dyn h(4) · button
//!   row h(1)] · finish row h(2);
//! - sandbox modal (76x18): fill panel + Esc shortcut wrapping a
//!   column [title · provider Select(autofocus) · model dyn (swaps
//!   line -> TextInput once a provider is picked) · prompt TextInput ·
//!   result dyn h(4) · Generate/Close buttons], opened on the review
//!   page's generation scope with the store reset in the SAME batch —
//!   the console's `open_sandbox_modal` through `open_form_guarded`;
//! - notices: an effect mirrors `store.notice` into a real `Toast`
//!   (console install_effects), and the footer's busy line re-renders
//!   from a ticking signal while an op is in flight (the 500 ms busy
//!   interval).

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use abstracttui::app::select::{Select, SelectOption};
use abstracttui::app::use_theme;
use abstracttui::app::{use_viewport, App, Driver, Modal, Toast};
use abstracttui::base::{Point, Rect, Rgba, Size};
use abstracttui::layout::{Edges, Style as LayoutStyle};
use abstracttui::reactive::{Scope, Signal};
use abstracttui::render::{Screenshot, Style};
use abstracttui::testing::{CaptureTerm, VtScreen};
use abstracttui::theme::TokenSet;
use abstracttui::ui::{dyn_view, dyn_view_scoped, Element, Key, KeyChord, View};
use abstracttui::widgets::{Block, BorderKind, Button, PageHost, Scroll, TextInput};

use super::harness::drive_to_idle;

// ---------------------------------------------------------------------
// console-shaped fixture
// ---------------------------------------------------------------------

/// The console's Loadable, reduced to what the review page renders.
#[derive(Clone, PartialEq)]
pub enum Sandbox {
    NotAsked,
    Loading,
    Ready(String),
}

#[derive(Clone)]
pub struct Fx {
    pub sandbox: Signal<Sandbox>,
    pub journal_rows: Signal<usize>,
    pub header_conn: Signal<String>,
    /// Mirrored into a real Toast by a root effect (console
    /// install_effects) AND read by the footer's notice line.
    pub notice: Signal<Option<String>>,
    /// Busy-op label; Some = the footer busy line renders and re-reads
    /// `tick` (the console's 500 ms busy interval shape).
    pub busy: Signal<Option<String>>,
    pub tick: Signal<u64>,
    /// The review PAGE's generation scope (PageHost page builder cx) —
    /// the console's `g` shortcut opens the modal on exactly this scope.
    pub page_scope: Rc<RefCell<Option<Scope>>>,
    pub modal: Rc<RefCell<Option<Modal>>>,
    pub overlays: abstracttui::app::Overlays,
}

/// Hand-rolled styled line — the console's `util::line` (a draw
/// closure printing spans, clipped to its own rect).
pub fn line(spans: Vec<(String, Rgba)>) -> View {
    Element::new()
        .style(LayoutStyle::line(1))
        .draw(move |canvas, rect| {
            let mut x = rect.x;
            let right = rect.x + rect.w;
            for (text, ink) in &spans {
                if x >= right {
                    break;
                }
                let budget = (right - x).max(0) as usize;
                let fitted: String = {
                    let mut out = String::new();
                    let mut used = 0usize;
                    for ch in text.chars() {
                        let w = abstracttui::text::width(&ch.to_string()) as usize;
                        if used + w > budget {
                            break;
                        }
                        out.push(ch);
                        used += w;
                    }
                    out
                };
                canvas.print_styled(Point::new(x, rect.y), &fitted, &Style::new().fg(*ink));
                x += abstracttui::text::width(&fitted);
            }
        })
        .build()
}

/// The console's `util::field`: fixed-width muted label + child.
pub fn field(t: &TokenSet, label: &str, child: View) -> View {
    let ink = t.text_muted;
    let label = label.to_string();
    Element::new()
        .style(LayoutStyle::row().gap(1))
        .child(
            Element::new()
                .style(LayoutStyle::default().w(18).h(1).shrink(0.0))
                .draw(move |canvas, rect| {
                    canvas.print(Point::new(rect.x, rect.y), &label, ink, Rgba::TRANSPARENT);
                })
                .build(),
        )
        .child(child)
        .build()
}

/// review.rs `sandbox_result`, reduced: 1 line NotAsked/Loading, 3
/// lines Ready.
pub fn sandbox_result(t: &TokenSet, s: &Sandbox) -> View {
    match s {
        Sandbox::NotAsked => line(vec![("no test run yet".into(), t.text_faint)]),
        Sandbox::Loading => line(vec![(
            "⟳ generating… (a real model call — can take tens of seconds)".into(),
            t.info,
        )]),
        Sandbox::Ready(text) => Element::new()
            .style(LayoutStyle::column())
            .child(line(vec![("✓ lmstudio / test-model".into(), t.ok)]))
            .child(line(vec![(format!("  “{text}”"), t.text)]))
            .child(line(vec![("  usage: 42 tk".into(), t.text_faint)]))
            .build(),
    }
}

/// The review page — the console's review::view shape. An empty
/// journal renders the one-line empty state (the operator's fresh
/// wizard session); a populated one renders a Scroll of rows.
fn review_page(cx: Scope, fx: &Fx, t: &TokenSet) -> View {
    let tt = *t;
    let fx2 = fx.clone();
    *fx.page_scope.borrow_mut() = Some(cx);
    let sandbox = fx.sandbox;
    let journal_rows = fx.journal_rows;

    Element::new()
        .style(LayoutStyle::column().gap(1))
        .shortcut(KeyChord::plain(Key::Char('g')), move |_| {
            open_sandbox_modal(&fx2);
        })
        .child(
            Block::new()
                .border(BorderKind::Rounded)
                .title("Changes this session (apply → verify via GET)")
                .fill(t.surface)
                .layout(LayoutStyle::column().gap(0).grow(1.0).padding(Edges::all(1)))
                .child(dyn_view_scoped(
                    LayoutStyle::default().grow(1.0),
                    move |gcx| {
                        let n = journal_rows.get();
                        if n == 0 {
                            return line(vec![(
                                "no changes applied this session — the wizard only writes when you save"
                                    .into(),
                                tt.text_muted,
                            )]);
                        }
                        let mut rows: Vec<View> = Vec::new();
                        for i in 0..n {
                            rows.push(line(vec![(
                                format!("12:0{} ✓ PUT /providers/profile-{i}", i % 10),
                                tt.text,
                            )]));
                            rows.push(line(vec![(
                                format!("        verified · profile-{i} present"),
                                tt.ok,
                            )]));
                        }
                        Scroll::new(
                            Element::new()
                                .style(LayoutStyle::column())
                                .children(rows)
                                .build(),
                        )
                        .view(gcx)
                    },
                ))
                .element(t)
                .build(),
        )
        .child(
            Block::new()
                .border(BorderKind::Rounded)
                .title("Live test (sandbox generate)")
                .fill(t.surface)
                .layout(LayoutStyle::column().gap(0).shrink(0.0).padding(Edges::all(1)))
                .child(dyn_view(LayoutStyle::line(1).shrink(0.0), move || {
                    line(vec![(
                        "run a REAL text generation through the gateway to prove a provider/model pair works"
                            .into(),
                        tt.text_muted,
                    )])
                }))
                .child(dyn_view(LayoutStyle::default().h(4).shrink(0.0), move || {
                    sandbox_result(&tt, &sandbox.get())
                }))
                .child(dyn_view_scoped(LayoutStyle::default().h(1).shrink(0.0), {
                    let fx3 = fx.clone();
                    move |gcx| {
                        let fx4 = fx3.clone();
                        Element::new()
                            .style(LayoutStyle::row().gap(2))
                            .child(
                                Button::new("Run a test (g)")
                                    .on_click(move || open_sandbox_modal(&fx4))
                                    .element(gcx, &tt)
                                    .build(),
                            )
                            .build()
                    }
                }))
                .element(t)
                .build(),
        )
        .child(dyn_view_scoped(
            LayoutStyle::default().h(2).shrink(0.0),
            move |gcx| {
                Element::new()
                    .style(LayoutStyle::row().gap(2).h(1))
                    .child(
                        Button::new("Finish — switch to browse mode")
                            .on_click(|| {})
                            .element(gcx, &tt)
                            .build(),
                    )
                    .build()
            },
        ))
        .build()
}

/// The console root: header + separator + PageHost + goal + footer,
/// plus the notice->Toast effect (install_effects).
fn console_root(cx: Scope, fx: Fx) -> View {
    let theme = use_theme(cx);
    let header_conn = fx.header_conn;
    let notice = fx.notice;
    let busy = fx.busy;
    let tick = fx.tick;

    // Notices → toast (console install_effects): every notice write
    // raises a real Toast layer over whatever is open.
    {
        let overlays = fx.overlays.clone();
        cx.effect(move || {
            if let Some(n) = notice.get() {
                let viewport = use_viewport(cx).get_untracked();
                Toast::show(&overlays, cx, viewport, n, Duration::from_secs(4));
            }
        });
    }

    let active = cx.signal("review".to_string());

    let host_fx = fx.clone();
    let host = dyn_view_scoped(LayoutStyle::default().grow(1.0), move |hcx| {
        let fx_page = host_fx.clone();
        let mut ph = PageHost::new();
        for (id, title) in [
            ("connection", "1 Connection"),
            ("providers", "2 Providers"),
            ("routes", "3 Routes"),
            ("users", "4 Users & Entities"),
            ("runtimes", "5 Runtimes"),
        ] {
            ph = ph.page(id, title, move |_gcx| {
                abstracttui::ui::text(format!("{title} page"))
            });
        }
        ph.page("review", "6 Review & Test", move |gcx| {
            let theme = use_theme(gcx);
            review_page(gcx, &fx_page, &theme.get().tokens)
        })
        .active(active)
        .number_jump(false)
        .chords(&[], &[])
        .view(hcx)
    });

    Element::new()
        .style(LayoutStyle::column())
        .child(dyn_view(LayoutStyle::line(1).shrink(0.0), move || {
            let t = theme.get().tokens;
            line(vec![
                (" AbstractGateway Console ".into(), t.accent),
                ("· wizard ".into(), t.text_muted),
                ("· http://127.0.0.1:8080 ".into(), t.text_muted),
                (format!("● {}", header_conn.get()), t.ok),
            ])
        }))
        .child(dyn_view(LayoutStyle::line(1).shrink(0.0), move || {
            line(vec![(String::new(), theme.get().tokens.text)])
        }))
        .child(host)
        .child(dyn_view_scoped(LayoutStyle::default().shrink(0.0), move |_| {
            let t = theme.get().tokens;
            Element::new()
                .style(LayoutStyle::line(1).shrink(0.0))
                .child(line(vec![
                    (" Step goal: ".into(), t.accent),
                    (
                        "run one real test (g) to prove a provider, then Finish.".into(),
                        t.text_muted,
                    ),
                ]))
                .build()
        }))
        .child(
            Element::new()
                .style(LayoutStyle::column().shrink(0.0))
                .child(dyn_view(LayoutStyle::line(1).shrink(0.0), move || {
                    // The console footer busy strip: in-flight ops with
                    // elapsed seconds (tick keeps it live), else the
                    // notice, else blank.
                    let t = theme.get().tokens;
                    let ticks = tick.get();
                    match busy.get() {
                        Some(op) => line(vec![(
                            format!(" ⟳ {op}… {}s", ticks / 2),
                            t.info,
                        )]),
                        None => match notice.get() {
                            Some(n) => line(vec![(format!(" {n}"), t.text_muted)]),
                            None => line(vec![(String::new(), t.text_muted)]),
                        },
                    }
                }))
                .child(dyn_view(LayoutStyle::line(1).shrink(0.0), move || {
                    let t = theme.get().tokens;
                    line(vec![(
                        " Ctrl+N/] next step · Ctrl+P/Esc back · Ctrl+C quit · Tab focus · g run sandbox test"
                            .into(),
                        t.text_muted,
                    )])
                }))
                .build(),
        )
        .build()
}

/// The sandbox modal — the console's `open_sandbox_modal` through the
/// same `open_form_guarded` shape (fill panel + Esc shortcut + column
/// of fields), on the review PAGE's generation scope. The store reset
/// rides the same batch as the open, exactly like the console.
pub const MODAL_SIZE: Size = Size { w: 76, h: 18 };

pub fn open_sandbox_modal(fx: &Fx) {
    let Some(cx) = *fx.page_scope.borrow() else {
        panic!("page scope not captured");
    };
    // console: a fresh modal starts with a fresh result slot.
    fx.sandbox.set(Sandbox::NotAsked);
    if let Some(m) = fx.modal.borrow_mut().take() {
        m.close();
    }
    let viewport = use_viewport(cx).get_untracked();
    let slot = fx.modal.clone();
    let closer: Rc<dyn Fn()> = Rc::new(move || {
        if let Some(m) = slot.borrow_mut().take() {
            m.close();
        }
    });
    let sandbox = fx.sandbox;
    let busy = fx.busy;
    let c_esc = closer.clone();
    let modal = Modal::open(&fx.overlays, cx, viewport, MODAL_SIZE, move |mcx| {
        let theme = use_theme(mcx);
        let t0 = theme.get().tokens;
        let prov_ix = mcx.signal(0usize);
        let prompt = mcx.signal("Reply with one short sentence: what model are you?".to_string());
        let closer2 = closer.clone();
        let c_x = closer.clone();
        Element::new()
            .style(LayoutStyle::fill())
            .shortcut(KeyChord::plain(Key::Escape), move |_| c_esc())
            // Test-only close chord: a BARE Esc byte needs the reader's
            // esc_timeout to disambiguate, which scripted turns never
            // reach; 'x' exercises the same in-tree shortcut -> closer
            // path deterministically.
            .shortcut(KeyChord::plain(Key::Char('x')), move |_| c_x())
            .child(
                Element::new()
                    .style(LayoutStyle::column().gap(0))
                    .child(line(vec![(
                        "Sandbox test — real generation".into(),
                        t0.accent,
                    )]))
                    .child(field(
                        &t0,
                        "provider",
                        Select::new(vec![
                            SelectOption::new("choose a provider…"),
                            SelectOption::new("lmstudio"),
                            SelectOption::new("ovh-provider"),
                        ])
                        .value(prov_ix)
                        .layout(LayoutStyle::default().w(40).h(1).shrink(0.0))
                        .element(mcx, &t0)
                        .autofocus()
                        .build(),
                    ))
                    .child(dyn_view_scoped(LayoutStyle::column().gap(0), move |gcx| {
                        // The console's model row: swaps a static line
                        // for an editor once a provider is picked (a
                        // dyn REGENERATION inside the open modal).
                        let t = theme.get().tokens;
                        if prov_ix.get() == 0 {
                            return field(
                                &t,
                                "model",
                                line(vec![("choose a provider first".into(), t.text_faint)]),
                            );
                        }
                        field(
                            &t,
                            "model",
                            TextInput::new()
                                .placeholder("type the model id")
                                .layout(LayoutStyle::default().w(52).h(1))
                                .element(gcx, &t)
                                .build(),
                        )
                    }))
                    .child(field(
                        &t0,
                        "prompt",
                        TextInput::new()
                            .value(prompt)
                            .layout(LayoutStyle::default().w(52).h(1))
                            .element(mcx, &t0)
                            .build(),
                    ))
                    .child(dyn_view(
                        LayoutStyle::default().h(4).shrink(0.0),
                        move || {
                            let t = theme.get().tokens;
                            sandbox_result(&t, &sandbox.get())
                        },
                    ))
                    .child(dyn_view_scoped(
                        LayoutStyle::default().h(1).shrink(0.0),
                        move |gcx| {
                            let t = theme.get().tokens;
                            let closer3 = closer2.clone();
                            Element::new()
                                .style(LayoutStyle::row().gap(2))
                                .child(
                                    Button::new("Generate")
                                        .on_click(move || {
                                            // console: synchronous Loading
                                            // + a busy op for the footer.
                                            sandbox.set(Sandbox::Loading);
                                            busy.set(Some("sandbox test".into()));
                                        })
                                        .element(gcx, &t)
                                        .build(),
                                )
                                .child(
                                    Button::new("Close (Esc)")
                                        .on_click(move || closer3())
                                        .element(gcx, &t)
                                        .build(),
                                )
                                .build()
                        },
                    ))
                    .build(),
            )
            .build()
    });
    *fx.modal.borrow_mut() = Some(modal);
}

/// Mount the fixture; returns the app + handles.
pub fn mount_console(size: Size, journal_rows: usize, sandbox0: Sandbox) -> (App, Fx) {
    let mut app = App::new(size);
    let overlays = app.overlays();
    let fx_slot: Rc<RefCell<Option<Fx>>> = Rc::new(RefCell::new(None));
    let fs = fx_slot.clone();
    app.mount(move |cx| {
        let fx = Fx {
            sandbox: cx.signal(sandbox0.clone()),
            journal_rows: cx.signal(journal_rows),
            header_conn: cx.signal("admin@default (token) admin".to_string()),
            notice: cx.signal(None),
            busy: cx.signal(None),
            tick: cx.signal(0),
            page_scope: Rc::new(RefCell::new(None)),
            modal: Rc::new(RefCell::new(None)),
            overlays: overlays.clone(),
        };
        *fs.borrow_mut() = Some(fx.clone());
        console_root(cx, fx)
    })
    .expect("mount");
    let fx = fx_slot.borrow().clone().expect("fx");
    (app, fx)
}

/// The modal's panel rect (mirrors popups::modal_bounds).
pub fn modal_rect(viewport: Size, size: Size) -> Rect {
    Rect::new(
        ((viewport.w - size.w) / 2).max(0),
        ((viewport.h - size.h) / 2).max(0),
        size.w.min(viewport.w),
        size.h.min(viewport.h),
    )
}

/// The live-test block's row band in a capture: from the row whose
/// text carries the block title to the row carrying the Run button
/// (inclusive, +1 for the bottom border). The block is `shrink(0.0)`
/// with a fixed h(4) result region, so ONLY these rows may change
/// when the sandbox store updates. `None` when the modal panel has
/// veiled the anchor strings (wide panels on narrow terminals) — the
/// caller falls back to a capture where they are visible.
pub fn live_block_band(shot: &Screenshot, viewport: Size) -> Option<Rect> {
    let lines: Vec<String> = shot.to_text().lines().map(str::to_string).collect();
    let top = lines.iter().position(|l| l.contains("Live test"))? as i32;
    let btn = lines.iter().position(|l| l.contains("Run a test"))? as i32;
    Some(Rect::new(0, top, viewport.w, (btn - top + 2).max(0)))
}

/// Drive the console's real open path: Tab until the "Run a test"
/// button holds focus, then Enter (the Button keyboard activation) —
/// the modal opens INSIDE dispatch, exactly like the console's click.
/// Returns the last pre-open screenshot (focus ring already painted).
pub fn open_modal_via_keyboard(
    driver: &mut Driver,
    app: &mut App,
    term: &mut CaptureTerm,
    vt: &mut VtScreen,
    fx: &Fx,
) -> Screenshot {
    for _ in 0..10 {
        term.push_input(b"\t");
        drive_to_idle(driver, app, term, vt);
        let pre = vt.screenshot();
        term.push_input(b"\r");
        drive_to_idle(driver, app, term, vt);
        if fx.modal.borrow().is_some() {
            return pre;
        }
    }
    panic!(
        "could not reach the Run button via Tab walk\n{}",
        vt.to_text()
    );
}
