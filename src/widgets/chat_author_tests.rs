//! Author rendering on chat rows (the `@entity` wave, 2026-11-08): the
//! contract's `author` object drives a kind tag + name, an entity's
//! coloured mark and the agent's mark — and a row WITHOUT it renders
//! exactly as it did before the field existed (golden minted from the
//! pre-change rendering).
use super::*;
use crate::{
    base::Size,
    render::Attrs,
    testing::snapshot::assert_snapshot,
    widgets::itest_util::{mount_widget, settle, styled_rows},
};

fn capture(size: Size, messages: Vec<ChatMessage>) -> (String, crate::ui::BufferCanvas) {
    let (root, mut tree) = mount_widget(size, move |cx| ChatTranscript::new(messages).view(cx));
    let canvas = settle(&mut tree, size);
    let dump = styled_rows(&canvas, size);
    root.dispose();
    (dump, canvas)
}

/// Every role, with and without host labels and meta: the rows any
/// host builds today. Pinned cell-for-cell (text, fg, bg, weight).
fn todays_rows() -> Vec<ChatMessage> {
    vec![
        ChatMessage::user("A question"),
        ChatMessage::assistant("An answer."),
        ChatMessage::assistant("Labelled answer.")
            .author("Castor")
            .meta("10:42"),
        ChatMessage::new(ChatRole::Notice, "A notice."),
        ChatMessage::new(ChatRole::Error, "It failed."),
        ChatMessage::user("Labelled question.").author("Laurent"),
    ]
}

#[test]
fn a_row_without_author_renders_exactly_as_before() {
    let (dump, _) = capture(Size::new(48, 30), todays_rows());
    assert_snapshot("chat_author_none_is_todays_rendering", &dump);
}

fn castor() -> ChatAuthor {
    ChatAuthor::new(AuthorKind::Entity, "Castor")
        .entity_id("ent-castor")
        .identity_ref("identity:castor@home")
        .avatar(AuthorAvatar {
            style: Some("watch-halo".into()),
            color: Some("#f5c542".into()),
        })
}

/// The heading row of the first bubble whose heading contains `name`.
fn heading_row(canvas: &crate::ui::BufferCanvas, size: Size, name: &str) -> i32 {
    (0..size.h)
        .find(|y| canvas.row_text(*y).contains(name))
        .unwrap_or_else(|| panic!("no row names {name}"))
}

fn cell(canvas: &crate::ui::BufferCanvas, x: i32, y: i32) -> (char, crate::base::Rgba, Attrs) {
    let p = crate::base::Point::new(x, y);
    let (ch, fg, _) = canvas.cell(p).expect("in bounds");
    (ch, fg, canvas.attrs_at(p))
}

#[test]
fn an_entity_row_shows_its_coloured_mark_name_and_kind() {
    let size = Size::new(48, 8);
    let t = crate::theme::default_theme().tokens;
    let (dump, canvas) = capture(
        size,
        vec![ChatMessage::assistant("I read the thread. Ship it.")
            .authored_by(castor())
            .meta("10:42")],
    );
    assert_snapshot("chat_author_entity_row", &dump);
    let y = heading_row(&canvas, size, "Castor");
    assert_eq!(canvas.row_text(y).trim_end(), " ● Castor · entity · 10:42");
    // The mark wears the entity's own colour (readable on the bubble).
    let (mark, fg, _) = cell(&canvas, 1, y);
    assert_eq!(mark, '●');
    let gold =
        crate::theme::derive::identity_ink(Some("#f5c542"), t.surface_raised, t.text, t.accent);
    assert_eq!(fg, gold);
    assert_ne!(fg, t.accent, "derived from avatar.color, not the theme");
    // Name: the role's bold ink. Kind tag: muted, not bold.
    let (_, fg, attrs) = cell(&canvas, 3, y);
    assert_eq!((fg, attrs.contains(Attrs::BOLD)), (t.text, true));
    let (ch, fg, attrs) = cell(&canvas, 12, y);
    assert_eq!(ch, 'e');
    assert_eq!((fg, attrs.contains(Attrs::BOLD)), (t.text_muted, false));
}

#[test]
fn an_agent_row_wears_the_agent_mark_and_a_person_none() {
    let size = Size::new(48, 12);
    let t = crate::theme::default_theme().tokens;
    let (_, canvas) = capture(
        size,
        vec![
            ChatMessage::assistant("Done.")
                .authored_by(ChatAuthor::new(AuthorKind::Agent, "Basic agent")),
            ChatMessage::user("Thanks.")
                .authored_by(ChatAuthor::new(AuthorKind::Person, "Laurent")),
        ],
    );
    let y = heading_row(&canvas, size, "Basic agent");
    assert_eq!(canvas.row_text(y).trim_end(), " ✦ Basic agent · agent");
    let (_, fg, _) = cell(&canvas, 1, y);
    assert_eq!(fg, t.accent, "the agent mark");
    let y = heading_row(&canvas, size, "Laurent");
    let row = canvas.row_text(y);
    assert!(row.trim_end().ends_with("Laurent · person"), "{row}");
    assert!(
        !row.contains('●') && !row.contains('✦'),
        "a person has no mark: {row}"
    );
}

#[test]
fn an_entity_without_a_colour_and_an_empty_name_fall_back_honestly() {
    let size = Size::new(48, 8);
    let t = crate::theme::default_theme().tokens;
    let (_, canvas) = capture(
        size,
        vec![ChatMessage::assistant("Hello.")
            .author("Castor (host label)")
            .authored_by(ChatAuthor::new(AuthorKind::Entity, ""))],
    );
    let y = heading_row(&canvas, size, "entity");
    assert_eq!(
        canvas.row_text(y).trim_end(),
        " ● Castor (host label) · entity",
        "an empty contract name keeps the row's own label"
    );
    let (_, fg, _) = cell(&canvas, 1, y);
    assert_eq!(fg, t.accent, "no avatar colour: the theme accent");
}

#[test]
fn a_narrow_authored_heading_is_cut_with_an_ellipsis() {
    let size = Size::new(16, 8);
    let (_, canvas) = capture(
        size,
        vec![ChatMessage::assistant("Hi")
            .authored_by(castor())
            .meta("10:42")],
    );
    let y = heading_row(&canvas, size, "Castor");
    let row = canvas.row_text(y);
    assert!(row.contains('…'), "{row}");
    assert!(crate::text::width(row.trim_end()) <= 16, "{row}");
}

#[test]
fn a_host_refusal_notice_is_a_plain_row() {
    // The host maps a refused mention (409 in_another_conversation,
    // 503 entity_cannot_answer, an old gateway) to a Notice row: no
    // author, so no mark, no kind tag and no name above the sentence.
    let size = Size::new(48, 6);
    let (_, canvas) = capture(
        size,
        vec![ChatMessage::new(
            ChatRole::Notice,
            "Castor is in another conversation.",
        )],
    );
    let y = heading_row(&canvas, size, "another conversation");
    let all: String = (0..size.h).map(|y| canvas.row_text(y)).collect();
    assert!(!all.contains('●') && !all.contains("· entity"), "{all}");
    assert_eq!(canvas.row_text(y - 1).trim(), "", "a notice has no heading");
}

#[test]
fn marks_are_one_source_for_hosts() {
    assert_eq!(AuthorKind::Entity.mark(), Some("●"));
    assert_eq!(AuthorKind::Agent.mark(), Some("✦"));
    assert_eq!(AuthorKind::Person.mark(), None);
}
