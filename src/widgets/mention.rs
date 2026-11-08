//! Mentions: the structured `@` picks a composer carries as inline
//! chips (the `@entity` wave, 2026-11-08).
//!
//! A mention is never parsed out of free text. It exists only when the
//! person PICKS a host-supplied [`MentionCandidate`] from the `@`
//! completion popup (`app::anchored::Completion::mentions`); a typed
//! `@word` without a pick stays text. The pick becomes one atomic chip
//! in the [`TextAreaState`](super::TextAreaState) buffer, and
//! [`TextAreaState::submission`](super::TextAreaState::submission)
//! hands the host `{text, mentions}` in text order.
//!
//! OWNER: REACT.

/// What a mention names. The candidate list comes from the host (its
/// entity roster, its agents); the composer only carries the pick.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum MentionKind {
    Entity,
    Agent,
}

impl MentionKind {
    /// The contract's spelling: `"entity"` or `"agent"`.
    pub fn as_str(self) -> &'static str {
        match self {
            MentionKind::Entity => "entity",
            MentionKind::Agent => "agent",
        }
    }
}

/// One row the host offers in the `@` popup: `{id, name, kind, hint}`.
/// `name` is what the chip shows (`@name`) and what the submitted text
/// carries; `id` is what the host sends; `hint` renders muted beside
/// the row (a state, a role) and never reaches the submission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MentionCandidate {
    pub id: String,
    pub name: String,
    pub kind: MentionKind,
    pub hint: Option<String>,
}

impl MentionCandidate {
    pub fn new(id: impl Into<String>, name: impl Into<String>, kind: MentionKind) -> Self {
        MentionCandidate {
            id: id.into(),
            name: name.into(),
            kind,
            hint: None,
        }
    }

    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// The chip's visible label and its spelling in the submitted text.
    pub fn label(&self) -> String {
        format!("@{}", self.name)
    }
}

/// One chip in a submitted message. `index` is the chip's 0-based
/// position among the message's mentions, left to right — the send
/// order, and the contract's `mention_index`. A name picked twice is
/// two mentions; what that means is the host's call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mention {
    pub id: String,
    pub name: String,
    pub kind: MentionKind,
    pub index: usize,
}

/// The composer's submitted value: `text` with every chip spelled
/// `@name`, and the chips themselves in text order. A draft with no
/// chips is `{text: <the draft>, mentions: []}`, byte for byte.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ComposerSubmission {
    pub text: String,
    pub mentions: Vec<Mention>,
}

/// The one sentence a person reads before a mentioned entity or agent
/// first sees the conversation: `"<name> will read this conversation's
/// history."` Several new names join into one sentence. `ChatPanel`
/// shows it once per session per name; hosts with their own composer
/// use the same wording.
pub fn history_notice(names: &[&str]) -> String {
    let who = match names {
        [] => return String::new(),
        [one] => (*one).to_string(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    };
    format!("{who} will read this conversation's history.")
}

/// The popup's filter over the host's list: case-insensitive, name or
/// id. Prefix hits come first, then substring hits; the host's order
/// holds inside each group. An empty query offers the whole list. This
/// filters a structured list for the popup; it never reads the draft.
pub(crate) fn filter_candidates(query: &str, all: Vec<MentionCandidate>) -> Vec<MentionCandidate> {
    let q = query.to_lowercase();
    let (mut prefix, mut inner) = (Vec::new(), Vec::new());
    for c in all {
        let (name, id) = (c.name.to_lowercase(), c.id.to_lowercase());
        if name.starts_with(&q) || id.starts_with(&q) {
            prefix.push(c);
        } else if name.contains(&q) || id.contains(&q) {
            inner.push(c);
        }
    }
    prefix.extend(inner);
    prefix
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roster() -> Vec<MentionCandidate> {
        vec![
            MentionCandidate::new("ent-1", "Pollux", MentionKind::Entity),
            MentionCandidate::new("ent-2", "Castor", MentionKind::Entity).hint("asleep"),
            MentionCandidate::new("basic-agent", "Basic agent", MentionKind::Agent),
        ]
    }

    #[test]
    fn filter_ranks_prefix_hits_first_and_keeps_host_order() {
        let names = |q: &str| {
            filter_candidates(q, roster())
                .into_iter()
                .map(|c| c.name)
                .collect::<Vec<_>>()
        };
        assert_eq!(names(""), ["Pollux", "Castor", "Basic agent"]);
        assert_eq!(names("CAS"), ["Castor"], "case-insensitive prefix");
        assert_eq!(names("basic-"), ["Basic agent"], "the id matches too");
        // No prefix hit for "a": the substring hits, in host order.
        assert_eq!(names("a"), ["Castor", "Basic agent"]);
        // A prefix hit outranks an earlier substring hit.
        assert_eq!(names("b"), ["Basic agent"]);
        assert_eq!(names("o"), ["Pollux", "Castor"]);
        assert_eq!(names("st"), ["Castor"], "substring after no prefix hit");
        assert!(names("zz").is_empty());
    }

    #[test]
    fn history_notice_names_everyone_in_one_sentence() {
        assert_eq!(
            history_notice(&["Castor"]),
            "Castor will read this conversation's history."
        );
        assert_eq!(
            history_notice(&["Castor", "Pollux"]),
            "Castor and Pollux will read this conversation's history."
        );
        assert_eq!(
            history_notice(&["A", "B", "C"]),
            "A, B and C will read this conversation's history."
        );
        assert_eq!(history_notice(&[]), "");
    }
}
