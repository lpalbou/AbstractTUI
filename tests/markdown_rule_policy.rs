//! The `---` policy: does a caller actually reach the three axes a
//! horizontal rule spends, and do BOTH renderers of that block obey?
//!
//! Every assertion here is on painted output — `App` + `Driver` +
//! `CaptureTerm` fed to the VT model, colours and glyphs read back off
//! the screen. Not on `MdRuleStyle`'s fields, which would pin the
//! setter and prove nothing about the pixels.
//!
//! Why all three axes in one file rather than a test per axis: the
//! acceptance condition this policy was built against
//! (`decision:thread-shape-in-a-terminal` clause 3, agora-wui's
//! sharpening of it) is that EVERY axis the block spends must be
//! reachable — a policy opening one is worse than none, because a
//! consumer can then ship half an ordinal and believe it is finished.
//! A per-axis test suite passes green while two thirds of the policy is
//! missing; `every_axis_a_rule_spends_is_reachable` cannot.
//!
//! OWNER: DESIGN.

use abstracttui::app::{App, Driver, RunConfig};
use abstracttui::base::{Rgba, Size};
use abstracttui::layout::Style as LayoutStyle;
use abstracttui::term::Capabilities;
use abstracttui::testing::{CaptureTerm, VtScreen};
use abstracttui::theme::{default_theme, TokenId, TokenSet};
use abstracttui::ui::Element;
use abstracttui::widgets::{
    CustomBlock, Feed, FeedBlock, FeedItem, FeedState, MarkdownView, MdRuleInk, MdRuleStyle,
    MdRuleWidth,
};

const W: i32 = 20;
const H: i32 = 10;

/// Truecolor, declared rather than detected: these assertions are about
/// which ink the policy chose, so the 256-cube must not stand between
/// the token and the screen (`adv_headless_caps` is why declaring is
/// not optional).
fn caps() -> RunConfig {
    RunConfig {
        caps: Some(Capabilities::with(|c| {
            c.truecolor = true;
            c.colors_256 = true;
        })),
        probe: false,
        ..RunConfig::default()
    }
}

fn tokens() -> TokenSet {
    default_theme().tokens
}

fn settle(driver: &mut Driver, app: &mut App, term: &mut CaptureTerm) {
    for _ in 0..64 {
        if driver.turn(app, term).expect("turn").idle {
            return;
        }
    }
    panic!("loop failed to settle within 64 turns");
}

/// One painted row: its glyphs, and the fg of every cell.
struct PaintedRow {
    glyphs: String,
    fg: Vec<Option<Rgba>>,
}

impl PaintedRow {
    /// `(first, last)` column carrying a rule glyph, and how many.
    fn rule_span(&self) -> Option<(usize, usize, usize)> {
        let cols: Vec<usize> = self
            .glyphs
            .chars()
            .enumerate()
            .filter(|(_, c)| *c == '─')
            .map(|(i, _)| i)
            .collect();
        Some((*cols.first()?, *cols.last()?, cols.len()))
    }

    fn rule_ink(&self) -> Option<Rgba> {
        let i = self.glyphs.chars().position(|c| c == '─')?;
        self.fg[i]
    }
}

fn screen_rows(term: &mut CaptureTerm) -> Vec<PaintedRow> {
    let mut screen = VtScreen::new(Size::new(W, H));
    screen.feed(term.bytes());
    assert_eq!(screen.unknown_seq_count(), 0, "unmodeled bytes");
    (0..H)
        .map(|y| PaintedRow {
            glyphs: (0..W)
                .map(|x| {
                    screen
                        .cell(x, y)
                        .map(|c| c.display().to_string())
                        .unwrap_or_default()
                })
                .collect(),
            fg: (0..W)
                .map(|x| screen.cell(x, y).and_then(|c| c.paint.fg))
                .collect(),
        })
        .collect()
}

/// Render `source` through a real driver under `rule` and read the
/// screen back.
fn render_markdown(source: &str, rule: MdRuleStyle) -> Vec<PaintedRow> {
    let t = tokens();
    let src = source.to_string();
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut app = App::new(Size::new(W, H));
    app.mount(move |_cx| {
        MarkdownView::new(src.clone())
            .rule_style(rule)
            .layout(LayoutStyle::fill())
            .element(&t)
            .build()
    })
    .expect("mount");
    let mut driver = Driver::new(&mut app, &mut term, caps()).expect("enter");
    settle(&mut driver, &mut app, &mut term);
    screen_rows(&mut term)
}

/// The SAME markdown, as a Feed item, under the same policy.
fn render_feed(source: &str, rule: MdRuleStyle) -> Vec<PaintedRow> {
    let t = tokens();
    let src = source.to_string();
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut app = App::new(Size::new(W, H));
    app.mount(move |cx| {
        let state = FeedState::new(cx);
        state.push("only", FeedItem::markdown(src.clone()));
        Element::new()
            .style(LayoutStyle::fill())
            .child(
                Feed::new(&state)
                    .rule_style(rule)
                    .layout(LayoutStyle::fill())
                    .element(cx, &t)
                    .build(),
            )
            .build()
    })
    .expect("mount");
    let mut driver = Driver::new(&mut app, &mut term, caps()).expect("enter");
    settle(&mut driver, &mut app, &mut term);
    screen_rows(&mut term)
}

fn rule_row(rows: &[PaintedRow]) -> (usize, &PaintedRow) {
    rows.iter()
        .enumerate()
        .find(|(_, r)| r.glyphs.contains('─'))
        .expect("a rule was painted")
}

const DOC: &str = "above\n\n---\n\nbelow";

// ---------------------------------------------------------------
// The default: nothing moved.
// ---------------------------------------------------------------

/// The policy's whole promise to existing consumers. The full suite
/// (2583 tests, every one of which renders markdown through this path)
/// is the broad control; this is the narrow one, stated where a reader
/// of the policy will look for it.
#[test]
fn the_default_policy_paints_exactly_what_the_hardwired_rule_did() {
    let t = tokens();
    let rows = render_markdown(DOC, MdRuleStyle::default());

    assert_eq!(rows[0].glyphs.trim_end(), "above");
    assert_eq!(rows[1].glyphs.trim_end(), "", "one blank row before");
    let (y, rule) = rule_row(&rows);
    assert_eq!(y, 2);
    assert_eq!(rows[3].glyphs.trim_end(), "", "one blank row after");
    assert_eq!(rows[4].glyphs.trim_end(), "below");

    assert_eq!(
        rule.rule_span(),
        Some((0, (W - 1) as usize, W as usize)),
        "full bleed, edge to edge"
    );
    assert_eq!(rule.rule_ink(), Some(t.border), "border ink");
}

// ---------------------------------------------------------------
// The acceptance condition: all three axes, or the policy is not done.
// ---------------------------------------------------------------

/// Ink, width and vertical space, each set to something the hardwired
/// renderer could not produce, each verified on the screen.
///
/// Delete any ONE of the three from `MdRuleStyle::resolve` (or stop
/// consulting it in `push_block` / `draw_rows`) and this goes red. That
/// is the point: it is the test a half-shipped policy cannot pass.
#[test]
fn every_axis_a_rule_spends_is_reachable() {
    let t = tokens();
    let quiet = MdRuleStyle::default()
        .ink(MdRuleInk::Token(TokenId::TextFaint))
        .width(MdRuleWidth::Inset(3))
        .space(0, 0);
    let rows = render_markdown(DOC, quiet);

    // SPACE: three rows collapse to one, and the neighbours close up.
    assert_eq!(rows[0].glyphs.trim_end(), "above");
    let (y, rule) = rule_row(&rows);
    assert_eq!(y, 1, "no leading blank");
    assert_eq!(rows[2].glyphs.trim_end(), "below", "no trailing blank");

    // WIDTH: inset by 3 on each side of the measure.
    assert_eq!(
        rule.rule_span(),
        Some((3, (W - 4) as usize, (W - 6) as usize)),
        "inset 3 cells on each side"
    );

    // INK: the faint token, not `border`.
    assert_eq!(rule.rule_ink(), Some(t.text_faint));
    assert_ne!(
        rule.rule_ink(),
        Some(t.border),
        "premise: the two tokens differ in this theme"
    );
}

/// A `Fixed` ink reaches the screen verbatim — the deliberate opt-out
/// of theme-following, for a consumer that has already resolved its own
/// colour.
#[test]
fn a_fixed_ink_reaches_the_screen_unchanged() {
    let ink = Rgba::rgb(200, 30, 90);
    let rows = render_markdown(DOC, MdRuleStyle::default().ink(MdRuleInk::Fixed(ink)));
    assert_eq!(rule_row(&rows).1.rule_ink(), Some(ink));
}

/// A token ink is resolved against the LIVE theme at typeset, so the
/// same policy paints a different colour under a different theme. A
/// policy that snapshotted `Rgba` at build time would pass every test
/// above and freeze the rule on the first theme it ever saw.
#[test]
fn a_token_ink_follows_a_theme_change() {
    let style = MdRuleStyle::default().ink(MdRuleInk::Token(TokenId::Accent));
    // Two themes whose accents actually differ, found in the registry
    // rather than assumed: `abstract-dark` and `abstract-light` share
    // one accent, which made the first version of this test pass its
    // assertion and prove nothing.
    let base = abstracttui::theme::get("abstract-dark").expect("house default");
    let other = abstracttui::theme::themes()
        .iter()
        .find(|th| th.tokens.accent != base.tokens.accent)
        .expect("some registry theme differs in accent");
    let mut seen = Vec::new();
    for t in [base.tokens, other.tokens] {
        let src = DOC.to_string();
        let mut term = CaptureTerm::new(Size::new(W, H));
        let mut app = App::new(Size::new(W, H));
        app.mount(move |_cx| {
            MarkdownView::new(src.clone())
                .rule_style(style)
                .layout(LayoutStyle::fill())
                .element(&t)
                .build()
        })
        .expect("mount");
        let mut driver = Driver::new(&mut app, &mut term, caps()).expect("enter");
        settle(&mut driver, &mut app, &mut term);
        let rows = screen_rows(&mut term);
        assert_eq!(rule_row(&rows).1.rule_ink(), Some(t.accent));
        seen.push(t.accent);
    }
    assert_ne!(seen[0], seen[1], "premise: the two accents differ");
}

// ---------------------------------------------------------------
// What the policy must NOT reach.
// ---------------------------------------------------------------

/// The level-1 heading underline paints the same chrome as `---` and is
/// a DIFFERENT block. One setter must not silently restyle two block
/// kinds — the drift this policy exists to prevent, arriving inside the
/// policy itself.
#[test]
fn the_rule_policy_leaves_the_h1_underline_alone() {
    let t = tokens();
    let loud = MdRuleStyle::default()
        .ink(MdRuleInk::Fixed(Rgba::rgb(255, 0, 0)))
        .width(MdRuleWidth::Inset(4))
        .space(0, 0);
    let rows = render_markdown("# Title\n\n---\n\nbelow", loud);

    // Row 0 is the heading text, row 1 its underline: untouched.
    let underline = &rows[1];
    assert_eq!(
        underline.rule_span(),
        Some((0, (W - 1) as usize, W as usize)),
        "the h1 underline is still full bleed"
    );
    assert_eq!(
        underline.rule_ink(),
        Some(t.border),
        "the h1 underline still uses border ink"
    );

    // ...while the `---` below it did move. It sits at row 2, not 3:
    // between two adjacent rules the FOLLOWING rule's `space_before`
    // wins over the preceding one's `space_after` — the more specific
    // of two declarations about the same gap, and the case an h1
    // immediately above a `---` is the only way to reach.
    let hr = &rows[2];
    assert_eq!(
        hr.rule_span(),
        Some((4, (W - 5) as usize, (W - 8) as usize))
    );
    assert_eq!(hr.rule_ink(), Some(Rgba::rgb(255, 0, 0)));
}

/// A rule inset past its own measure would paint NOTHING — the silent
/// no-op `decision:panel-ground-ownership` clause 4 names. The renderer
/// will not manufacture one: absence is a legitimate answer only when
/// the caller DELETES the rule, never a side effect of arithmetic.
#[test]
fn an_inset_wider_than_the_measure_still_paints_a_rule() {
    let rows = render_markdown(DOC, MdRuleStyle::default().width(MdRuleWidth::Inset(999)));
    let (_, rule) = rule_row(&rows);
    let (_, _, n) = rule.rule_span().expect("still painted");
    assert!(
        n >= 1,
        "a rule that paints nothing is the defect, not a value"
    );
}

// ---------------------------------------------------------------
// Row positions: the fold the app reads must be the fold it sees.
// ---------------------------------------------------------------

/// A document ENDING in a rule still ends at the rule. The trailing gap
/// is spent by the block that follows, so there is no block to spend it
/// — as before the policy existed.
#[test]
fn a_document_ending_in_a_rule_gains_no_trailing_blank() {
    let t = tokens();
    assert_eq!(MarkdownView::rows("above\n\n---", &t, W), 3);
    assert_eq!(
        MarkdownView::rows_ruled("above\n\n---", &t, W, MdRuleStyle::default().space(0, 0)),
        2
    );
}

/// The scroll clamp moves with the policy, and `rows_ruled` is how a
/// styled view asks for it. Taking the clamp from the default fold
/// while rendering a tight one is exactly the drift the doc comment on
/// `rows` warns about — here it is, as a number.
#[test]
fn the_row_count_moves_with_the_policy_and_rows_ruled_tracks_it() {
    let t = tokens();
    let tight = MdRuleStyle::default().space(0, 0);
    let painted = render_markdown(DOC, tight);
    let painted_rows = painted
        .iter()
        .filter(|r| !r.glyphs.trim_end().is_empty())
        .count();

    assert_eq!(MarkdownView::rows(DOC, &t, W), 5, "the default fold");
    assert_eq!(MarkdownView::rows_ruled(DOC, &t, W, tight), 3);
    assert_eq!(painted_rows, 3, "and 3 is what reaches the screen");
}

/// A TOC jump under a policy lands on the heading, not one row off.
///
/// This is the mirror that `separator_rows` replaced: the doc fold used
/// to predict the separator as "one blank row" and locate every heading
/// from that guess. With a rule's `space_after` set to anything but 1,
/// the guess is wrong and the jump lands in the wrong place — silently,
/// because a TOC that scrolls one row short looks like taste.
#[test]
fn a_heading_after_a_rule_reports_the_row_it_is_painted_on() {
    let t = tokens();
    let doc = "intro\n\n---\n\n# Target\n\nbody";
    for style in [
        MdRuleStyle::default(),
        MdRuleStyle::default().space(0, 0),
        MdRuleStyle::default().space(2, 3),
    ] {
        let entries = MarkdownView::outline_rows_ruled(doc, &t, W, style);
        let reported = entries.first().expect("one heading").row;
        let rows = render_markdown(doc, style);
        let painted = rows
            .iter()
            .position(|r| r.glyphs.trim_end() == "Target")
            .expect("the heading is on screen");
        assert_eq!(
            reported, painted,
            "outline row {reported} but painted at {painted} for {style:?}"
        );
    }
}

// ---------------------------------------------------------------
// Two renderers, one rule.
// ---------------------------------------------------------------

/// The Feed and the MarkdownView typeset through one recipe, so they
/// must obey one policy. A rule that read differently inside a feed
/// item and inside a document would be this policy's own defect
/// repeated one widget over — and the Feed has its own segment
/// boundaries that spend separators the typesetter cannot see.
#[test]
fn the_feed_and_the_markdown_view_paint_the_same_rule() {
    for style in [
        MdRuleStyle::default(),
        MdRuleStyle::default()
            .ink(MdRuleInk::Token(TokenId::TextFaint))
            .width(MdRuleWidth::Inset(2))
            .space(0, 0),
    ] {
        let doc = render_markdown(DOC, style);
        let feed = render_feed(DOC, style);
        let (dy, drow) = rule_row(&doc);
        let (fy, frow) = rule_row(&feed);
        assert_eq!(dy, fy, "same row for {style:?}");
        assert_eq!(drow.glyphs, frow.glyphs, "same extent for {style:?}");
        assert_eq!(drow.rule_ink(), frow.rule_ink(), "same ink for {style:?}");
    }
}

/// The stream's closed/open segment boundary spends the POLICY's gap.
///
/// `push_doc_block` cannot see across a segment, so the feed spends
/// that separator itself — it used to spend a hardwired single blank
/// row. With a rule as the last CLOSED block, that row is the rule's
/// `space_after`, and a hardwired one puts a streaming message one row
/// taller than the same message re-rendered after it settles: the
/// content jumping as it streams, which reads as a bug in the app.
///
/// Rendered mid-stream deliberately (no `stream_finish`) — once every
/// block is closed the boundary is not crossed and the defect hides.
#[test]
fn a_streaming_item_spends_the_policys_gap_at_the_segment_boundary() {
    let t = tokens();
    let tight = MdRuleStyle::default().space(0, 0);
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut app = App::new(Size::new(W, H));
    app.mount(move |cx| {
        let state = FeedState::new(cx);
        state.push_stream("s");
        state.stream_append("s", DOC);
        Element::new()
            .style(LayoutStyle::fill())
            .child(
                Feed::new(&state)
                    .rule_style(tight)
                    .layout(LayoutStyle::fill())
                    .element(cx, &t)
                    .build(),
            )
            .build()
    })
    .expect("mount");
    let mut driver = Driver::new(&mut app, &mut term, caps()).expect("enter");
    settle(&mut driver, &mut app, &mut term);
    let streamed = screen_rows(&mut term);

    let settled = render_markdown(DOC, tight);
    for y in 0..3 {
        assert_eq!(
            streamed[y].glyphs, settled[y].glyphs,
            "row {y}: streaming and settled renders disagree"
        );
    }
}

/// The gap in front of a CUSTOM block, when a rule is what precedes it.
///
/// The feed's rhythm between item blocks is the feed's own — except
/// immediately after a rule, where the rule's `space_after` wins,
/// because that gap is the rule's. This boundary lives in a third place
/// (`typeset_static`'s custom arm, which takes the rows away into their
/// own segment first), and a hardwired blank there would let a tight
/// rule still cost two rows in exactly one arrangement.
#[test]
fn a_custom_block_after_a_rule_takes_the_rules_gap() {
    let t = tokens();
    let tight = MdRuleStyle::default().space(0, 0);
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut app = App::new(Size::new(W, H));
    app.mount(move |cx| {
        let state = FeedState::new(cx);
        state.push(
            "only",
            FeedItem::new()
                .block(FeedBlock::Markdown("above\n\n---".into()))
                .block(FeedBlock::Custom(CustomBlock::new(
                    |_| 1,
                    move |canvas, rect| {
                        canvas.print(rect.origin(), "custom", Rgba::WHITE, Rgba::TRANSPARENT);
                    },
                ))),
        );
        Element::new()
            .style(LayoutStyle::fill())
            .child(
                Feed::new(&state)
                    .rule_style(tight)
                    .layout(LayoutStyle::fill())
                    .element(cx, &t)
                    .build(),
            )
            .build()
    })
    .expect("mount");
    let mut driver = Driver::new(&mut app, &mut term, caps()).expect("enter");
    settle(&mut driver, &mut app, &mut term);
    let rows = screen_rows(&mut term);

    assert_eq!(rows[0].glyphs.trim_end(), "above");
    assert!(rows[1].glyphs.contains('─'), "the rule");
    assert_eq!(
        rows[2].glyphs.trim_end(),
        "custom",
        "a tight rule leaves no gap before the custom block"
    );
}
