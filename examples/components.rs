//! components — the shareable-component pattern, concretely.
//!
//! THE REFERENCE for app authors: how to build reusable components with
//! props, children and events — "like a React web page" — from plain
//! functions. No macros, no registry: a component is
//!
//! ```text
//! fn my_component(cx: Scope, t: &TokenSet, props...) -> View
//! ```
//!
//! - PROPS are arguments (owned data + `Signal<T>` for live bindings).
//! - CHILDREN are `View` arguments (composition — see `field`).
//! - EVENTS are `impl FnMut(..) + 'static` callbacks (see `StatCard`'s
//!   `on_click`), captured into the widget's handlers.
//! - STATE lives in signals created by the CALLER (or the component's
//!   own `cx` for private state); writes re-render exactly the `Dyn`
//!   regions that read them.
//!
//! The screen composes the same three components repeatedly with
//! different props: a toolbar, three clickable stat cards (one component,
//! three instances, independent counters), and a settings form whose
//! fields wrap ANY child widget — with a live summary reading the same
//! signals the form writes.
//!
//! Keys: Tab focus · Enter/Space activate · type in the field ·
//! Ctrl+T theme · q quit.
//!
//! OWNER: DESIGN.

mod common;

use abstracttui::prelude::*;
use abstracttui::theme::themes;
use abstracttui::ui::{MouseKind, Phase, UiEvent};

fn main() -> abstracttui::base::Result<()> {
    if !abstracttui::term::have_tty() {
        println!("components: needs an interactive terminal — skipping cleanly");
        return Ok(());
    }
    let mut app = App::new(Size::new(96, 30));
    let quitter = app.quitter();
    app.mount(move |cx| {
        let theme = use_theme(cx);
        // App state: plain signals. Components receive them as props.
        let deploys = cx.signal(12u32);
        let alerts = cx.signal(2u32);
        let uptime_days = cx.signal(97u32);
        let name = cx.signal(String::from("orion"));
        let notify = cx.signal(true);
        let channel = cx.signal(0usize);
        let theme_ix = cx.signal(0usize);
        let features = cx.signal(Vec::<String>::new());
        // App-owned fold state for the CONTROLLED disclosure card —
        // the external button flips it (the 0850 toggle-all shape).
        let ext_folded = cx.signal(true);

        Element::new()
            .style(LayoutStyle::column().padding(Edges::all(1)).gap(1))
            .shortcut(KeyChord::plain(Key::Char('q')), move |_| quitter.quit())
            .shortcut(KeyChord::new(Mods::CTRL, Key::Char('t')), move |_| {
                theme_ix.update(|i| *i = (*i + 1) % themes().len());
                set_theme_by_id(themes()[theme_ix.get_untracked()].id);
            })
            // One theme generation: rebuilt with fresh tokens on switch.
            .child(dyn_view_scoped(LayoutStyle::default().grow(1.0), move |gcx| {
                let t = theme.get().tokens;
                Element::new()
                    .style(LayoutStyle::column().gap(1))
                    // -- component #1: Toolbar { children } --------------
                    .child(toolbar(
                        &t,
                        vec![
                            Button::new("deploy")
                                .on_click(move || deploys.update(|n| *n += 1))
                                .element(gcx, &t)
                                .build(),
                            Button::new("ack alert")
                                .on_click(move || alerts.update(|n| *n = n.saturating_sub(1)))
                                .element(gcx, &t)
                                .build(),
                            Button::new("disabled").disabled(true).element(gcx, &t).build(),
                        ],
                    ))
                    // -- component #2: StatCard, three instances ---------
                    // Same function, different props/events; each card's
                    // value is a live Dyn over its own signal.
                    .child(
                        Element::new()
                            .style(LayoutStyle::row().gap(2).h(6))
                            .child(stat_card(&t, "deploys", deploys, Trend::Up, move || {
                                deploys.update(|n| *n += 1)
                            }))
                            .child(stat_card(&t, "alerts", alerts, Trend::Down, move || {
                                alerts.update(|n| *n = n.saturating_sub(1))
                            }))
                            .child(stat_card(&t, "uptime d", uptime_days, Trend::Flat, move || {
                                uptime_days.update(|n| *n += 1)
                            }))
                            .build(),
                    )
                    // -- component #3: Field { label, child } ------------
                    // The child is ANY View: input, checkbox, radio — the
                    // wrapper never knows what it hosts (composition).
                    .child(
                        Block::new()
                            .title("settings")
                            .fill(t.surface)
                            .shadow(t.shadow_ground)
                            .layout(LayoutStyle::column().gap(1).grow(1.0))
                            .child(field(
                                &t,
                                "name",
                                TextInput::new()
                                    .value(name)
                                    .placeholder("service name…")
                                    .layout(LayoutStyle::default().w(32).h(1))
                                    .element(gcx, &t)
                                    .build(),
                            ))
                            .child(field(
                                &t,
                                "notifications",
                                Checkbox::new("page the on-call")
                                    .checked(notify)
                                    .element(gcx, &t)
                                    .build(),
                            ))
                            .child(field(
                                &t,
                                "channel",
                                RadioGroup::new(vec!["stable".to_string(), "beta".to_string(), "nightly".to_string()])
                                .selection(channel)
                                .element(gcx, &t)
                                .build(),
                            ))
                            // -- the 0500 choice family, real state ------
                            // The SAME channel signal as the radio above:
                            // committing here moves the radio dot too.
                            .child(field(
                                &t,
                                "channel select",
                                Select::new(vec![
                                    SelectOption::new("stable").hint("lts"),
                                    SelectOption::new("beta"),
                                    SelectOption::new("nightly").hint("daily"),
                                ])
                                .value(channel)
                                .layout(LayoutStyle::default().w(28).h(1).shrink(0.0))
                                .element(gcx, &t)
                                .build(),
                            ))
                            // The consumer's /theme case: pick a theme by
                            // typing — commit applies it live.
                            .child(field(
                                &t,
                                "theme",
                                Combobox::new(
                                    themes()
                                        .iter()
                                        .map(|th| {
                                            SelectOption::new(th.label)
                                                .hint(if th.dark { "dark" } else { "light" })
                                        })
                                        .collect(),
                                )
                                .value(theme_ix)
                                .placeholder("type to search themes…")
                                .on_change(|i| {
                                    set_theme_by_id(themes()[i].id);
                                })
                                .layout(LayoutStyle::default().w(28).h(1).shrink(0.0))
                                .element(gcx, &t)
                                .build(),
                            ))
                            .child(field(
                                &t,
                                "features",
                                MultiSelect::new(vec![
                                    SelectOption::new("autosave"),
                                    SelectOption::new("telemetry"),
                                    SelectOption::new("backups"),
                                    SelectOption::new("beta api").disabled(true),
                                ])
                                .values(features)
                                .placeholder("enable features…")
                                .layout(LayoutStyle::default().w(28).h(1).shrink(0.0))
                                .element(gcx, &t)
                                .build(),
                            ))
                            // Live summary: a Dyn reading the SAME signals
                            // the form writes — edits appear as you type.
                            .child(dyn_view(LayoutStyle::default().h(1), move || {
                                let ch = ["stable", "beta", "nightly"][channel.get().min(2)];
                                text(format!(
                                    "→ {} · notifications {} · {} channel · {} features",
                                    if name.get().is_empty() { "unnamed".into() } else { name.get() },
                                    if notify.get() { "on" } else { "off" },
                                    ch,
                                    features.get().len(),
                                ))
                            }))
                            .element(&t)
                            .build(),
                    )
                    // -- component #4: Disclosure cards ------------------
                    // Progressive disclosure, three state modes: card 1
                    // starts OPEN (uncontrolled initial state), card 2
                    // folds a long body behind a capped scroll region
                    // (unfold it: 4 rows + a scrollbar), card 3's fold
                    // state lives in an app signal the external button
                    // writes (the toggle-all policy shape). Click a
                    // title row or focus it and press Enter/Space.
                    .child(
                        Element::new()
                            .style(LayoutStyle::row().gap(2))
                            .child(
                                Button::new("toggle 3rd")
                                    .on_click(move || ext_folded.update(|f| *f = !*f))
                                    .element(gcx, &t)
                                    .build(),
                            )
                            .child(
                                Disclosure::markdown("welcome", "**hello** — this card starts open")
                                    .initially_folded(false)
                                    .max_body_rows(3)
                                    .layout(card_slot())
                                    .element(gcx, &t)
                                    .build(),
                            )
                            .child(
                                Disclosure::text("build log", build_log())
                                    .detail("14 lines")
                                    .max_body_rows(4)
                                    .layout(card_slot())
                                    .element(gcx, &t)
                                    .build(),
                            )
                            .child(
                                Disclosure::text("external", "fold state lives in an app signal")
                                    .folded(ext_folded)
                                    .layout(card_slot())
                                    .element(gcx, &t)
                                    .build(),
                            )
                            .build(),
                    )
                    .child(text(
                        "tab focus · enter/space activate · click cards · ctrl+t theme · q quit",
                    ))
                    .build()
            }))
            .build()
    })?;
    app.run()
}

/// Trend direction prop for [`stat_card`].
#[derive(Copy, Clone)]
enum Trend {
    Up,
    Down,
    Flat,
}

/// A reusable, clickable stat card: label + live value + trend arrow.
/// PROPS: label (data), `value` (live signal), trend; EVENT: `on_click`.
/// Click or focus+Enter fires it. One function — every instance on
/// screen is this.
fn stat_card(
    t: &TokenSet,
    label: &'static str,
    value: Signal<u32>,
    trend: Trend,
    mut on_click: impl FnMut() + 'static,
) -> View {
    let (arrow, arrow_ink) = match trend {
        Trend::Up => ("▲", t.ok),
        Trend::Down => ("▼", t.error),
        Trend::Flat => ("→", t.text_muted),
    };
    let label_ink = t.text_muted;
    let value_ink = t.accent;
    Block::new()
        .border(BorderKind::Rounded)
        .fill(t.surface)
        .shadow(t.shadow_ground)
        .layout(LayoutStyle::column().grow(1.0))
        .child(
            Element::new()
                .style(LayoutStyle::default().h(1))
                .draw(move |canvas, rect| {
                    canvas.print(
                        Point::new(rect.x, rect.y),
                        label,
                        label_ink,
                        Rgba::TRANSPARENT,
                    );
                })
                .build(),
        )
        .child(dyn_view(LayoutStyle::default().h(1), move || {
            // The card's number is live: only THIS row re-renders when
            // its signal changes.
            let shown = format!("{}", value.get());
            Element::new()
                .style(LayoutStyle::default().h(1))
                .draw(move |canvas, rect| {
                    canvas.print(
                        Point::new(rect.x, rect.y),
                        &shown,
                        value_ink,
                        Rgba::TRANSPARENT,
                    );
                })
                .build()
        }))
        .child(
            Element::new()
                .style(LayoutStyle::default().h(1))
                .draw(move |canvas, rect| {
                    canvas.print(
                        Point::new(rect.x, rect.y),
                        arrow,
                        arrow_ink,
                        Rgba::TRANSPARENT,
                    );
                })
                .build(),
        )
        .element(t)
        // Events attach to the ELEMENT the component returns — callers
        // could add more (the component stays open for extension).
        .focusable()
        .on(Phase::Bubble, move |ctx, ev| match ev {
            UiEvent::Mouse(m) if matches!(m.kind, MouseKind::Down(_)) => {
                on_click();
                ctx.stop_propagation();
            }
            UiEvent::Key(k) if k.key == Key::Enter || k.key == Key::Char(' ') => {
                on_click();
                ctx.stop_propagation();
            }
            _ => {}
        })
        .build()
}

/// A labeled form row: `label` in the gutter, ANY child view beside it —
/// children-as-props composition.
fn field(t: &TokenSet, label: &'static str, child: View) -> View {
    let ink = t.text_muted;
    Element::new()
        .style(LayoutStyle::row().gap(1))
        .child(
            Element::new()
                .style(LayoutStyle::default().w(15).h(1))
                .draw(move |canvas, rect| {
                    canvas.print(Point::new(rect.x, rect.y), label, ink, Rgba::TRANSPARENT);
                })
                .build(),
        )
        .child(child)
        .build()
}

/// Equal-split slot for a disclosure card in the demo row: basis 0 +
/// grow shares the row; shrink 0 keeps cards honest under pressure.
fn card_slot() -> LayoutStyle {
    LayoutStyle::column()
        .grow(1.0)
        .basis(Dimension::Cells(0))
        .shrink(0.0)
}

/// 14 log lines — enough to overflow the 4-row body cap and show the
/// scrollbar.
fn build_log() -> String {
    (0..14)
        .map(|i| format!("compiling unit {i:>2} … ok"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A toolbar strip: children on a raised ground — the simplest
/// children-collection component.
fn toolbar(t: &TokenSet, children: Vec<View>) -> View {
    let ground = t.surface_raised;
    let ink = t.text;
    Element::new()
        .style(LayoutStyle::row().gap(2).h(1).padding(Edges::hv(1, 0)))
        .draw(move |canvas, rect| {
            canvas.fill(rect, ' ', ink, ground);
        })
        .children(children)
        .build()
}
