//! Semantic design tokens: the only color vocabulary widgets may speak.
//!
//! Widgets never hold hex values; they resolve a [`TokenId`] against the
//! active theme's [`TokenSet`] (a signal — swapping themes re-renders
//! through normal reactivity). The set is sized for a full UI engine:
//! grounds and elevations, three text tiers, two accents, four semantic
//! states, interaction colors (selection/cursor/link/focus) and an 8-entry
//! chart ramp for data visuals.
//!
//! Token values are either ported verbatim from the AbstractUIC
//! `theme.css` family or derived by the documented rules in
//! `registry.rs`/`derive.rs`. Floors are enforced by `contrast::audit`.
//!
//! OWNER: DESIGN.

use crate::base::Rgba;

/// A complete resolved theme palette. All colors are opaque except
/// [`overlay`](TokenSet::overlay) and [`shadow`](TokenSet::shadow), which
/// deliberately carry alpha for the compositor to blend.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct TokenSet {
    /// Application ground — the deepest layer, fills the terminal.
    pub bg: Rgba,
    /// Panel/card ground (theme.css `--bg-secondary`). May be darker than
    /// `bg` in families that design it so (catppuccin mantle, one-dark).
    pub surface: Rgba,
    /// Raised chrome: popovers, menus, active tabs (`--bg-tertiary`).
    pub surface_raised: Rgba,
    /// Modal scrim, composited over whatever it covers (carries alpha).
    pub overlay: Rgba,
    /// Hairline strokes: pane separators, boxes, rules.
    pub border: Rgba,
    /// Focused-element stroke; must read stronger than `border`.
    pub border_focus: Rgba,
    /// Primary copy.
    pub text: Rgba,
    /// Secondary copy: labels, descriptions, timestamps.
    pub text_muted: Rgba,
    /// Decoration tier: placeholders, disabled glyphs, watermark art.
    /// Never used for information-carrying text.
    pub text_faint: Rgba,
    /// The theme's identity color: primary actions, active states, brand.
    pub accent: Rgba,
    /// Curated second accent (gradients, secondary emphasis). Sourced from
    /// the AbstractUIC swatch family; may equal `info` where the upstream
    /// family curates it so.
    pub accent_alt: Rgba,
    /// Positive state (success).
    pub ok: Rgba,
    /// Caution state.
    pub warn: Rgba,
    /// Failure/destructive state.
    pub error: Rgba,
    /// Informational state.
    pub info: Rgba,
    /// Selected-row/text background (accent-tinted, readability-guarded).
    pub selection_bg: Rgba,
    /// Text color on top of `selection_bg` (usually `text`).
    pub selection_fg: Rgba,
    /// Soft/block cursor color when the engine draws its own cursor.
    pub cursor: Rgba,
    /// Hyperlink color (renders with underline; usually `info`-family).
    pub link: Rgba,
    /// Dim multiplier for cell-space drop shadows (carries alpha; the
    /// compositor multiplies covered cells toward this color).
    pub shadow: Rgba,
    /// `shadow` pre-composited over `bg` (opaque): the ground for
    /// cell-level elevation strips (Block::shadow) — widgets never do
    /// the compositing themselves (RT1-9b).
    pub shadow_ground: Rgba,
    /// Categorical ramp for charts/sparklines, hue-separated, all legible
    /// on `bg`. Indexing convention: 0 accent, 1 info, 2 ok, 3 warn,
    /// 4 error, 5 accent_alt, 6 teal (ok<->info midpoint), 7 orange
    /// (error<->warn midpoint).
    pub chart: [Rgba; 8],
    /// Syntax highlighting inks (cycle 6): derived per theme from the
    /// audited accent/semantic family, contrast-guarded against
    /// `surface_raised` (the harder code ground on BOTH polarities —
    /// see the derivation rules in `registry.rs` and doc §1.6).
    pub syntax_keyword: Rgba,
    pub syntax_string: Rgba,
    pub syntax_number: Rgba,
    /// Deliberately secondary (3:1 class): comments recede.
    pub syntax_comment: Rgba,
    pub syntax_type: Rgba,
    pub syntax_func: Rgba,
    /// Structural ink: brackets, operators, delimiters.
    pub syntax_punct: Rgba,
}

impl Default for TokenSet {
    /// The engine default is `abstract-dark` — the AbstractFramework
    /// house palette.
    fn default() -> Self {
        crate::theme::registry::default_theme().tokens
    }
}

/// Stable identifier for each token — the runtime tooling surface
/// (theme editors, debug overlays, config files).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum TokenId {
    Bg,
    Surface,
    SurfaceRaised,
    Overlay,
    Border,
    BorderFocus,
    Text,
    TextMuted,
    TextFaint,
    Accent,
    AccentAlt,
    Ok,
    Warn,
    Error,
    Info,
    SelectionBg,
    SelectionFg,
    Cursor,
    Link,
    Shadow,
    ShadowGround,
    Chart0,
    Chart1,
    Chart2,
    Chart3,
    Chart4,
    Chart5,
    Chart6,
    Chart7,
    SyntaxKeyword,
    SyntaxString,
    SyntaxNumber,
    SyntaxComment,
    SyntaxType,
    SyntaxFunc,
    SyntaxPunct,
}

impl TokenId {
    /// Every token, in declaration order. Iteration order is stable and
    /// part of the tooling contract.
    pub const ALL: [TokenId; 36] = [
        TokenId::Bg,
        TokenId::Surface,
        TokenId::SurfaceRaised,
        TokenId::Overlay,
        TokenId::Border,
        TokenId::BorderFocus,
        TokenId::Text,
        TokenId::TextMuted,
        TokenId::TextFaint,
        TokenId::Accent,
        TokenId::AccentAlt,
        TokenId::Ok,
        TokenId::Warn,
        TokenId::Error,
        TokenId::Info,
        TokenId::SelectionBg,
        TokenId::SelectionFg,
        TokenId::Cursor,
        TokenId::Link,
        TokenId::Shadow,
        TokenId::ShadowGround,
        TokenId::Chart0,
        TokenId::Chart1,
        TokenId::Chart2,
        TokenId::Chart3,
        TokenId::Chart4,
        TokenId::Chart5,
        TokenId::Chart6,
        TokenId::Chart7,
        TokenId::SyntaxKeyword,
        TokenId::SyntaxString,
        TokenId::SyntaxNumber,
        TokenId::SyntaxComment,
        TokenId::SyntaxType,
        TokenId::SyntaxFunc,
        TokenId::SyntaxPunct,
    ];

    /// Chart token for slot `i` (clamped to the last slot — total function,
    /// callers index from fixed-size data).
    pub const fn chart(i: u8) -> TokenId {
        match i {
            0 => TokenId::Chart0,
            1 => TokenId::Chart1,
            2 => TokenId::Chart2,
            3 => TokenId::Chart3,
            4 => TokenId::Chart4,
            5 => TokenId::Chart5,
            6 => TokenId::Chart6,
            _ => TokenId::Chart7,
        }
    }

    /// Canonical snake_case name (config files, theme editors, docs).
    pub const fn name(self) -> &'static str {
        match self {
            TokenId::Bg => "bg",
            TokenId::Surface => "surface",
            TokenId::SurfaceRaised => "surface_raised",
            TokenId::Overlay => "overlay",
            TokenId::Border => "border",
            TokenId::BorderFocus => "border_focus",
            TokenId::Text => "text",
            TokenId::TextMuted => "text_muted",
            TokenId::TextFaint => "text_faint",
            TokenId::Accent => "accent",
            TokenId::AccentAlt => "accent_alt",
            TokenId::Ok => "ok",
            TokenId::Warn => "warn",
            TokenId::Error => "error",
            TokenId::Info => "info",
            TokenId::SelectionBg => "selection_bg",
            TokenId::SelectionFg => "selection_fg",
            TokenId::Cursor => "cursor",
            TokenId::Link => "link",
            TokenId::Shadow => "shadow",
            TokenId::ShadowGround => "shadow_ground",
            TokenId::Chart0 => "chart0",
            TokenId::Chart1 => "chart1",
            TokenId::Chart2 => "chart2",
            TokenId::Chart3 => "chart3",
            TokenId::Chart4 => "chart4",
            TokenId::Chart5 => "chart5",
            TokenId::Chart6 => "chart6",
            TokenId::Chart7 => "chart7",
            TokenId::SyntaxKeyword => "syntax_keyword",
            TokenId::SyntaxString => "syntax_string",
            TokenId::SyntaxNumber => "syntax_number",
            TokenId::SyntaxComment => "syntax_comment",
            TokenId::SyntaxType => "syntax_type",
            TokenId::SyntaxFunc => "syntax_func",
            TokenId::SyntaxPunct => "syntax_punct",
        }
    }

    /// Reverse of [`name`](TokenId::name). Unknown names return `None` —
    /// callers own their fallback (and its `#FALLBACK` label).
    pub fn from_name(name: &str) -> Option<TokenId> {
        TokenId::ALL.iter().copied().find(|id| id.name() == name)
    }
}

impl TokenSet {
    /// Resolve a token by id.
    pub fn get(&self, id: TokenId) -> Rgba {
        match id {
            TokenId::Bg => self.bg,
            TokenId::Surface => self.surface,
            TokenId::SurfaceRaised => self.surface_raised,
            TokenId::Overlay => self.overlay,
            TokenId::Border => self.border,
            TokenId::BorderFocus => self.border_focus,
            TokenId::Text => self.text,
            TokenId::TextMuted => self.text_muted,
            TokenId::TextFaint => self.text_faint,
            TokenId::Accent => self.accent,
            TokenId::AccentAlt => self.accent_alt,
            TokenId::Ok => self.ok,
            TokenId::Warn => self.warn,
            TokenId::Error => self.error,
            TokenId::Info => self.info,
            TokenId::SelectionBg => self.selection_bg,
            TokenId::SelectionFg => self.selection_fg,
            TokenId::Cursor => self.cursor,
            TokenId::Link => self.link,
            TokenId::Shadow => self.shadow,
            TokenId::ShadowGround => self.shadow_ground,
            TokenId::Chart0 => self.chart[0],
            TokenId::Chart1 => self.chart[1],
            TokenId::Chart2 => self.chart[2],
            TokenId::Chart3 => self.chart[3],
            TokenId::Chart4 => self.chart[4],
            TokenId::Chart5 => self.chart[5],
            TokenId::Chart6 => self.chart[6],
            TokenId::Chart7 => self.chart[7],
            TokenId::SyntaxKeyword => self.syntax_keyword,
            TokenId::SyntaxString => self.syntax_string,
            TokenId::SyntaxNumber => self.syntax_number,
            TokenId::SyntaxComment => self.syntax_comment,
            TokenId::SyntaxType => self.syntax_type,
            TokenId::SyntaxFunc => self.syntax_func,
            TokenId::SyntaxPunct => self.syntax_punct,
        }
    }

    /// Overwrite a token by id (theme editors, user overrides).
    pub fn set(&mut self, id: TokenId, color: Rgba) {
        match id {
            TokenId::Bg => self.bg = color,
            TokenId::Surface => self.surface = color,
            TokenId::SurfaceRaised => self.surface_raised = color,
            TokenId::Overlay => self.overlay = color,
            TokenId::Border => self.border = color,
            TokenId::BorderFocus => self.border_focus = color,
            TokenId::Text => self.text = color,
            TokenId::TextMuted => self.text_muted = color,
            TokenId::TextFaint => self.text_faint = color,
            TokenId::Accent => self.accent = color,
            TokenId::AccentAlt => self.accent_alt = color,
            TokenId::Ok => self.ok = color,
            TokenId::Warn => self.warn = color,
            TokenId::Error => self.error = color,
            TokenId::Info => self.info = color,
            TokenId::SelectionBg => self.selection_bg = color,
            TokenId::SelectionFg => self.selection_fg = color,
            TokenId::Cursor => self.cursor = color,
            TokenId::Link => self.link = color,
            TokenId::Shadow => self.shadow = color,
            TokenId::ShadowGround => self.shadow_ground = color,
            TokenId::Chart0 => self.chart[0] = color,
            TokenId::Chart1 => self.chart[1] = color,
            TokenId::Chart2 => self.chart[2] = color,
            TokenId::Chart3 => self.chart[3] = color,
            TokenId::Chart4 => self.chart[4] = color,
            TokenId::Chart5 => self.chart[5] = color,
            TokenId::Chart6 => self.chart[6] = color,
            TokenId::Chart7 => self.chart[7] = color,
            TokenId::SyntaxKeyword => self.syntax_keyword = color,
            TokenId::SyntaxString => self.syntax_string = color,
            TokenId::SyntaxNumber => self.syntax_number = color,
            TokenId::SyntaxComment => self.syntax_comment = color,
            TokenId::SyntaxType => self.syntax_type = color,
            TokenId::SyntaxFunc => self.syntax_func = color,
            TokenId::SyntaxPunct => self.syntax_punct = color,
        }
    }

    /// Iterate `(id, color)` pairs in [`TokenId::ALL`] order.
    pub fn iter(&self) -> impl Iterator<Item = (TokenId, Rgba)> + '_ {
        TokenId::ALL.iter().map(move |id| (*id, self.get(*id)))
    }

    /// Chart ramp slot, clamped to the last entry — a total function so
    /// series indexing from arbitrary data can never panic (mirrors
    /// [`TokenId::chart`]).
    pub fn chart(&self, i: usize) -> Rgba {
        self.chart[i.min(self.chart.len() - 1)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_set_round_trips_every_token() {
        let mut t = TokenSet::default();
        for (i, id) in TokenId::ALL.iter().enumerate() {
            let probe = Rgba::new(i as u8, 7, 42, 255);
            t.set(*id, probe);
            assert_eq!(t.get(*id), probe, "token {:?} lost its write", id);
        }
    }

    #[test]
    fn names_are_unique_and_round_trip() {
        for id in TokenId::ALL {
            assert_eq!(TokenId::from_name(id.name()), Some(id));
        }
        let mut names: Vec<&str> = TokenId::ALL.iter().map(|i| i.name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), TokenId::ALL.len());
        assert_eq!(TokenId::from_name("not_a_token"), None);
    }

    #[test]
    fn default_is_abstract_dark() {
        let t = TokenSet::default();
        // The house ground and accent, ported from AbstractUIC theme.css.
        assert_eq!(t.bg, Rgba::from_hex("#1a1a2e").unwrap());
        assert_eq!(t.accent, Rgba::from_hex("#e94560").unwrap());
    }

    #[test]
    fn chart_slot_clamp_is_total() {
        assert_eq!(TokenId::chart(0), TokenId::Chart0);
        assert_eq!(TokenId::chart(7), TokenId::Chart7);
        assert_eq!(TokenId::chart(200), TokenId::Chart7);
        // The value accessor mirrors the id clamp (and coexists with the
        // public field: `t.chart` is the array, `t.chart(i)` the method).
        let t = TokenSet::default();
        assert_eq!(t.chart(0), t.chart[0]);
        assert_eq!(t.chart(7), t.chart[7]);
        assert_eq!(t.chart(999), t.chart[7]);
    }
}
