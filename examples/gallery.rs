//! gallery — the whole design system on one board.
//!
//! One screen, one keypress: every token (grounds, text tiers, semantic
//! and syntax inks, selection pair, chart ramp), every visual widget in
//! its states — including the choice controls (`Select`) and the
//! multiline composer (`TextArea`) — live charts, a highlighted code
//! block, a diff-tinted patch and a rich markdown sample — all restyling
//! together through the one theme signal. This is the design-system
//! screenshot AND the visual-regression surface: any token or widget
//! drift shows up here first.
//!
//! Keys: t / T next / prev theme · Tab focus · Enter/Space activate ·
//! q quit.
//!
//! OWNER: DESIGN.

mod common;

use abstracttui::prelude::*;
use abstracttui::theme::themes;
use abstracttui::ui::Canvas;
use abstracttui::widgets::{
    BarChart, Button, Checkbox, CodeView, LineChart, MarkdownView, Sparkline, SpinnerKind,
    TextInput, TitleAlign, Tone,
};

const CODE_SAMPLE: &str = "// tokens, not hex\nfn theme(id: &str) -> u32 {\n    let floor = 4.5;\n    resolve(id, \"dark\", floor)\n}";

const DIFF_SAMPLE: &str =
    "@@ -1,2 +1,2 @@ fn theme()\n-let floor = 4.4;\n+let floor = 4.5;\n resolve(id)";

const MD_SAMPLE: &str = "## Rich text\nBody with **bold**, *italic*, `code` and a [link](x).\n> quotes recede politely\n- lists mark with accent_alt";

fn main() -> abstracttui::base::Result<()> {
    if !abstracttui::term::have_tty() {
        println!("gallery: needs an interactive terminal — skipping cleanly");
        return Ok(());
    }
    let mut app = App::new(Size::new(112, 38));
    let quitter = app.quitter();
    app.mount(move |cx| {
        let theme = use_theme(cx);
        let viewport = use_viewport(cx);
        let theme_ix = cx.signal(0usize);
        let agree = cx.signal(true);
        let name = cx.signal(String::new());
        let spin = cx.signal(0u64);
        let channel = cx.signal(0usize);
        // Two seeded lines so the composer's multiline nature is visible
        // on the still (a one-row TextArea reads as a TextInput).
        let composer = TextAreaState::new(cx);
        composer.set_text("a multiline composer —\nit grows with content");

        let step_theme = move |delta: i64| {
            let n = themes().len() as i64;
            theme_ix.update(|i| *i = ((*i as i64 + delta).rem_euclid(n)) as usize);
            set_theme_by_id(themes()[theme_ix.get_untracked()].id);
        };

        Element::new()
            .style(LayoutStyle::column().padding(Edges::all(1)).gap(1))
            .shortcut(KeyChord::plain(Key::Char('q')), move |_| quitter.quit())
            .shortcut(KeyChord::plain(Key::Char('t')), move |_| step_theme(1))
            .shortcut(KeyChord::plain(Key::Char('T')), move |_| step_theme(-1))
            .shortcut(KeyChord::plain(Key::Char(' ')), move |_| {
                spin.update(|s| *s += 1)
            })
            .child(dyn_view_scoped(
                LayoutStyle::default().grow(1.0),
                move |gcx| {
                    let th = theme.get();
                    let t = th.tokens;
                    // Responsive: the content column needs real width to be
                    // worth reading — below ~104 cols it bows out and the
                    // remaining panels breathe (resize story).
                    let wide = viewport.get().w >= 104;
                    let mut board = Element::new()
                        .style(LayoutStyle::row().gap(1).grow(1.0))
                        .child(tokens_panel(&t))
                        .child(widgets_panel(
                            gcx, &t, agree, name, spin, channel, &composer,
                        ));
                    if wide {
                        board = board.child(content_panel(&t));
                    }
                    Element::new()
                        .style(LayoutStyle::column().gap(1))
                        .child(header(&t, th.label, th.is_dark()))
                        .child(board.build())
                        .child(text(
                            "t/T theme · tab focus · enter/space activate · space spin · q quit",
                        ))
                        .build()
                },
            ))
            .build()
    })?;
    app.run()
}

fn header(t: &TokenSet, label: &'static str, dark: bool) -> View {
    let tokens = *t;
    Element::new()
        .style(LayoutStyle::row().gap(2).h(2))
        .child(Logo::new().tagline(true).element(t).build())
        .child(
            Badge::new(format!(
                "{label}{}",
                if dark { " · dark" } else { " · light" }
            ))
            .tone(Tone::Accent)
            .element(t)
            .build(),
        )
        .child(
            Element::new()
                .style(LayoutStyle::default().grow(1.0).h(1))
                .draw(move |canvas, rect| {
                    // Registry strip: one dot per theme, the active one lit.
                    let mut x = rect.right() - themes().len() as i32 * 2;
                    for entry in themes() {
                        let on = entry.id == current_theme().id;
                        let ink = if on { tokens.accent } else { tokens.text_faint };
                        canvas.print(
                            Point::new(x, rect.y),
                            if on { "●" } else { "·" },
                            ink,
                            Rgba::TRANSPARENT,
                        );
                        x += 2;
                    }
                })
                .build(),
        )
        .build()
}

/// Column 1 — the raw vocabulary: grounds, tiers, semantics, selection,
/// chart ramp, syntax inks.
fn tokens_panel(t: &TokenSet) -> View {
    let tokens = *t;
    Block::new()
        .title("tokens")
        .fill(t.surface)
        .shadow(t.shadow_ground)
        .layout(LayoutStyle::column().w(34))
        .child(
            Element::new()
                .style(LayoutStyle::default().grow(1.0))
                .draw(move |canvas, rect| draw_tokens(canvas, rect, &tokens))
                .build(),
        )
        .element(t)
        .build()
}

fn draw_tokens(canvas: &mut dyn Canvas, rect: Rect, t: &TokenSet) {
    let mut y = rect.y;
    let x = rect.x;
    let label = |canvas: &mut dyn Canvas, y: i32, s: &str| {
        canvas.print(Point::new(x, y), s, t.text_faint, Rgba::TRANSPARENT);
    };

    label(canvas, y, "grounds");
    y += 1;
    for (name, ground) in [
        ("bg", t.bg),
        ("surface", t.surface),
        ("raised", t.surface_raised),
    ] {
        let mut cx = x;
        cx += canvas.print(Point::new(cx, y), "██", ground, Rgba::TRANSPARENT);
        canvas.print(Point::new(cx + 1, y), name, t.text_muted, Rgba::TRANSPARENT);
        // Text tiers demoed ON that ground, right-aligned.
        let sample = "Aa Aa Aa";
        let sx = rect.right() - sample.chars().count() as i32 - 1;
        let mut px = sx;
        for tier in [t.text, t.text_muted, t.text_faint] {
            px += canvas.print(Point::new(px, y), "Aa", tier, ground);
            px += canvas.print(Point::new(px, y), " ", tier, ground);
        }
        y += 1;
    }

    y += 1;
    label(canvas, y, "semantics");
    y += 1;
    let mut cx = x;
    for (name, ink) in [
        ("ok", t.ok),
        ("wrn", t.warn),
        ("err", t.error),
        ("inf", t.info),
    ] {
        cx += canvas.print(Point::new(cx, y), "●", ink, Rgba::TRANSPARENT);
        cx += canvas.print(Point::new(cx, y), name, t.text_muted, Rgba::TRANSPARENT);
        cx += 1;
    }
    y += 1;
    let mut cx = x;
    cx += canvas.print(Point::new(cx, y), "accent ", t.accent, Rgba::TRANSPARENT);
    cx += canvas.print(Point::new(cx, y), "alt ", t.accent_alt, Rgba::TRANSPARENT);
    cx += canvas.print(Point::new(cx, y), "link ", t.link, Rgba::TRANSPARENT);
    canvas.print(Point::new(cx, y), " sel ", t.selection_fg, t.selection_bg);
    y += 2;

    label(canvas, y, "chart ramp");
    y += 1;
    let mut cx = x;
    for i in 0..8 {
        cx += canvas.print(Point::new(cx, y), "▅▇", t.chart(i), Rgba::TRANSPARENT);
        cx += 1;
    }
    y += 2;

    label(canvas, y, "syntax (on raised)");
    y += 1;
    let ground = t.surface_raised;
    canvas.fill(Rect::new(x, y, rect.w.min(30), 2), ' ', t.text, ground);
    let mut cx = x + 1;
    for (word, ink) in [
        ("fn", t.syntax_keyword),
        (" name", t.syntax_func),
        ("(", t.syntax_punct),
        ("\"str\"", t.syntax_string),
        (", ", t.syntax_punct),
        ("42", t.syntax_number),
        (")", t.syntax_punct),
    ] {
        cx += canvas.print(Point::new(cx, y), word, ink, ground);
    }
    canvas.print(
        Point::new(x + 1, y + 1),
        "// comments recede",
        t.syntax_comment,
        ground,
    );
    y += 3;

    // Border weights + focus ring economy in one line.
    label(canvas, y, "border · border_focus");
    y += 1;
    let mut cx = x;
    cx += canvas.print(Point::new(cx, y), "────", t.border, Rgba::TRANSPARENT);
    cx += canvas.print(Point::new(cx, y), "  ", t.border, Rgba::TRANSPARENT);
    canvas.print(Point::new(cx, y), "────", t.border_focus, Rgba::TRANSPARENT);
}

/// Column 2 — widgets in their states (§3.2 rendered live).
fn widgets_panel(
    gcx: Scope,
    t: &TokenSet,
    agree: Signal<bool>,
    name: Signal<String>,
    spin: Signal<u64>,
    channel: Signal<usize>,
    composer: &TextAreaState,
) -> View {
    let mut badges = Element::new().style(LayoutStyle::row().gap(1).h(1));
    for (tone, label) in [
        (Tone::Accent, "accent"),
        (Tone::Ok, "ok"),
        (Tone::Warn, "warn"),
        (Tone::Error, "err"),
        (Tone::Info, "info"),
        (Tone::Muted, "muted"),
    ] {
        badges = badges.child(Badge::new(label).tone(tone).element(t).build());
    }

    Block::new()
        .title("widgets")
        .fill(t.surface)
        .shadow(t.shadow_ground)
        .layout(
            LayoutStyle::column()
                .gap(1)
                .grow(1.2)
                .basis(Dimension::Cells(0)),
        )
        .child(badges.build())
        .child(
            Element::new()
                .style(LayoutStyle::row().gap(1).h(1))
                .child(Button::new("action").element(gcx, t).build())
                .child(
                    Button::new("disabled")
                        .disabled(true)
                        .element(gcx, t)
                        .build(),
                )
                .build(),
        )
        .child(
            TextInput::new()
                .value(name)
                .placeholder("focus me, type…")
                .layout(LayoutStyle::default().h(1))
                .element(gcx, t)
                .build(),
        )
        // The 0500 choice family (Select face) and the 0120 multiline
        // composer share the TextInput frame vocabulary — the parity
        // is the point of showing them side by side.
        .child(
            Select::new(vec![
                SelectOption::new("stable").hint("lts"),
                SelectOption::new("beta"),
                SelectOption::new("nightly").hint("daily"),
            ])
            .value(channel)
            .layout(LayoutStyle::default().h(1).shrink(0.0))
            .view(gcx),
        )
        .child(
            TextArea::new()
                .state(composer)
                .rows(1, 2)
                .placeholder("multiline composer…")
                .element(gcx, t)
                .build(),
        )
        .child(
            Checkbox::new("selection pair on focus")
                .checked(agree)
                .element(gcx, t)
                .build(),
        )
        .child(Separator::horizontal().label("progress").element(t).build())
        .child(Progress::new(0.35).element(t).build())
        .child(Progress::new(0.72).ramp(true).element(t).build())
        .child(Progress::new(0.93).ramp(true).element(t).build())
        .child(dyn_view(LayoutStyle::row().gap(2).h(1), move || {
            let frame = spin.get();
            let t = current_theme().tokens;
            Element::new()
                .style(LayoutStyle::row().gap(2))
                .child(
                    abstracttui::widgets::Spinner::new()
                        .kind(SpinnerKind::Braille)
                        .frame(frame)
                        .label("braille")
                        .element(&t)
                        .build(),
                )
                .child(
                    abstracttui::widgets::Spinner::new()
                        .kind(SpinnerKind::Line)
                        .frame(frame)
                        .label("line")
                        .element(&t)
                        .build(),
                )
                .build()
        }))
        .child(
            Block::new()
                .border(BorderKind::Rounded)
                .title("focused pane")
                .title_align(TitleAlign::Left)
                .focused(true)
                .layout(LayoutStyle::default().h(3))
                .child(text("border_focus ring"))
                .element(t)
                .build(),
        )
        .element(t)
        .build()
}

/// Column 3 — content: charts, code, markdown.
fn content_panel(t: &TokenSet) -> View {
    let rx: Vec<f32> = (0..48)
        .map(|i| 50.0 + 34.0 * ((i as f32) * 0.23).sin())
        .collect();
    let tx: Vec<f32> = (0..48)
        .map(|i| 38.0 + 22.0 * ((i as f32) * 0.31 + 1.4).sin())
        .collect();
    let bars: Vec<f32> = (0..8).map(|i| 0.25 + 0.09 * i as f32).collect();
    let spark: Vec<f32> = (0..40).map(|i| ((i as f32) * 0.4).sin()).collect();

    Block::new()
        .title("content")
        .fill(t.surface)
        .shadow(t.shadow_ground)
        .layout(
            LayoutStyle::column()
                .gap(1)
                .grow(1.0)
                .basis(Dimension::Cells(0)),
        )
        .child(
            LineChart::new(vec![rx, tx])
                .range(0.0, 100.0)
                .layout(LayoutStyle::default().h(6))
                .element(t)
                .build(),
        )
        .child(
            Element::new()
                .style(LayoutStyle::row().gap(1).h(2))
                .child(
                    BarChart::new(bars)
                        .bar(2, 1)
                        .layout(LayoutStyle::default().grow(1.0))
                        .element(t)
                        .build(),
                )
                .child(
                    Sparkline::new(spark)
                        .slot(6)
                        .layout(LayoutStyle::default().grow(1.0).h(1))
                        .element(t)
                        .build(),
                )
                .build(),
        )
        .child(
            CodeView::new(CODE_SAMPLE)
                .layout(LayoutStyle::default().h(5))
                .element(t)
                .build(),
        )
        // The diff mapping (0140): added/removed/hunk ride the audited
        // semantic inks — state, not syntax.
        .child(
            CodeView::new(DIFF_SAMPLE)
                .lang("diff")
                .line_numbers(false)
                .layout(LayoutStyle::default().h(4))
                .element(t)
                .build(),
        )
        .child(
            MarkdownView::new(MD_SAMPLE)
                .layout(LayoutStyle::default().grow(1.0))
                .element(t)
                .build(),
        )
        .element(t)
        .build()
}
