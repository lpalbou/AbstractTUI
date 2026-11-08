//! Mention chip tests (the `@entity` wave) — split sibling for the
//! file budget, `#[path]`-included as `textarea::chips_tests`. A real
//! mounted TextArea, real key dispatch, real paint.

use std::cell::RefCell;
use std::rc::Rc;

use super::{TextArea, TextAreaState};
use crate::base::{Point, Size};
use crate::render::Attrs;
use crate::theme::default_theme;
use crate::ui::{Key, UiTree};
use crate::widgets::itest_util::{key, mount_widget, settle, type_str};
use crate::widgets::{ComposerSubmission, Mention, MentionCandidate, MentionKind};

const SIZE: Size = Size::new(40, 4);

fn castor() -> MentionCandidate {
    MentionCandidate::new("ent-castor", "Castor", MentionKind::Entity).hint("awake")
}
fn helper() -> MentionCandidate {
    MentionCandidate::new("basic-agent", "Basic agent", MentionKind::Agent)
}
fn mention(c: &MentionCandidate, index: usize) -> Mention {
    Mention {
        id: c.id.clone(),
        name: c.name.clone(),
        kind: c.kind,
        index,
    }
}

struct Rig {
    _root: crate::reactive::RootScope,
    tree: UiTree,
    state: TextAreaState,
}

fn rig() -> Rig {
    let holder: Rc<RefCell<Option<TextAreaState>>> = Default::default();
    let h2 = holder.clone();
    let (root, mut tree) = mount_widget(SIZE, move |cx| {
        let state = TextAreaState::new(cx);
        *h2.borrow_mut() = Some(state.clone());
        TextArea::new()
            .state(&state)
            .rows(1, 3)
            .element(cx, &default_theme().tokens)
            .build()
    });
    key(&mut tree, Key::Tab); // focus
    let state = holder.borrow().clone().expect("state");
    Rig {
        _root: root,
        tree,
        state,
    }
}

impl Rig {
    /// Type `@` + `query`, then accept `pick` over that token — what
    /// the completion controller's accept does.
    fn pick(&mut self, query: &str, pick: &MentionCandidate) {
        type_str(&mut self.tree, &format!("@{query}"));
        let caret = self.state.caret_byte();
        let start = caret - query.len() - 1;
        self.state.insert_mention(start..caret, pick);
    }
}

#[test]
fn a_pick_becomes_one_chip_and_the_submission_spells_it() {
    let mut r = rig();
    type_str(&mut r.tree, "ask ");
    r.pick("cas", &castor());
    type_str(&mut r.tree, "about it");
    let raw = r.state.text();
    assert_eq!(
        raw.chars().filter(|c| !c.is_ascii()).count(),
        1,
        "exactly ONE marker scalar for the chip: {raw:?}"
    );
    assert_eq!(
        r.state.submission(),
        ComposerSubmission {
            text: "ask @Castor about it".into(),
            mentions: vec![mention(&castor(), 0)],
        }
    );
}

#[test]
fn backspace_removes_the_chip_as_one_unit() {
    let mut r = rig();
    type_str(&mut r.tree, "hi ");
    r.pick("ca", &castor());
    // First Backspace: the space the pick added after the chip.
    key(&mut r.tree, Key::Backspace);
    assert_eq!(r.state.submission().text, "hi @Castor");
    // Second: the WHOLE chip, never one letter of its name.
    key(&mut r.tree, Key::Backspace);
    assert_eq!(
        r.state.submission(),
        ComposerSubmission {
            text: "hi ".into(),
            mentions: vec![],
        }
    );
    assert_eq!(r.state.text(), "hi ", "no remnant left in the buffer");
}

#[test]
fn the_caret_steps_over_a_chip_and_typing_never_edits_it() {
    let mut r = rig();
    r.pick("ca", &castor());
    // Caret is after "X " (chip + space). Left once: before the space;
    // Left again: before the chip — one step over the whole name.
    key(&mut r.tree, Key::Left);
    let after_chip = r.state.caret_byte();
    key(&mut r.tree, Key::Left);
    assert_eq!(r.state.caret_byte(), 0, "one Left crosses the chip");
    type_str(&mut r.tree, "x");
    // Back to just after the chip, type there: still outside it.
    key(&mut r.tree, Key::Right);
    assert_eq!(r.state.caret_byte(), after_chip + 1);
    type_str(&mut r.tree, "y");
    let sub = r.state.submission();
    assert_eq!(sub.text, "x@Castory ");
    assert_eq!(sub.mentions, vec![mention(&castor(), 0)], "name untouched");
    // Delete in front of the chip removes it whole too.
    key(&mut r.tree, Key::Home);
    key(&mut r.tree, Key::Right); // past "x"
    key(&mut r.tree, Key::Delete);
    assert_eq!(r.state.submission().text, "xy ");
}

#[test]
fn several_chips_submit_in_text_order_with_their_index() {
    let mut r = rig();
    r.pick("ba", &helper());
    type_str(&mut r.tree, "then");
    // Insert a second chip BEFORE the first one: order is the text's,
    // not the order the picks were made in.
    key(&mut r.tree, Key::Home);
    r.pick("cas", &castor());
    let sub = r.state.submission();
    assert_eq!(sub.text, "@Castor @Basic agent then");
    assert_eq!(
        sub.mentions,
        vec![mention(&castor(), 0), mention(&helper(), 1)]
    );
    // The same name picked twice is two mentions (the host decides).
    key(&mut r.tree, Key::End);
    type_str(&mut r.tree, " ");
    r.pick("c", &castor());
    let ids: Vec<(String, usize)> = r
        .state
        .submission()
        .mentions
        .into_iter()
        .map(|m| (m.id, m.index))
        .collect();
    assert_eq!(
        ids,
        [
            ("ent-castor".to_string(), 0),
            ("basic-agent".to_string(), 1),
            ("ent-castor".to_string(), 2)
        ]
    );
}

#[test]
fn a_typed_at_word_without_a_pick_stays_text() {
    let mut r = rig();
    type_str(&mut r.tree, "@Castor hello @castor");
    assert_eq!(
        r.state.submission(),
        ComposerSubmission {
            text: "@Castor hello @castor".into(),
            mentions: vec![],
        },
        "a mention exists only as a pick, never parsed from the text"
    );
    // A host draft round trip (set_text of plain text) adds none either.
    r.state.set_text("@Castor again");
    assert!(r.state.submission().mentions.is_empty());
}

#[test]
fn a_chip_renders_as_its_label_in_the_chip_style() {
    let mut r = rig();
    type_str(&mut r.tree, "hi ");
    r.pick("ca", &castor());
    type_str(&mut r.tree, "ok");
    let canvas = settle(&mut r.tree, SIZE);
    let row = canvas.row_text(0);
    // Cell column of the label (the stroke glyph before it is 3 bytes).
    let x = row[..row.find("@Castor").expect(&row)].chars().count() as i32;
    let t = default_theme().tokens;
    for dx in 0..7 {
        let (_, fg, bg) = canvas.cell(Point::new(x + dx, 0)).expect("cell");
        assert_eq!((fg, bg), (t.accent, t.surface_raised), "chip cell {dx}");
        assert!(canvas.attrs_at(Point::new(x + dx, 0)).contains(Attrs::BOLD));
    }
    // Typed text beside it keeps the text style.
    let (_, fg, bg) = canvas.cell(Point::new(x - 2, 0)).expect("cell");
    assert_eq!((fg, bg), (t.text, t.surface), "typed text is plain");
    assert!(row.contains("hi @Castor ok"), "{row}");
    // The caret column after the chip counts its label's width.
    let cell = r.state.caret_cell().get_untracked().expect("focused");
    assert_eq!(cell.x, x + 7 + 1 + 2, "chip 7 + space + 'ok'");
}

#[test]
fn chips_wrap_at_their_label_width() {
    let mut r = rig();
    // 38 interior cells (40 - 2 strokes): 30 chars + a 12-cell chip
    // cannot share a row.
    type_str(&mut r.tree, &"a".repeat(30));
    type_str(&mut r.tree, " ");
    r.pick("ba", &helper());
    let canvas = settle(&mut r.tree, SIZE);
    assert!(
        canvas.row_text(1).contains("@Basic agent"),
        "the chip moved to the next row whole:\n{}\n{}",
        canvas.row_text(0),
        canvas.row_text(1)
    );
}
