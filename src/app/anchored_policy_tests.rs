//! Trigger position policies (first-app/0292) and panel placement
//! bias (first-app/0294), driven through the parent module's real rig
//! (mounted composer, real overlay store, driver-order routing) plus
//! the pure `place_panel_biased` contract. Child module of
//! `anchored_tests` (shares its `Rig`/`completion_rig_with`).

use super::*;

/// The parent rig's '/' command provider, minus the Anywhere policy —
/// policy cases register it under the position they exercise.
fn command_candidates(query: &str) -> Vec<CompletionCandidate> {
    ["help", "theme", "clear", "quit"]
        .iter()
        .filter(|c| c.starts_with(query))
        .map(|c| CompletionCandidate::new(format!("/{c}"), format!("/{c} ")).detail("cmd"))
        .collect()
}

/// A rig whose '/' trigger carries `at` and whose '@' trigger stays
/// `Anywhere` — the filed pairing (commands vs mentions).
fn policy_rig(size: Size, at: TriggerPosition) -> Rig {
    completion_rig_with(size, false, move |c, q| {
        c.trigger_at('/', at, move |query| {
            q.borrow_mut().push(query.to_string());
            command_candidates(query)
        })
        .trigger('@', |query| {
            ["alice", "bob"]
                .iter()
                .filter(|c| c.starts_with(query))
                .map(|c| CompletionCandidate::new(format!("@{c}"), format!("@{c} ")))
                .collect()
        })
        .max_visible(3)
    })
}

/// Alt+Enter — the composer's insert-newline chord on every wire.
fn newline(rig: &mut Rig) {
    rig.send(&crate::ui::UiEvent::Key(KeyEvent::new(
        Key::Enter,
        Mods::ALT,
    )));
}

// ------------------------------------------------- 0292: positions

#[test]
fn start_of_input_fires_only_for_the_drafts_first_token() {
    let mut rig = policy_rig(Size::new(40, 10), TriggerPosition::StartOfInput);
    // The filed defect shape: a mid-sentence path fragment must not
    // open a dropdown — and the provider is never even consulted
    // (the app-side workaround had to run the provider to refuse).
    rig.type_str("check /mo");
    assert!(rig.panel().is_none(), "mid-sentence '/' never opens");
    assert!(rig.queries.borrow().is_empty(), "provider not consulted");
    // Mentions keep firing mid-text in the same draft.
    rig.type_str(" @a");
    assert!(rig.panel().is_some(), "'@' stays Anywhere");
    rig.key(Key::Escape);

    // The draft's first token DOES fire…
    rig.state.set_text("");
    flush_effects();
    rig.type_str("/h");
    let (_, rows) = rig.panel().expect("draft-start '/' opens");
    assert!(rows.iter().any(|r| r.contains("/help")), "{rows:?}");
    rig.key(Key::Escape);

    // …and leading whitespace is tolerated ("first token", not
    // "byte zero" — the consumer's own trim_start convention).
    rig.state.set_text("");
    flush_effects();
    rig.type_str("  /h");
    assert!(rig.panel().is_some(), "whitespace-led first token fires");
}

#[test]
fn start_of_line_vs_start_of_input_differ_on_multiline_drafts() {
    // Same draft driven into both policies: "hello⏎/h".
    let mut line_rig = policy_rig(Size::new(40, 10), TriggerPosition::StartOfLine);
    line_rig.type_str("hello");
    newline(&mut line_rig);
    line_rig.type_str("/h");
    assert!(
        line_rig.panel().is_some(),
        "StartOfLine: a second line's first token fires"
    );
    line_rig.key(Key::Escape);
    // Mid-line on the second line: not a line start.
    newline(&mut line_rig);
    line_rig.type_str("say /h");
    assert!(
        line_rig.panel().is_none(),
        "StartOfLine: mid-line tokens stay quiet"
    );

    let mut input_rig = policy_rig(Size::new(40, 10), TriggerPosition::StartOfInput);
    input_rig.type_str("hello");
    newline(&mut input_rig);
    input_rig.type_str("/h");
    assert!(
        input_rig.panel().is_none(),
        "StartOfInput: only the DRAFT's first token, never line two"
    );
    assert!(input_rig.queries.borrow().is_empty(), "never consulted");
}

#[test]
fn positioned_trigger_refilters_dismisses_and_accepts_unchanged() {
    let mut rig = policy_rig(Size::new(40, 10), TriggerPosition::StartOfInput);
    // Refilter: the provider sees the growing query, one call per edit.
    rig.type_str("/th");
    assert!(rig.panel().is_some());
    assert_eq!(
        rig.queries.borrow().as_slice(),
        ["", "t", "th"],
        "one provider call per edit through the policy gate"
    );
    // Escape mutes the token; typing inside it stays calm.
    rig.key(Key::Escape);
    assert!(rig.panel().is_none(), "Escape closes");
    rig.type_str("e");
    assert!(rig.panel().is_none(), "dismissed token stays muted");
    // A LATER token is not at the input start: the policy (not the
    // mute) keeps it closed — the two gates compose.
    rig.type_str(" /q");
    assert!(rig.panel().is_none(), "second token fails StartOfInput");
    // A fresh first-token trigger accepts end-to-end.
    rig.state.set_text("");
    flush_effects();
    rig.type_str("/t");
    assert!(rig.panel().is_some());
    rig.key(Key::Enter);
    assert_eq!(rig.state.text(), "/theme ", "accept replaces the token");
    assert!(rig.panel().is_none(), "accept closes");
}

// ------------------------------------------- 0294: placement bias

#[test]
fn place_panel_biased_above_preferred_mirrors_the_rule() {
    let vp = Size::new(80, 24);
    let width = PanelWidth::Content { min: 8, max: 44 };
    let above = PanelPlacement::AbovePreferred;
    // The filed shape: one row exists below the anchor (the status
    // bar), the content is SHORT — classic placement parks on it;
    // the above bias keeps the panel over the caret instead.
    let anchor = Rect::new(10, 22, 1, 1);
    let content = Size::new(20, 1);
    assert_eq!(
        place_panel(vp, anchor, content, width),
        Rect::new(10, 23, 20, 1),
        "classic rule: a short list 'fits' on the chrome row below"
    );
    assert_eq!(
        place_panel_biased(vp, anchor, content, width, above),
        Rect::new(10, 21, 20, 1),
        "above bias: the short list sits over the anchor"
    );
    // Viewport-edge flip still works mirrored: an anchor on the TOP
    // row has no room above, so the panel falls below.
    let anchor = Rect::new(10, 0, 1, 1);
    assert_eq!(
        place_panel_biased(vp, anchor, Size::new(20, 4), width, above),
        Rect::new(10, 1, 20, 4),
        "no room above = fall below"
    );
    // Cramped both sides: the LONGER side wins, height clamped —
    // the mirror of the classic tie rule.
    let anchor = Rect::new(0, 2, 1, 1);
    let placed = place_panel_biased(Size::new(80, 4), anchor, Size::new(20, 4), width, above);
    assert_eq!(placed, Rect::new(0, 0, 20, 2), "above (2) >= below (1)");
    let anchor = Rect::new(0, 1, 1, 1);
    let placed = place_panel_biased(Size::new(80, 5), anchor, Size::new(20, 4), width, above);
    assert_eq!(placed, Rect::new(0, 2, 20, 3), "below (3) > above (1)");
}

#[test]
fn place_panel_biased_below_preferred_is_byte_identical_to_place_panel() {
    // The compatibility pin: the classic face delegates, so every
    // (anchor, content, viewport) cell must agree — including the
    // no-room-honesty and clamp edges the 0500 tests pin.
    let width = PanelWidth::Content { min: 4, max: 30 };
    for vp_h in [1, 4, 10, 24] {
        let vp = Size::new(40, vp_h);
        for anchor_y in 0..vp_h {
            for content_h in [1, 3, 8] {
                let anchor = Rect::new(35, anchor_y, 1, 1);
                let content = Size::new(20, content_h);
                assert_eq!(
                    place_panel(vp, anchor, content, width),
                    place_panel_biased(vp, anchor, content, width, PanelPlacement::default()),
                    "vp_h={vp_h} anchor_y={anchor_y} content_h={content_h}"
                );
            }
        }
    }
}

#[test]
fn above_preferred_completion_keeps_short_lists_off_the_chrome_below() {
    // The full filed shape, live: composer directly above a one-row
    // status bar, biased completion attached.
    let mut rig = completion_rig_with(Size::new(40, 10), true, |c, q| {
        c.trigger('/', move |query| {
            q.borrow_mut().push(query.to_string());
            command_candidates(query)
        })
        .max_visible(3)
        .placement(PanelPlacement::AbovePreferred)
    });
    rig.type_str("/th"); // one candidate left as you finish typing
    let (rect, rows) = rig.panel().expect("open");
    assert!(rows.iter().any(|r| r.contains("/theme")), "{rows:?}");
    let caret = rig.state.caret_cell().get_untracked().expect("anchor");
    assert_eq!(rect.h, 1, "short list");
    assert!(
        rect.bottom() <= caret.y,
        "short list sits ABOVE the caret, not on the legend: {rect:?} vs {caret:?}"
    );
    // Refilter to many candidates: still above, windowed as usual.
    rig.state.set_text("");
    flush_effects();
    rig.type_str("/");
    let (rect, _) = rig.panel().expect("open");
    assert_eq!(rect.h, 3, "4 candidates windowed to max_visible 3");
    assert!(rect.bottom() <= caret.y, "long lists honor the bias too");
}

#[test]
fn below_preferred_default_still_lands_on_the_row_below() {
    // The compatible default, pinned on the SAME chrome shape: without
    // the bias, a short list still takes the row under the caret (the
    // 0294 evidence — kept byte-identical, the opener opts out).
    let mut rig = completion_rig_with(Size::new(40, 10), true, |c, q| {
        c.trigger('/', move |query| {
            q.borrow_mut().push(query.to_string());
            command_candidates(query)
        })
        .max_visible(3)
    });
    rig.type_str("/th");
    let (rect, _) = rig.panel().expect("open");
    let caret = rig.state.caret_cell().get_untracked().expect("anchor");
    assert_eq!(
        rect.y,
        caret.y + 1,
        "default placement unchanged: below the caret"
    );
}

#[test]
fn passive_panel_biased_opens_above_and_update_keeps_the_bias() {
    // Substrate-level: the biased open face + update re-placement.
    let overlays = Overlays::new();
    overlays.ensure_root(Size::new(40, 12));
    let vp = Size::new(40, 12);
    let (root, panel) = create_root(|cx| {
        AnchoredPanel::open_passive_biased(
            &overlays,
            cx,
            vp,
            PanelAnchor::cell(Point::new(5, 10)),
            PanelWidth::Content { min: 4, max: 30 },
            Size::new(10, 1),
            PanelPlacement::AbovePreferred,
            |_| {
                Element::new()
                    .style(
                        LayoutStyle::default()
                            .width(Dimension::Percent(1.0))
                            .height(Dimension::Percent(1.0)),
                    )
                    .child(crate::ui::text("panel"))
                    .build()
            },
        )
    });
    assert_eq!(
        panel.rect().expect("placed"),
        Rect::new(5, 9, 10, 1),
        "opened above the anchor despite room below"
    );
    // A moved anchor re-places with the SAME bias (stored, not
    // per-call): still above.
    panel.update(vp, PanelAnchor::cell(Point::new(9, 8)), Size::new(10, 1));
    assert_eq!(panel.rect().expect("placed"), Rect::new(9, 7, 10, 1));
    // And the mirror flip: an anchor at the top edge falls below.
    panel.update(vp, PanelAnchor::cell(Point::new(3, 0)), Size::new(10, 2));
    assert_eq!(panel.rect().expect("placed"), Rect::new(3, 1, 10, 2));
    panel.close();
    root.dispose();
}
