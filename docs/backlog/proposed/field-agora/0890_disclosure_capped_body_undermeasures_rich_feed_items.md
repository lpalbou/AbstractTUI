# Proposed: Disclosure's capped body region under-measures Feed bodies containing rich items — the last rows clip

## Metadata
- Created: 2026-07-24
- Status: Proposed (field-agora, agora-tui build — found adopting 0.2.11's Disclosure)
- Severity: P2 — silently clips body content; app-side workaround (uncapped region + block-level max_rows) holds
- Class: bug

## Context
agora-tui's card bodies open with a RICH meta line (colored spans) above
the message text — one `FeedItem::rich_lines(meta).block(Text(body))`
(also reproduced with two items: rich then text). Under
`max_body_rows(n)` (any positive cap), the body region sizes to the
measured extent — and consistently comes up SHORT: with meta(1 rich
row) + text(2 rows), the region settles at 2 rows and the last text row
never paints; with meta(1) + rich placeholder(1), it settles at 1 and
the placeholder never paints. The pattern matches "rich rows contribute
0/partially to the measured extent" (the same family as finding 0860:
rich surfaces lacking measure).

The engine's own `Disclosure::text` (a single TEXT item) renders
completely under the same cap, and BOTH shapes render completely with
`max_body_rows(0)` (the uncapped region takes natural height through
the reactive Feed measure) — the defect is specific to the CAPPED
region + rich-item extent interaction.

## Current code reality (0.2.11)
- `src/widgets/disclosure.rs:370-401` — the capped path sizes a
  `style_signal` region from `Scroll::extent_signal`'s measured
  `(w, h)`; the extent under-reports when the inner Feed's items
  include rich blocks.
- Repro'd headlessly (CaptureTerm, 3+ settle turns — beyond the
  documented one-turn settle) in the app's suite during migration;
  minimal shape below.

## Repro (~15 lines against 0.2.11)
```rust
let open = cx.signal(false); // expanded
Disclosure::new("card")
    .folded(open)
    .max_body_rows(8)
    .body(move |gcx| {
        let fs = FeedState::new(gcx);
        fs.push("card",
            FeedItem::rich_lines(vec![RichLine::from_spans(vec![
                Span::new("META", Style::new().fg(t.accent)),
            ])])
            .block(FeedBlock::Text("BODY-1\nBODY-2".into())));
        Feed::new(&fs).gap(0).element(gcx, &t).build()
    })
    .view(cx)
// Renders META + BODY-1; BODY-2 clips at every turn count.
// max_body_rows(0) renders all three rows.
```

## Workaround in the field (delete when fixed)
`src/ui/panes.rs` in agora-tui: `max_body_rows(0)` (uncapped region) +
`FeedItem::max_rows(24)` block-level caps with the honest "+K more
lines" marker — bodies bound correctly but lose the in-card scrollbar
(the feature the capped region exists for). The engine fix — rich rows
counted in the measured extent — restores `max_body_rows(24)` and
in-card scrolling for long hub reports.
