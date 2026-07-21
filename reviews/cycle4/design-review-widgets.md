# DESIGN cycle-4 conformance review — REACT + GFX3D widgets vs §3

Scope: `button, input, list, scroll, tabs, table` (REACT), `image,
viewport3d` (GFX3D), against `docs/design/theme-identity.md` §3 (the
binding style guide). Verdict up front: **one real violation (D4-1), one
plumbing gap (D4-2), two polish notes; everything else conforms —
several files are exemplary.**

## D4-1 (P2, REACT/table): header ink `accent_alt` under-floors on
## `surface_raised` grounds

`table.rs:102` sets `header_fg = t.accent_alt` over
`header_ground = t.surface_raised`. `accent_alt` is audited against
**bg** only (§1.3); on raised grounds it is unaudited, and it fails in
the shipped family: **nord** measures `#5e81ac` on `#434c5e` ≈ **2.1:1**
— below the 3:1 informational floor for a header that carries column
meaning + the sort indicator. solarized-dark scrapes by at ≈ 3.06:1;
one-dark ≈ 3.6:1, gruvbox ≈ 3.3:1.

Fix options, in preference order:
1. `header_fg = t.text_muted` (headers are secondary labels — §3.1 ink
   tiers; muted is the vocabulary for "label", and it clears floors on
   every theme's raised ground with margin), optionally + BOLD through
   `StyledCanvas` for weight;
2. keep an accent identity via `t.text` for the SORTED column only
   (state, not decoration) with muted siblings.
Do not "fix" by auditing accent_alt/raised — that would constrain 21
palettes for one widget's choice.

## D4-2 (P3, REACT/list+table): §3.2's composition rule has no plumbing

§3.2: a selected row keeps the selection pair even unfocused; "the
OWNING pane's `border_focus` says where the keyboard goes." `Block`
exposes `.focused(bool)` for exactly that — but `List`/`Table` keep
their focus signal internal, so an app CANNOT wire the pane stroke to
the widget's real focus. `Button` already takes the
`.focus_signal(sig)` route; please expose the same on List/Table (or an
`on_focus(bool)` callback). Until then the dashboard/gallery panes
cannot honestly show keyboard ownership — they render unfocused strokes
even while the list owns arrows.

## Polish notes (explicitly NOT violations)

- **Row hover** (list/table): §3.2 makes hover garnish-only, so its
  absence is conformant; when you want the polish, hovered row ink ->
  `accent` (never a bg change — bg is selection vocabulary).
- **Scrollbars** (list/scroll/table all draw track=`border`,
  thumb=`text_muted`): the guide had no scrollbar row — your choice was
  right, and I have now CODIFIED it in §3.3 so the three copies cannot
  drift.

## Conformance highlights (keep doing this)

- `input.rs` is the §3 reference implementation: frame stroke
  `border -> border_focus` (focus never expressed by ground), placeholder
  `text_faint` vanishing on first input, caret = `cursor` token,
  selection pair for selected text — every §3.2/§3.3 line, plus the
  in-file comment citing the guide.
- `tabs.rs`: active `text` + `border_focus` strip drawn as CELLS (not
  SGR underline — survives 16-color, §3.1 rule 1) with idle
  `text_muted`; the unit test literally asserts "strip ink is
  border_focus (§3.3)". This is what a binding guide looks like when it
  works.
- `button.rs`: `ButtonStyle`'s defaults are the §3.2 state table row for
  borderless widgets, field for field, disabled outside the focus order.
- `image.rs`/`viewport3d.rs` (GFX3D): placeholder/diagnostics in
  `text_faint` (decoration tier — correct), pixel content exempt by
  nature, zero color arithmetic outside the bitmap path; both files sit
  in the widgets lint and pass.

## Guide delta (mine, shipped this cycle)

§3.3's per-widget token map now covers the SHIPPED set (button/input/
list/table/tabs/scroll/chart + scrollbar row) instead of the
"incoming widgets" paragraph — the guide describes reality again.
D4-1's fix should land against that table.
