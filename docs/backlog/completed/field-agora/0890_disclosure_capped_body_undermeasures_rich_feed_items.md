# Completed: Disclosure's capped body region under-measures Feed bodies containing rich items — the last rows clip

## Metadata
- Created: 2026-07-24
- Status: Completed (2026-08-21 — DOES NOT REPRODUCE on 0.4.0; closed by
  evidence, not by a fix. All three shapes the report names render
  completely and are now pinned as regressions in
  `tests/wave_disclosure.rs`, commit `fc0e2ac`; see the completion
  report at the bottom, including the one receipt that is NOT ours)
- Completed: 2026-08-21
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

## Completion report (2026-08-21, `fc0e2ac`)

**It does not reproduce on 0.4.0.** The report was filed against
0.2.11; something between then and 0.4.0 addressed it, and nothing in
this closure claims to know what — attributing a fix we cannot point
at would be worse than saying so.

The regression was written BEFORE touching `src/`, which is what makes
"does not reproduce" a result rather than a shrug: the tests were built
to go red on the reported defect, and they came up green on the first
run. All three shapes the report names render completely under a cap
larger than the body:

- (a) rich META + two text rows in ONE item — the headline case, which
  the report says settles at 2 rows and clips `BODY-2`;
- (b) two items, rich then text;
- (c) rich rows only, which the report says settles at 1.

Plus the two CONTROLS the report itself names as working — a pure-text
body under the same cap, and the same rich body uncapped — so a future
failure localises to text-vs-rich and capped-vs-uncapped instead of
just going red somewhere in Disclosure.

**Not reproducing is evidence, not a fix, and an unpinned fix is a fix
that regresses.** The tests landed anyway, and that is the whole point
of this closure: `tests/wave_disclosure.rs`,
`capped_body_measures_rich_feed_rows_and_clips_nothing`,
`capped_body_measures_rich_across_the_reports_other_shapes`,
`capped_body_with_a_pure_text_body_clips_nothing`,
`uncapped_body_renders_every_rich_row`.

**Falsified, because "nothing clips" is exactly the assertion that can
pass by rendering nothing at all.** Dropping the cap to 2 (below the
3-row body) turns the headline assertion RED — it genuinely detects a
clipped last row rather than an empty screen.

Gates at close: `cargo test --all` zero failures, `wave_disclosure`
12/12, fmt clean, `clippy --all-targets --all-features` clean.

**One receipt is missing and it is not ours to produce.** The
consumer-side proof — that agora-tui can delete the `src/ui/panes.rs`
workaround above and restore `max_body_rows(24)` with in-card
scrolling — belongs to agora-tui, against their real cards. Their card
shape may differ from the three in the report; only they can tell. This
item closes on the ENGINE side; if their real cards still clip, that is
a NEW report against 0.4.0 with a shape these tests do not cover, not a
reopening of this one.
