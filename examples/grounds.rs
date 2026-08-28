//! grounds — panel elevation at 256 colours: the defect, and the fix.
//!
//! Demonstrates: what a theme's five GROUNDS (`bg`, `surface`,
//! `surface_raised`, `overlay`, `shadow_ground`) resolve to when a
//! terminal only has 256 colours, side by side — a plain nearest-entry
//! lookup, which collapses different grounds onto one palette entry in
//! 15 of the 26 built-in themes, against the per-theme ASSIGNMENT the
//! engine now installs, which gives each ground its own entry.
//!
//! Why it is drawn rather than described: the collapse is invisible by
//! construction. Nothing errors, every token is correct, the audit
//! passes at truecolor, and the panel simply has no edge. The middle
//! column here is what your users on a 256-colour terminal were seeing.
//!
//! The `+ panel` row is a CONSUMER-minted ground — a colour the theme
//! has never heard of, declared through `RunConfig::extra_grounds`.
//! Ground separation only protects what it is handed, so an app that
//! mints its own must say so; press `p` to see the difference the
//! declaration makes.
//!
//! **Press `i` for the other half of the story: separating every ground
//! is not always right.** Where a theme AUTHORED two grounds to look
//! alike, giving them their own entries invents an edge nobody drew, and
//! the 256 rendering ends up more separated than truecolor. Walk to
//! `solarized-dark`, `one-light` or `abstract-midnight` and toggle `i`:
//! the theme declares that pair `Same` (`render::color::GroundIntent`)
//! and they go back to one entry — matching what your eye sees in the
//! truecolor column. On the other 23 themes `i` changes nothing, which
//! is the point: the declaration is PER PAIR and additive, so it only
//! ever releases a merge the palette was already forcing.
//!
//! Keys: ↑/↓ walk the themes · p toggle the declared panel ground ·
//! i toggle the theme's ground intent · q quits.
//!
//! Docs: docs/theming.md (Depth downgrade).
//!
//! OWNER: DESIGN.

mod common;

use abstracttui::base::palette::XTERM_256;
use abstracttui::prelude::*;
use abstracttui::render::color::{
    nearest_xterm256, quantize_set_256_into_with, GroundIntent, PairIntent,
};
use abstracttui::theme::contrast::{floors, ink_on};
use abstracttui::theme::{
    register, themes, RegisterMode, Theme, ThemeCandidate, TokenId, TokenSet,
};
use abstracttui::ui::Canvas;

/// A panel ground an app might mint itself: the theme knows nothing
/// about it, so nothing keeps it off another ground's palette entry
/// unless the app declares it.
const PANEL: Rgba = Rgba::rgb(41, 49, 62);

/// A BRIGHT panel an app might mint. It earns its place here twice: it
/// is a second undeclared ground for the separator, and it is the only
/// band on this screen whose readable ink is the OPPOSITE pole from the
/// dark one — so the polarity flip `ink_on` performs is visible on the
/// first screen of every theme, instead of only on the light themes
/// twenty keypresses away.
const PANEL_BRIGHT: Rgba = Rgba::rgb(240, 200, 90);

const LABEL_W: i32 = 16;
const SWATCH_W: i32 = 10;

fn main() -> abstracttui::base::Result<()> {
    if !abstracttui::term::have_tty() {
        println!("grounds: needs an interactive terminal — skipping cleanly");
        return Ok(());
    }
    let mut app = App::new(Size::new(96, 30));
    let quitter = app.quitter();
    app.mount(move |cx| {
        let sel = cx.signal(0usize);
        let declared = cx.signal(true);
        let intent = cx.signal(false);
        let n = themes().len();
        Element::new()
            .style(LayoutStyle::fill())
            .shortcut(KeyChord::plain(Key::Char('q')), move |_| quitter.quit())
            .shortcut(KeyChord::plain(Key::Char('p')), move |_| {
                declared.update(|d| *d = !*d)
            })
            .shortcut(KeyChord::plain(Key::Char('i')), move |_| {
                intent.update(|d| *d = !*d)
            })
            .shortcut(KeyChord::plain(Key::Up), move |_| {
                sel.update(|s| *s = s.saturating_sub(1))
            })
            .shortcut(KeyChord::plain(Key::Down), move |_| {
                sel.update(|s| *s = (*s + 1).min(n - 1))
            })
            // The two signals are read HERE, inside the dyn_view, not
            // inside the draw closure: a tracked read during phase D is
            // a region that never repaints when the value changes, and
            // the engine panics on it in debug builds (RT1-2). The draw
            // closure below receives plain values.
            .child(dyn_view(LayoutStyle::fill(), move || {
                let (sel, declared, intent) = (sel.get(), declared.get(), intent.get());
                // Tell the APP which theme is on screen, so the driver's
                // own palette assignment is the one this page is
                // describing. Without it the whole demo painted theme N
                // while the driver held theme 0's assignment — every
                // number below would have been about a theme the
                // terminal was not being sent.
                //
                // This is also the ONLY thing that makes `i` real: the
                // declared variant is a registered theme, and the driver
                // reads `Theme::ground_intent` off whatever is active.
                let live = active_theme(sel, intent);
                abstracttui::app::set_theme(live);
                Element::new()
                    .style(LayoutStyle::fill())
                    .draw(move |canvas, rect| draw(canvas, rect, live, sel, declared))
                    .build()
            }))
            .build()
    })?;
    // The ONLY door to ground separation on the `App::run` path: the
    // Driver is created inside `run` and never handed out, so the
    // `Driver::set_extra_grounds` setter is unreachable from here. An
    // app declares its grounds up front instead.
    app.run_with(RunConfig {
        extra_grounds: vec![PANEL, PANEL_BRIGHT],
        ..RunConfig::default()
    })
}

/// One printed line: the ground's name, its true colour, the palette
/// entry a plain nearest lookup gives it, and the entry the assignment
/// gives it.
type GroundRow = (String, Rgba, u8, u8);

/// The ground pairs of `t` that land on ONE palette entry before the
/// separator touches anything — the only pairs a declaration can reach,
/// because intent RELEASES a merge and never creates one.
///
/// Declaring exactly these, rather than every pair, is what makes the
/// `i` toggle honest: the theme is saying "where the palette has already
/// forced these two together, leave them", not "merge whatever you
/// like". Every pair it does not name keeps its own entry.
fn colliding_pairs(t: &TokenSet) -> Vec<(TokenId, TokenId, PairIntent)> {
    let g = t.grounds();
    let mut out = Vec::new();
    for i in 0..g.len() {
        for j in (i + 1)..g.len() {
            if nearest_xterm256(g[i].1) == nearest_xterm256(g[j].1) {
                out.push((g[i].0, g[j].0, PairIntent::Same));
            }
        }
    }
    out
}

/// The theme the app should be running: the built-in at `sel`, or — when
/// `intent` is on and that theme has a colliding pair to release — a
/// REGISTERED variant of it that declares the pair `Same`.
///
/// This is the whole point of the `i` key, and it is deliberately not a
/// local calculation: the variant is a real `Theme` carrying a real
/// `ground_intent`, handed to `set_theme`, and the driver is what reads
/// the declaration and installs the assignment. If this example computed
/// the merge itself it would demonstrate `render::color` and prove
/// nothing about whether an app can actually declare anything.
///
/// Built-ins are silent by ruling, so the variant is a separate
/// registration rather than an edit — and `register` dedups a
/// byte-identical candidate, so walking back and forth costs one
/// registration per theme, not one per keypress.
fn active_theme(sel: usize, intent: bool) -> &'static Theme {
    let base = &themes()[sel];
    if !intent {
        return base;
    }
    let pairs = colliding_pairs(&base.tokens);
    if pairs.is_empty() {
        // Nothing to release. Returning the base theme is the honest
        // answer, and the footer says so rather than leaving the key
        // looking broken.
        return base;
    }
    register(
        ThemeCandidate {
            id: format!("{}-ground-intent", base.id),
            label: format!("{} (grounds declared)", base.label),
            dark: base.dark,
            tokens: base.tokens,
            ground_intent: pairs,
        },
        // Labeled, not Strict: this is a byte-for-byte copy of a shipped
        // theme, so any finding it has is the built-in's and refusing
        // would take the demo down over something `i` did not cause.
        RegisterMode::Labeled,
    )
    .map(|r| r.theme)
    .unwrap_or(base)
}

/// Every ground of `theme`, plus the consumer panel, with the count of
/// distinct entries before and after the assignment.
///
/// The intent is read off the LIVE theme rather than recomputed, so the
/// table describes the same declaration the driver is acting on. Get
/// this wrong and the page becomes a second opinion about the screen
/// instead of a reading of it.
fn rows(theme: &'static Theme, declared: bool) -> (Vec<GroundRow>, usize, usize) {
    let t = theme.tokens;
    let mut names: Vec<String> = t
        .grounds()
        .iter()
        .map(|(id, _)| id.name().to_string())
        .collect();
    let mut colors: Vec<Rgba> = t.grounds().iter().map(|(_, c)| *c).collect();
    names.push("+ panel".into());
    colors.push(PANEL);
    names.push("+ panel bright".into());
    colors.push(PANEL_BRIGHT);

    // BEFORE: each ground quantised on its own — what the emitter did
    // before the assignment existed, and what it still does for a
    // ground nobody declared.
    let before: Vec<u8> = colors.iter().map(|c| nearest_xterm256(*c)).collect();

    // AFTER: the set separated together. The consumer panel only joins
    // the set when the app has declared it — undeclared, it keeps its
    // nearest entry and may land on a theme ground's.
    //
    // The theme's INTENT rides the same call. It names only the theme's
    // own grounds: the consumer panels are the app's colours and the
    // theme has no standing to say they read as one surface with
    // anything.
    let set_len = if declared { colors.len() } else { 5 };
    let pairs = TokenSet::resolve_ground_intent(theme.ground_intent)
        .expect("a registered theme's declaration names only grounds");
    let mut after = vec![0u8; set_len];
    quantize_set_256_into_with(&colors[..set_len], GroundIntent::new(&pairs), &mut after);
    if !declared {
        after.push(nearest_xterm256(PANEL));
        after.push(nearest_xterm256(PANEL_BRIGHT));
    }

    let distinct = |v: &[u8]| {
        let mut s: Vec<u8> = v.to_vec();
        s.sort_unstable();
        s.dedup();
        s.len()
    };
    let (nb, na) = (distinct(&before), distinct(&after));
    let out = (0..colors.len())
        .map(|i| (names[i].clone(), colors[i], before[i], after[i]))
        .collect();
    (out, nb, na)
}

/// One ground band, with text ON it in the theme's readable pole.
///
/// The ink is never picked by hand: `ink_on` returns whichever of the
/// theme's two authored poles reads better on `ground`, which is what
/// makes this correct across all 26 themes and both depths rather than
/// on the one the author happened to be looking at.
fn band(canvas: &mut dyn Canvas, x: i32, y: i32, t: &TokenSet, ground: Rgba) {
    canvas.fill(Rect::new(x, y, SWATCH_W, 1), ' ', t.text, ground);
    let ink = ink_on(t, ground);
    canvas.print(Point::new(x + 3, y), "Text", ink.color, Rgba::TRANSPARENT);
}

fn draw(canvas: &mut dyn Canvas, rect: Rect, theme: &'static Theme, sel: usize, declared: bool) {
    let t = theme.tokens;
    if common::too_small(canvas, rect, Size::new(72, 17), &t) {
        return;
    }
    canvas.fill(rect, ' ', t.text, t.bg);
    let (rows, n_before, n_after) = rows(theme, declared);
    let total = rows.len();

    let mut y = rect.y;
    canvas.print(
        Point::new(rect.x + 2, y),
        &format!("{}  ({}/{})", theme.id, sel + 1, themes().len()),
        t.accent,
        Rgba::TRANSPARENT,
    );
    y += 1;
    canvas.print(
        Point::new(rect.x + 2, y),
        "↑/↓ theme · p declared panel ground · i theme ground intent · q quit",
        t.text_faint,
        Rgba::TRANSPARENT,
    );
    y += 2;

    let x_true = rect.x + 2 + LABEL_W;
    let x_before = x_true + SWATCH_W + 2;
    let x_after = x_before + SWATCH_W + 6;
    for (label, x) in [
        ("truecolor", x_true),
        ("nearest (was)", x_before),
        ("assigned (is)", x_after),
    ] {
        canvas.print(Point::new(x, y), label, t.text_muted, Rgba::TRANSPARENT);
    }
    y += 1;

    for (name, color, before, after) in &rows {
        // Six lines are reserved below: blank, verdict, declaration
        // note, the intent note, the repeated-index note, and the
        // readability line.
        if y >= rect.bottom() - 6 {
            break;
        }
        canvas.print(Point::new(rect.x + 2, y), name, t.text, Rgba::TRANSPARENT);
        // The three columns are painted as GROUNDS (a filled band), not
        // as glyph ink: the defect is about backgrounds sitting next to
        // each other, and it only reads when they are.
        //
        // Each band then carries the word `Text`, inked by
        // `theme::contrast::ink_on` against the colour ACTUALLY under it.
        // Two things are on show: that a ground is a surface you can put
        // text on, and that the polarity flips per theme and per band —
        // the dark bands take the light pole, the bright ones the dark.
        // Reaching for `t.text` here instead would be unreadable in 8 of
        // the 26 themes on the declared panel alone.
        band(canvas, x_true, y, &t, *color);
        band(canvas, x_before, y, &t, XTERM_256[*before as usize]);
        canvas.print(
            Point::new(x_before + SWATCH_W + 1, y),
            &format!("{before:>3}"),
            if collides(&rows, *before, |r| r.2) {
                t.error
            } else {
                t.text_faint
            },
            Rgba::TRANSPARENT,
        );
        band(canvas, x_after, y, &t, XTERM_256[*after as usize]);
        canvas.print(
            Point::new(x_after + SWATCH_W + 1, y),
            &format!("{after:>3}"),
            if collides(&rows, *after, |r| r.3) {
                t.error
            } else {
                t.text_faint
            },
            Rgba::TRANSPARENT,
        );
        y += 1;
    }

    y += 1;
    let verdict =
        format!("{n_before} of {total} distinct entries before · {n_after} of {total} after",);
    canvas.print(
        Point::new(rect.x + 2, y),
        &verdict,
        if n_before < total { t.warn } else { t.text },
        Rgba::TRANSPARENT,
    );
    y += 1;
    let note = if declared {
        "panel ground DECLARED (RunConfig::extra_grounds) — it joins the set"
    } else {
        "panel ground NOT declared — the separator was never handed it"
    };
    canvas.print(
        Point::new(rect.x + 2, y),
        note,
        if declared { t.ok } else { t.warn },
        Rgba::TRANSPARENT,
    );
    y += 1;
    // What the LIVE theme declares, read off the theme the driver is
    // running rather than recomputed — and stated as a count, so the 23
    // themes where `i` can do nothing say so plainly instead of looking
    // like a key that did not register.
    let declaring = theme.ground_intent.len();
    let releasable = colliding_pairs(&t).len();
    let intent_note = match (declaring, releasable) {
        (0, 0) => "theme declares nothing — and no pair of its grounds collides, so `i` has nothing to release".into(),
        (0, n) => format!("theme declares nothing — {n} pair(s) the separator is holding apart; press i"),
        (n, _) => format!(
            "LIVE THEME IS `{}`, declaring {n} pair(s) Same — the driver read that off the theme",
            theme.id
        ),
    };
    canvas.print(
        Point::new(rect.x + 2, y),
        &intent_note,
        if declaring > 0 { t.ok } else { t.text_faint },
        Rgba::TRANSPARENT,
    );
    y += 1;
    canvas.print(
        Point::new(rect.x + 2, y),
        "a repeated index in red is two grounds arriving as ONE colour",
        t.text_faint,
        Rgba::TRANSPARENT,
    );
    y += 1;
    // The readability of the text ON those bands, reported rather than
    // assumed. `ink_on` picks the best AUTHORED pole; on a few
    // theme/ground combinations the best available still misses the 4.5
    // floor, and a demo that hid that would be teaching the wrong thing.
    let worst = rows
        .iter()
        .map(|(name, color, _, _)| (name, ink_on(&t, *color).contrast))
        .fold(None::<(&String, f32)>, |acc, (n, c)| match acc {
            Some((_, best)) if best <= c => acc,
            _ => Some((n, c)),
        });
    if let Some((name, c)) = worst {
        let ok = c >= floors::TEXT;
        canvas.print(
            Point::new(rect.x + 2, y),
            &format!(
                "text on grounds: worst is {name} at {c:.2}:1 ({}) — ink_on picks the pole",
                if ok { "clears 4.5" } else { "MISSES 4.5" }
            ),
            if ok { t.text_faint } else { t.error },
            Rgba::TRANSPARENT,
        );
    }
}

/// Does `entry` appear more than once in the column `pick` reads?
fn collides(
    rows: &[(String, Rgba, u8, u8)],
    entry: u8,
    pick: fn(&(String, Rgba, u8, u8)) -> u8,
) -> bool {
    rows.iter().filter(|r| pick(r) == entry).count() > 1
}
