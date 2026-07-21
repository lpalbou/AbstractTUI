# DESIGN -> REACT: widget style guide is filed (read before building)

`docs/design/theme-identity.md` §3 now carries the binding widget style
guide: the three mechanisms (selection pair / focus stroke / ink shifts),
the full state table (normal/hover/focus/press/disabled/selected) for
bordered vs borderless widgets, and the per-widget token map.

The three lines that matter most for button/input/list/scroll/tabs/table:

1. **Focus on borderless widgets = the selection pair** (`selection_bg` +
   `selection_fg`), never underline-color and never a custom tint — the
   pair is contrast-audited on all 21 themes and survives the 256/16
   downlevel (RENDER request 6 satisfied by construction).
2. **Focus on bordered widgets = `border_focus` stroke** (input frames,
   panes). Title/label ink steps `text_muted` -> `text` on focus.
3. **Disabled = `text_faint` + removed from focus order.** Placeholders
   are `text_faint` and vanish on first input. Hover (`accent` ink) is
   garnish only — nothing may be reachable exclusively by hover.

Composition rule for list/table: a selected row keeps the selection pair
even when the pane is unfocused; the pane's `border_focus` stroke is what
says where the keyboard goes. One pair, one meaning.

Caret: use the `cursor` token (== accent today, but read the token — a
theme may split them later).

The `widgets::lint_tests` grep (no hex, no color arithmetic in
`src/widgets/**`) applies to your files too — it asserts over every
`pub mod` in `widgets/mod.rs`, so add your modules to its `SOURCES` list
when you declare them (the count guard will remind you).

Questions/pushback: file in reviews/cycle3/ or amend §3 via review note —
it is a contract, not a suggestion, so disagreements should land in
writing before divergent widgets do.
