//! [`GraphStyle`] (resolved inks) + [`GraphAlgo`] (pass selection) —
//! the configuration half of the view, split from `view.rs` for the
//! file-size discipline.
//!
//! OWNER: CANVAS (view half of 0440).

use abstracttui::base::Rgba;
use abstracttui::theme::TokenSet;

use crate::layout::{ForceOpts, LayeredOpts};

/// Resolved ink set for the view (the widget token rule: callers
/// resolve theme tokens into plain `Rgba`; the view invents no
/// colors). Author-written, shape-stable: plain fields + [`Default`]
/// (derived from the default theme) + FRU per ADR-0003 §2.
#[derive(Clone, Debug, PartialEq)]
pub struct GraphStyle {
    /// Card fill.
    pub card_bg: Rgba,
    /// Card border ink.
    pub card_border: Rgba,
    /// Border ink of the SELECTED card (the restyle).
    pub card_border_selected: Rgba,
    /// Title ink.
    pub card_title: Rgba,
    /// Badge ink.
    pub badge: Rgba,
    /// Edge stroke ink.
    pub edge: Rgba,
    /// Ink of cycle-broken edges (the honesty marker; broken edges
    /// also render dotted).
    pub edge_broken: Rgba,
    /// Edge midpoint-label ink.
    pub edge_label: Rgba,
    /// Fallback notice line ink.
    pub notice: Rgba,
    /// Kind -> accent ink for the card's left border column (exact
    /// string match on `NodeDesc::kind`; unknown kinds stay plain).
    pub kind_accents: Vec<(String, Rgba)>,
}

impl GraphStyle {
    /// Derive the default ink set from a resolved token set.
    pub fn from_tokens(t: &TokenSet) -> GraphStyle {
        GraphStyle {
            card_bg: t.surface_raised,
            card_border: t.border,
            card_border_selected: t.border_focus,
            card_title: t.text,
            badge: t.info,
            edge: t.text_muted,
            edge_broken: t.error,
            edge_label: t.text_faint,
            notice: t.warn,
            kind_accents: Vec::new(),
        }
    }

    /// Map a node kind to an accent ink (builder style).
    pub fn kind_accent(mut self, kind: impl Into<String>, ink: Rgba) -> GraphStyle {
        self.kind_accents.push((kind.into(), ink));
        self
    }

    pub(crate) fn accent_of(&self, kind: Option<&str>) -> Option<Rgba> {
        let kind = kind?;
        self.kind_accents
            .iter()
            .find(|(k, _)| k == kind)
            .map(|(_, ink)| *ink)
    }
}

impl Default for GraphStyle {
    fn default() -> Self {
        GraphStyle::from_tokens(&abstracttui::theme::default_theme().tokens)
    }
}

/// Layout pass selection. The vocabulary may grow with the crate's
/// passes, hence `#[non_exhaustive]` (ADR-0003 §3); constructing the
/// existing variants stays stable.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub enum GraphAlgo {
    /// Sugiyama-lite (the workflow/DAG path) — the default.
    Layered(LayeredOpts),
    /// Bounded seeded force placement (the knowledge-graph path).
    Force(ForceOpts),
    /// Labeled near-square grid (the honest fallback, explicitly).
    Grid,
}

impl Default for GraphAlgo {
    fn default() -> Self {
        GraphAlgo::Layered(LayeredOpts::default())
    }
}
