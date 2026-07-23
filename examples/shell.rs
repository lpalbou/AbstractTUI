//! shell — the APP-SHELL demo: a global `PageHost` hosting three full
//! pages (a dashboard-ish overview, a markdown reader, a settings
//! form) behind one themed tab bar, plus edge-anchored drawers.
//!
//! What it proves (app-kits 0545): page switching is a signal + one
//! host; durable page state lives in app-owned signals OUTSIDE the
//! page builders (type into Settings, leave, come back — the draft
//! survives the remount); tab badges update reactively without
//! remounting pages; Ctrl+PgUp/PgDn are container-reserved chords and
//! 1-3 jump directly (opted in).
//!
//! Keys: Ctrl+PgUp/PgDn or click · 1-3 jump · i inspector drawer ·
//! g nav drawer · Tab focus · n raise an alert (watch the Overview
//! badge) · Ctrl+T theme · q quit.
//!
//! CO-OWNED: TABS (page host, this file's shell) + DRAWER (edge
//! drawers — their regions are marked below; additions there stay
//! append-shaped).

use abstracttui::prelude::*;
use abstracttui::theme::themes;
use abstracttui::widgets::Tone;

const READER_DOC: &str = "# The page host\n\n\
Full complex pages behind one themed tab bar. Only the **active**\n\
page is mounted: switching disposes the outgoing page's scope —\n\
its timers and effects die with it (the zero-idle law) — and the\n\
incoming page builds fresh.\n\n\
## Where state lives\n\n\
Durable state belongs in app-owned signals created *outside* the\n\
page builders. This document scrolls; the settings draft survives\n\
switches; the alert count feeds the Overview tab badge.\n\n\
## Navigation\n\n\
- Click a tab, or the overflow indicators when many tabs compress.\n\
- Ctrl+PgUp / Ctrl+PgDn cycle pages (container-reserved chords).\n\
- Digits 1-3 jump (an explicit opt-in — apps own their digits).\n";

fn main() -> abstracttui::base::Result<()> {
    if !abstracttui::term::have_tty() {
        println!("shell: needs an interactive terminal — skipping cleanly");
        return Ok(());
    }
    if let Ok(id) = std::env::var("ABSTRACTTUI_THEME") {
        abstracttui::app::set_theme_by_id(&id);
    }

    let mut app = App::new(Size::new(100, 30));
    let quitter = app.quitter();
    #[allow(unused_variables)]
    let overlays = app.overlays();

    app.mount(move |cx| {
        let theme = use_theme(cx);
        // ------------------------------------------------------------
        // The app-owned store (THE state recipe): everything a page
        // must not lose lives here, OUTSIDE the page builders.
        // ------------------------------------------------------------
        let alerts = cx.signal(2u32);
        let name = cx.signal(String::new());
        let notify = cx.signal(true);
        let density = cx.signal(0usize);
        let theme_ix = cx.signal(0usize);

        // === DRAWER REGION (peer-owned) =============================
        // DRAWER (0585): a right INSPECTOR drawer (modal + scrim,
        // hosting a full scrollable Feed page) and a left NAV drawer
        // (narrower, cells-sized). Both resolve the overlay store from
        // reactive context (App::mount provides it) and are toggled by
        // the global actions registered in the lower drawer region
        // ('i' / 'g'); Esc or ✕ closes. Handles cross to that region
        // through the example-local slot below (mount runs first).
        let inspector =
            abstracttui::app::drawer::Drawer::new(abstracttui::app::drawer::DrawerEdge::Right)
                .size(abstracttui::app::drawer::DrawerSize::Percent(0.45))
                .title("Inspector")
                .install(cx, move |dcx| inspector_page(dcx, theme, alerts));
        let nav = abstracttui::app::drawer::Drawer::new(abstracttui::app::drawer::DrawerEdge::Left)
            .size(abstracttui::app::drawer::DrawerSize::Cells(26))
            .title("Navigate")
            .install(cx, move |dcx| nav_page(dcx, theme));
        DRAWER_HANDLES.with(|slot| *slot.borrow_mut() = Some((inspector, nav)));
        // ============================================================

        let host = PageHost::new()
            .page("overview", "Overview", move |gcx| {
                overview_page(gcx, theme, alerts)
            })
            .page("reader", "Reader", move |gcx| reader_page(gcx, theme))
            .page("settings", "Settings", move |gcx| {
                settings_page(gcx, theme, name, notify, density)
            })
            .badge("overview", move || {
                let n = alerts.get();
                (n > 0).then(|| n.to_string())
            })
            .number_jump(true)
            .view(cx);

        Element::new()
            .style(
                LayoutStyle::column()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .shortcut(KeyChord::plain(Key::Char('q')), move |_| quitter.quit())
            .shortcut(KeyChord::plain(Key::Char('n')), move |_| {
                alerts.update(|n| *n += 1)
            })
            .shortcut(KeyChord::new(Mods::CTRL, Key::Char('t')), move |_| {
                let list = themes();
                let next = (theme_ix.get_untracked() + 1) % list.len();
                theme_ix.set(next);
                abstracttui::app::set_theme(&list[next]);
            })
            .child(host)
            .child(text(
                " Ctrl+PgUp/PgDn pages · 1-3 jump · i inspector · g nav · n alert · Ctrl+T theme · q quit",
            ))
            .build()
    })?;
    // Anchor the keyboard on the shell (the bar is the first focusable)
    // so the page chords answer from frame one.
    app.tree().focus_first();

    // === DRAWER REGION (peer-owned) =================================
    // DRAWER (0585): global toggle keys — 'i' inspector · 'g' nav.
    // Actions sit LAST in key resolution, so a focused widget (the
    // settings TextInput) always wins over these letters; while a
    // modal drawer is open its tree owns the keyboard and Esc/✕ close.
    let _ = overlays; // the drawers resolved the store via context
    DRAWER_HANDLES.with(|slot| {
        if let Some((inspector, nav)) = slot.borrow().clone() {
            app.actions().register(
                "drawer.inspector",
                Some(KeyChord::plain(Key::Char('i'))),
                move || inspector.toggle(),
            );
            app.actions().register(
                "drawer.nav",
                Some(KeyChord::plain(Key::Char('g'))),
                move || nav.toggle(),
            );
        }
    });
    // ================================================================

    app.run()
}

// === DRAWER REGION (peer-owned, appended items) =====================

// Handle handoff between the mount closure (drawers install where the
// scope lives) and the post-mount action registration (where `app`
// lives). Example-local plumbing, not an engine pattern.
thread_local! {
    static DRAWER_HANDLES: std::cell::RefCell<
        Option<(
            abstracttui::app::drawer::DrawerHandle,
            abstracttui::app::drawer::DrawerHandle,
        )>,
    > = const { std::cell::RefCell::new(None) };
}

/// The inspector: a FULL page in a drawer — a live counter line riding
/// the app store plus a scrolling Feed, proving complex pages host
/// unchanged inside the overlay panel.
fn inspector_page(
    cx: Scope,
    theme: Signal<&'static abstracttui::theme::Theme>,
    alerts: Signal<u32>,
) -> View {
    use abstracttui::widgets::{Feed, FeedItem, FeedState};
    let t = theme.get().tokens;
    let feed = FeedState::new(cx);
    feed.push(
        "intro",
        FeedItem::markdown(
            "**Run inspector** — a full page living in a drawer.\n\n\
             Wheel/drag scrolls; the counter below is live while open.",
        ),
    );
    for i in 0..24 {
        feed.push(
            format!("ln{i}"),
            FeedItem::text(format!("event {i:02} · gateway tick accepted")),
        );
    }
    Element::new()
        .style(LayoutStyle::column().gap(1).grow(1.0))
        .child(dyn_view(LayoutStyle::line(1), move || {
            text(format!("open alerts: {} (live)", alerts.get()))
        }))
        .child(
            Scroll::new(Feed::new(&feed).gap(0).view(cx))
                .element(cx, &t)
                .build(),
        )
        .build()
}

/// The nav drawer: a glanceable page list (the entity-app left-rail
/// feel). Esc or ✕ dismisses; 1-3 still jump pages once closed.
fn nav_page(cx: Scope, theme: Signal<&'static abstracttui::theme::Theme>) -> View {
    let _ = (cx, theme);
    Element::new()
        .style(LayoutStyle::column().gap(1).grow(1.0))
        .child(text(
            "Pages\n\n  1 · Overview\n  2 · Reader\n  3 · Settings",
        ))
        .child(text(
            "close me, then press a digit\nto jump — esc or ✕ closes",
        ))
        .build()
}

// ====================================================================

/// Dashboard-ish: status chips + live counters riding the store.
fn overview_page(
    _cx: Scope,
    theme: Signal<&'static abstracttui::theme::Theme>,
    alerts: Signal<u32>,
) -> View {
    let t = theme.get().tokens;
    Block::new()
        .border(BorderKind::Rounded)
        .title("overview")
        .fill(t.surface)
        .layout(LayoutStyle::column().gap(1).grow(1.0))
        .child(
            Element::new()
                .style(LayoutStyle::row().gap(1).height(Dimension::Cells(1)))
                .child(Badge::new("gateway up").tone(Tone::Ok).element(&t).build())
                .child(
                    Badge::new("3 runtimes")
                        .tone(Tone::Info)
                        .element(&t)
                        .build(),
                )
                .child(
                    Badge::new("build green")
                        .tone(Tone::Accent)
                        .element(&t)
                        .build(),
                )
                .build(),
        )
        .child(dyn_view(LayoutStyle::line(1), move || {
            text(format!(
                "open alerts: {}   (press n — the tab badge follows)",
                alerts.get()
            ))
        }))
        .child(text(
            "This page is REBUILT on every visit; the counters above\n\
             live in the app store, so nothing is lost.",
        ))
        .element(&t)
        .build()
}

/// Reader-ish: a scrolling markdown document filling the page.
fn reader_page(cx: Scope, theme: Signal<&'static abstracttui::theme::Theme>) -> View {
    let t = theme.get().tokens;
    Block::new()
        .border(BorderKind::Rounded)
        .title("reader")
        .fill(t.surface)
        .layout(LayoutStyle::column().grow(1.0))
        .child(Scroll::new(MarkdownView::new(READER_DOC).element(&t).build()).view(cx))
        .element(&t)
        .build()
}

/// Settings-ish: a small form whose draft survives page switches
/// because every field signal is app-owned.
fn settings_page(
    cx: Scope,
    theme: Signal<&'static abstracttui::theme::Theme>,
    name: Signal<String>,
    notify: Signal<bool>,
    density: Signal<usize>,
) -> View {
    let t = theme.get().tokens;
    Block::new()
        .border(BorderKind::Rounded)
        .title("settings")
        .fill(t.surface)
        .layout(LayoutStyle::column().gap(1).grow(1.0))
        .child(text("display name (type, switch away, come back):"))
        .child(
            TextInput::new()
                .value(name)
                .placeholder("operator name")
                .element(cx, &t)
                .build(),
        )
        .child(
            Checkbox::new("desktop notifications")
                .checked(notify)
                .element(cx, &t)
                .build(),
        )
        .child(
            RadioGroup::new(vec![
                "comfortable".to_string(),
                "compact".to_string(),
                "dense".to_string(),
            ])
            .selection(density)
            .element(cx, &t)
            .build(),
        )
        .element(&t)
        .build()
}
