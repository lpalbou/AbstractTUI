//! Who wrote a chat row (private sibling of chat.rs — file-size split):
//! the contract's `author` object and the heading it renders.
//!
//! Every turn the gateway returns carries
//! `author: {kind, name, entity_id, identity_ref, avatar: {style, color}}`
//! with nulls present; the host maps it onto [`ChatAuthor`] (this crate
//! parses no JSON). The heading then reads `mark name · kind`: an
//! entity's mark is `●` in its own avatar colour, an agent's is `✦`, a
//! person has none. A row without an author renders exactly as before
//! the field existed (golden-pinned).
//!
//! OWNER: REACT.

use crate::{
    render::{Attrs, Style},
    theme::TokenSet,
};

/// The contract's `author.kind`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum AuthorKind {
    Entity,
    Agent,
    Person,
}

impl AuthorKind {
    /// The kind tag the heading shows (and the contract's spelling).
    pub fn as_str(self) -> &'static str {
        match self {
            AuthorKind::Entity => "entity",
            AuthorKind::Agent => "agent",
            AuthorKind::Person => "person",
        }
    }

    /// The heading's mark: `●` for an entity (drawn in its avatar
    /// colour through [`identity_ink`](crate::theme::derive::identity_ink)),
    /// `✦` for an agent, none for a person. For hosts that draw their
    /// own transcript rows and want the same marks.
    pub fn mark(self) -> Option<&'static str> {
        match self {
            AuthorKind::Entity => Some(ENTITY_MARK),
            AuthorKind::Agent => Some(AGENT_MARK),
            AuthorKind::Person => None,
        }
    }
}

/// The contract's `author.avatar`: both fields nullable.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AuthorAvatar {
    pub style: Option<String>,
    /// `#rrggbb` (the gateway's identity colour); tints an entity's mark.
    pub color: Option<String>,
}

/// The contract's `author` object, as the host received it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatAuthor {
    pub kind: AuthorKind,
    pub name: String,
    pub entity_id: Option<String>,
    pub identity_ref: Option<String>,
    pub avatar: Option<AuthorAvatar>,
}

impl ChatAuthor {
    pub fn new(kind: AuthorKind, name: impl Into<String>) -> Self {
        ChatAuthor {
            kind,
            name: name.into(),
            entity_id: None,
            identity_ref: None,
            avatar: None,
        }
    }

    pub fn entity_id(mut self, id: impl Into<String>) -> Self {
        self.entity_id = Some(id.into());
        self
    }

    pub fn identity_ref(mut self, r: impl Into<String>) -> Self {
        self.identity_ref = Some(r.into());
        self
    }

    pub fn avatar(mut self, avatar: AuthorAvatar) -> Self {
        self.avatar = Some(avatar);
        self
    }
}

/// The entity mark: a small filled disc.
pub(crate) const ENTITY_MARK: &str = "●";
/// The agent mark (the desktop Assistant's header glyph).
pub(crate) const AGENT_MARK: &str = "✦";

/// The heading of an authored row as styled spans: `[mark ]name`,
/// ` · kind` (muted), then ` · meta` in the name's style when present.
/// `fallback` names the row when the author's name is empty (the host
/// label or the role's default — the same name the row had before).
pub(crate) fn heading_spans(
    author: &ChatAuthor,
    fallback: &str,
    meta: &str,
    ink: crate::base::Rgba,
    bg: crate::base::Rgba,
    t: &TokenSet,
) -> Vec<(String, Style)> {
    let strong = Style::new().fg(ink).bg(bg).attrs(Attrs::BOLD);
    let muted = Style::new().fg(t.text_muted).bg(bg);
    let mut spans = Vec::new();
    if let Some(mark) = author.kind.mark() {
        let ink = match author.kind {
            AuthorKind::Entity => {
                let color = author.avatar.as_ref().and_then(|a| a.color.as_deref());
                crate::theme::derive::identity_ink(color, bg, t.text, t.accent)
            }
            _ => t.accent,
        };
        spans.push((format!("{mark} "), strong.fg(ink)));
    }
    let name = if author.name.is_empty() {
        fallback
    } else {
        author.name.as_str()
    };
    spans.push((name.to_string(), strong));
    spans.push((format!(" · {}", author.kind.as_str()), muted));
    if !meta.is_empty() {
        spans.push((format!(" · {meta}"), strong));
    }
    spans
}

/// Paint spans left to right inside `max` cells; the span that does not
/// fit is cut with an ellipsis and painting stops there.
pub(crate) fn print_spans(
    canvas: &mut dyn crate::ui::StyledCanvas,
    at: crate::base::Point,
    spans: &[(String, Style)],
    max: i32,
) {
    let mut x = 0;
    for (text, style) in spans {
        let room = max - x;
        if room <= 0 {
            return;
        }
        let w = crate::text::width(text);
        let shown = if w > room {
            crate::text::truncate_ellipsis(text, room)
        } else {
            text.clone()
        };
        canvas.print_styled(crate::base::Point::new(at.x + x, at.y), &shown, style);
        if w > room {
            return;
        }
        x += w;
    }
}
