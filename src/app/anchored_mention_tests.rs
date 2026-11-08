//! `Completion::mentions` (the `@entity` wave), driven through the
//! parent module's real rig: a mounted composer, a real overlay store,
//! driver-order routing. Child module of `anchored_tests`.

use super::*;
use crate::widgets::{ComposerSubmission, Mention, MentionCandidate, MentionKind};

fn roster() -> Vec<MentionCandidate> {
    vec![
        MentionCandidate::new("ent-castor", "Castor", MentionKind::Entity).hint("awake"),
        MentionCandidate::new("ent-pollux", "Pollux", MentionKind::Entity),
        MentionCandidate::new("basic-agent", "Basic agent", MentionKind::Agent),
    ]
}

/// '/' commands beside '@' mentions — the AbstractCode composer shape.
fn mention_rig() -> Rig {
    completion_rig_with(Size::new(48, 10), false, |c, q| {
        c.trigger('/', move |query| {
            q.borrow_mut().push(query.to_string());
            vec![CompletionCandidate::new("/help", "/help ")]
                .into_iter()
                .filter(|c| c.label[1..].starts_with(query))
                .collect()
        })
        .mentions('@', roster)
    })
}

fn castor(index: usize) -> Mention {
    Mention {
        id: "ent-castor".into(),
        name: "Castor".into(),
        kind: MentionKind::Entity,
        index,
    }
}

#[test]
fn the_popup_lists_the_hosts_candidates_with_kind_and_hint() {
    let mut rig = mention_rig();
    rig.type_str("@");
    let (_, rows) = rig.panel().expect("'@' opened the popup");
    assert!(
        rows[0].contains("@Castor") && rows[0].contains("entity · awake"),
        "{rows:?}"
    );
    assert!(rows
        .iter()
        .any(|r| r.contains("@Basic agent") && r.contains("agent")));
    rig.type_str("po");
    let (_, rows) = rig.panel().expect("still open");
    assert_eq!(rows.len(), 1);
    assert!(rows[0].contains("@Pollux"), "{rows:?}");
}

#[test]
fn accepting_a_mention_row_inserts_a_chip_not_text() {
    let mut rig = mention_rig();
    rig.type_str("ask @ca");
    rig.key(Key::Enter);
    assert!(rig.panel().is_none(), "accept closes");
    rig.type_str("now");
    assert_eq!(
        rig.state.submission(),
        ComposerSubmission {
            text: "ask @Castor now".into(),
            mentions: vec![castor(0)],
        }
    );
    // One Backspace past "now " takes the whole chip: it is a chip.
    for _ in 0..4 {
        rig.key(Key::Backspace);
    }
    rig.key(Key::Backspace);
    assert_eq!(rig.state.submission().text, "ask ");
    // Tab accepts too; the text trigger beside it still inserts text.
    rig.type_str("@pol");
    rig.key(Key::Tab);
    rig.type_str("/");
    rig.key(Key::Enter);
    let sub = rig.state.submission();
    assert_eq!(sub.text, "ask @Pollux /help ");
    assert_eq!(sub.mentions.len(), 1);
}

#[test]
fn a_fully_typed_name_that_is_never_picked_stays_text() {
    let mut rig = mention_rig();
    // The popup is open on an exact match; the space closes the token
    // without a pick — no chip may appear from the text.
    rig.type_str("@castor ");
    assert!(rig.panel().is_none(), "the token ended");
    rig.type_str("hello");
    assert_eq!(
        rig.state.submission(),
        ComposerSubmission {
            text: "@castor hello".into(),
            mentions: vec![],
        }
    );
    // Escape dismisses without a pick: still text.
    rig.type_str(" @Pollux");
    rig.key(Key::Escape);
    assert!(rig.state.submission().mentions.is_empty());
}

// ------------------------------------------------- ChatPanel mentions
//
// Here rather than in widgets/chat tests: the popup places against the
// app viewport, whose publisher is app-private.

use crate::reactive::Signal;
use crate::widgets::itest_util::{settle, styled_rows};
use crate::widgets::ChatPanel;

struct ChatRig {
    _root: RootScope,
    tree: UiTree,
    overlays: Overlays,
    size: Size,
    told: Signal<Vec<String>>,
    sent: Rc<RefCell<Vec<ComposerSubmission>>>,
    texts: Rc<RefCell<Vec<String>>>,
}

/// A ChatPanel with `@` mentions under a provided Overlays context.
/// `structured` = the host uses `on_send`; otherwise the `&str` path.
fn chat_rig(structured: bool) -> ChatRig {
    let size = Size::new(64, 14);
    crate::app::viewport::publish_viewport(size);
    let overlays = Overlays::new();
    overlays.ensure_root(size);
    let sent: Rc<RefCell<Vec<ComposerSubmission>>> = Default::default();
    let texts: Rc<RefCell<Vec<String>>> = Default::default();
    let (s2, t2, ov) = (sent.clone(), texts.clone(), overlays.clone());
    let told_holder: Rc<RefCell<Option<Signal<Vec<String>>>>> = Default::default();
    let th = told_holder.clone();
    let mut tree = UiTree::new(size);
    let (root, ()) = create_root(|cx| {
        cx.provide_context(ov.clone());
        let draft = cx.signal(String::new());
        let told = cx.signal(Vec::<String>::new());
        *th.borrow_mut() = Some(told);
        let mut panel = ChatPanel::new(draft, Vec::new, move |q| {
            t2.borrow_mut().push(q.to_string());
            draft.set(String::new());
        })
        .mentions(roster)
        .mention_notices(told);
        if structured {
            panel = panel.on_send(move |v| {
                s2.borrow_mut().push(v.clone());
                draft.set(String::new());
            });
        }
        tree.mount(cx, panel.view(cx));
    });
    tree.layout();
    flush_effects();
    let told = told_holder.borrow().expect("told");
    ChatRig {
        _root: root,
        tree,
        overlays,
        size,
        told,
        sent,
        texts,
    }
}

impl ChatRig {
    fn send(&mut self, ev: &crate::ui::UiEvent) {
        if self.overlays.dispatch(ev).is_none() {
            self.tree.dispatch(ev);
        }
        flush_effects();
    }
    fn type_str(&mut self, s: &str) {
        for ch in s.chars() {
            self.send(&crate::ui::UiEvent::Key(KeyEvent::plain(Key::Char(ch))));
        }
    }
    fn key(&mut self, k: Key) {
        self.send(&crate::ui::UiEvent::Key(KeyEvent::plain(k)));
    }
    /// Type `@query`, accept the highlighted row with Enter.
    fn pick(&mut self, query: &str) {
        self.type_str(&format!("@{query}"));
        self.key(Key::Enter);
    }
    fn screen(&mut self) -> (String, String) {
        let canvas = settle(&mut self.tree, self.size);
        let text = (0..self.size.h)
            .map(|y| canvas.row_text(y).trim_end().to_string())
            .collect::<Vec<_>>()
            .join("\n");
        (text, styled_rows(&canvas, self.size))
    }
}

const NOTICE: &str = "will read this conversation's history.";

#[test]
fn chat_panel_sends_text_and_mentions_in_order() {
    let mut rig = chat_rig(true);
    rig.type_str("ask ");
    rig.pick("cas");
    rig.type_str("and ");
    rig.pick("basic");
    rig.type_str("about the release");
    // Capture: the composer holding two chips, before the send.
    let (text, styled) = rig.screen();
    assert!(
        text.contains("ask @Castor and @Basic agent about the release"),
        "{text}"
    );
    crate::testing::snapshot::assert_snapshot(
        "chat_composer_two_chips",
        &format!("{text}\n\n--- styles ---\n{styled}"),
    );
    rig.key(Key::Enter);
    let sent = rig.sent.borrow().clone();
    assert_eq!(sent.len(), 1, "one send");
    assert_eq!(
        sent[0].text,
        "ask @Castor and @Basic agent about the release"
    );
    let order: Vec<(&str, usize)> = sent[0]
        .mentions
        .iter()
        .map(|m| (m.id.as_str(), m.index))
        .collect();
    assert_eq!(order, [("ent-castor", 0), ("basic-agent", 1)]);
    assert!(
        rig.texts.borrow().is_empty(),
        "on_send replaces the &str path"
    );
}

#[test]
fn chat_panel_without_on_send_hands_the_host_the_spelled_text() {
    let mut rig = chat_rig(false);
    rig.pick("pol");
    rig.type_str("hi");
    rig.key(Key::Enter);
    assert_eq!(rig.texts.borrow().as_slice(), ["@Pollux hi"]);
}

#[test]
fn the_history_notice_is_told_once_per_session_per_name() {
    let mut rig = chat_rig(true);
    rig.pick("cas");
    let (text, _) = rig.screen();
    assert!(text.contains(&format!("Castor {NOTICE}")), "{text}");
    rig.type_str("hello");
    rig.key(Key::Enter);
    assert_eq!(rig.told.get_untracked(), ["ent-castor"], "learned on send");
    // Castor again in the same session: no second telling.
    rig.pick("cas");
    let (text, _) = rig.screen();
    assert!(!text.contains(NOTICE), "told once:\n{text}");
    // A new name is told; the old one is not repeated beside it.
    rig.pick("pol");
    let (text, _) = rig.screen();
    assert!(text.contains(&format!("Pollux {NOTICE}")), "{text}");
    assert!(!text.contains("Castor and Pollux"), "{text}");
    // No chip, no notice: a plain draft shows nothing.
    let mut plain = chat_rig(true);
    plain.type_str("@castor typed, not picked");
    plain.key(Key::Escape);
    let (text, _) = plain.screen();
    assert!(!text.contains(NOTICE), "{text}");
}

#[test]
fn chat_panel_sends_a_typed_name_as_text_without_a_mention() {
    let mut rig = chat_rig(true);
    // Exact name, exact case, popup open on it — but never accepted:
    // the space ends the token, Escape dismisses the next one.
    rig.type_str("@Castor hello @Pollux");
    rig.key(Key::Escape);
    rig.key(Key::Enter);
    let sent = rig.sent.borrow().clone();
    assert_eq!(
        sent,
        [ComposerSubmission {
            text: "@Castor hello @Pollux".into(),
            mentions: vec![],
        }],
        "nothing is read out of the text on send"
    );
    assert!(rig.told.get_untracked().is_empty());
}
