# Proposed: Disclosure's title row needs a rich-span slot — plain-string titles cannot carry a message card's identity language

## Metadata
- Created: 2026-07-24
- Status: Proposed (field-agora, agora-tui build — follow-up to 0850, filed while ADOPTING 0.2.11's Disclosure)
- Severity: P2 — the folded 95%-view loses the color anatomy the design review fought for; workaround ships
- Class: capability gap

## Context
agora-tui migrated every hub message to a real `Disclosure` (operator
directive) the day 0.2.11 shipped. The migration surfaced the one gap
between the widget and 0850's field spec: the title row is
`title: String` + `detail: String` — plain, single-ink (text / muted).
The message-card identity language lives exactly there when folded:
sender in a stable per-sender ink (chart slots), status BADGES
(`open`/`blocked` on raised ground, warn/ok inks), `→ you` in
accent_alt, tinted `▲/▼` tallies. A folded card — the state ~95% of
cards are in — now renders monochrome; the colored anatomy only appears
in the body's rich meta line after expansion.

0850's spec asked for "status chips, sender, seq/time, addressee,
decorations, and the message title" on the title surface; the cycle-3
design review (P1-3/P1-4) established WHY: chips = state, colored names
= identity, and folded one-liners are the scanning surface.

## Current code reality (0.2.11)
- `src/widgets/disclosure.rs:82-91` — `title: String`,
  `detail: Option<String>`; `draw_header` paints title in `text_fg`,
  detail in `muted`, glyph in `accent` — no span path.
- The engine HAS the currency: `render::RichText`/`RichLine` spans,
  already accepted by `FeedItem::rich_lines` and `RichTextView`.

## Repro
Fold any card whose identity matters: sender ink, status badge and
addressee tint all collapse to two inks.

## Workaround in the field (delete when fixed)
`src/ui/panes.rs` in agora-tui: a glyph vocabulary compensates in plain
text — `‼` critical title prefix, `open/blocked/✔ ⚡now ⏵next →you ◦n ✓
▲n ▼n` packed into the detail slot, a 1-col status-ink gutter drawn
OUTSIDE the widget as the card's color spine, and the full rich meta
line as the body's first row (visible on expansion). An engine
`Disclosure::title_rich(RichLine)` (or `title_spans`) — same truncate
rule, spans instead of one ink — deletes the compensation and restores
identity color to the folded view.
