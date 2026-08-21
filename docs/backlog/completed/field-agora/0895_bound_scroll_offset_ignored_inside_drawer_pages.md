# 0895: bound `Scroll::offset_y(Signal)` is ignored inside Drawer pages

- **Status:** completed 2026-08-21 — fixed in `335aea1`, with the
  spun-off warm-start half fixed in `862525c` and documented in
  `12bb12a`. The reported CAUSE was wrong (it is not the Drawer) and
  the reported SYMPTOM was real; see the completion report at the
  bottom.
- **Band:** field-agora (agora-tui field reports)
- **Engine:** abstracttui 0.2.12
- **Severity:** P1 for keyboard-first drawer pages (the 0.2.12 headline
  use case) — the wheel path works, the app-state path renders nothing.

## What happened

agora-tui adopted the 0.2.12 reader Drawer per the engine's own upgrade
prompt (`reviews/agora-tui-v2-upgrade-prompt.md` §1): a right drawer
hosting the selected message with the body "wrapped in a Scroll".
Keyboard scrolling routes through an app-owned `Signal<i32>` bound with
`Scroll::offset_y(sig)` — the exact pattern our pane bands use, where
it works.

Inside the drawer page the binding is dead: the signal HOLDS the
written value (no clamp-writeback — writes stick), but the rendered
Scroll stays at the top. The same composition, character for
character, scrolls correctly when mounted in the root tree.

## Minimal repro (headless, 0.2.12)

```rust
fn composition(cx: Scope, offset: Signal<i32>) -> View {
    dyn_view_scoped(LayoutStyle::column().grow(1.0).gap(0), move |rcx| {
        let fs = FeedState::new(rcx);
        let body: String = (1..=30).map(|i| format!("line {i}"))
            .collect::<Vec<_>>().join("\n");
        fs.push("subject", FeedItem::text("meta row")
            .block(FeedBlock::Text(body)).max_rows(400));
        Element::new().style(LayoutStyle::column().grow(1.0))
            .child(Scroll::new(Feed::new(&fs).gap(0).view(rcx))
                .offset_y(offset)
                .scrollbar_auto_hide(true)
                .view(rcx))
            .build()
    })
}
```

- Mounted at the app root: `offset.set(10)` + one turn → rows 10..29
  visible. Correct.
- Mounted as a Drawer page (`Drawer::new(Right).size(Percent(0.55))
  .motion(ZERO).install(cx, |mount| composition(mount, offset))`,
  opened, settled): `offset.set(10)` + three turns → signal reads 10,
  screen still shows rows 0..N. The write is silently ignored.

Passive vs Modal focus makes no difference. `max_rows` capped vs
uncapped makes no difference.

## Why the acceptance battery missed it

`tests/wave_drawers.rs` pins Feed-in-drawer scrolling via the MOUSE
WHEEL over the panel — the Scroll's internal offset path. The bound
`offset_y(Signal)` path has no drawer-context pin. In a keyboard-first
terminal the bound path is the only way an app routes PgUp/PgDn into a
drawer page (a passive drawer never holds focus, and a modal drawer's
page still needs app-owned offset state to survive reopen).

## Suspected shape

The drawer page renders in an overlay tree. The Scroll's offset
binding is subscribed in whatever render/layout pass reads it — if
that subscription lands on the MAIN tree's damage tracking, a write
marks the main tree dirty but the OVERLAY tree's Scroll never re-reads
it. Wheel events mutate the Scroll's internal state directly inside
the overlay tree, which is why that path repaints.

## Ask

Bound `offset_y` (and `follow_tail`, if it shares the plumbing) should
work identically in overlay-hosted trees. A one-line addition to the
wave_drawers battery — drive the drawer's Scroll by signal instead of
wheel — reproduces it.

## Field workaround (shipped in agora-tui)

The reader page dropped `Scroll` entirely: it windows its own content
(`Vec<RichLine>`, slice at the app-owned offset, `RichTextView` with
wrap, a "▲ N above" marker row when scrolled). Works, but forfeits the
scrollbar and the engine's clamp — every drawer page with keyboard
scrolling will re-derive this until the binding works.

## Completion report (2026-08-21)

### The report's diagnosis was wrong, and its symptom was exactly right

"Bound `offset_y` is dead inside Drawer pages" pointed at the overlay
tree. It is not the Drawer, not `PageHost`, and not the overlay's
damage tracking. Isolated by elimination (`eb8549b`, `8e5088b`): a bare
`Scroll` + `offset_y` inside a Drawer does NOT reproduce. The defect is
`Feed` × `Scroll` across a REMOUNT — and a drawer page is simply the
most common way an app remounts one. Every reproduction in the report
is genuine; the drawer was the setting, not the cause. Filing the
symptom with the setting it was seen in is the right thing for a field
report to do, and it is why the shape had to be re-derived here rather
than taken.

### What shipped (`335aea1`) — two changes, both load-bearing

1. **The offset repair treats the first measurement after the `(0,0)`
   sentinel as PROVISIONAL and never clamps against it.** A
   width-dependent measurer cannot know its height until it has been
   drawn once, so the first solve reports `(w, 1)` and the old code
   trusted it — clamping the caller's restored offset to 0 before the
   real extent ever arrived. Every LATER change is still trusted, which
   is where a genuine shrink (first-app 0281) lands.
2. **The wrapper inset is clamped at RENDER time against the current
   extent**, and this is what makes (1) safe. The acceptance test is
   what found it: skipping the render clamp left the content parked
   outside the clip, and a culled child never draws, never discovers
   its width, and never corrects the extent it was culled for — the
   pane stayed VOID forever, waiting on a measurement that could only
   happen if it were visible. The old clamp had been load-bearing for
   VISIBILITY, not only for correctness. Clamping where content is
   DRAWN breaks that deadlock without touching the caller's signal.

Two gates that were planned and turned out to be dead ends, recorded
because both would have compiled and read plausibly:
`frame_tasks_pending()` counts a different queue from the timer
`after()` arms, so it would have protected nothing; and a frame-task
nudge would never have fired in the 0281 unit tests, whose settle
helper runs `run_due_timers` but not `run_frame_tasks`.

### The half that was NOT covered, spun off and then also fixed

`335aea1` deliberately left one case open as an `#[ignore]`d test with
a narrowed reason rather than a stale one: with a CALLER-BOUND
`Scroll::extent_signal` there is no `(0,0)` sentinel, so the warm value
itself spent the one-shot trust exemption and the provisional solve
arrived as a trusted second observation. Binding `extent_signal` to
survive a remount — which its own rustdoc says is what it is for —
therefore bought exactly nothing.

Fixed in `862525c`: the repair captures what the extent signal held at
BUILD time and treats any observation still equal to it as "nothing has
arrived yet this mount", so the exemption is spent on the first extent
to actually ARRIVE. This unifies the two spellings of one idea — the
`(0,0)` sentinel and a warm extent are both "no solve has run yet".
Fresh mounts are unchanged (the build value IS the sentinel); hint mode
is unchanged (the exemption starts spent).

`12bb12a` documents the one thing that fix does NOT cover: the extent
signal itself still carries the `(w, 1)` placeholder for one turn, so
chrome rendering "N more rows" straight off it shows one frame of
nonsense on mount. Cosmetic, one frame, nothing blocked on it — written
down rather than left to be rediscovered. It needs its own row if
anyone wants it fixed.

### Verification

`tests/scroll_remount_offset.rs` is fully un-ignored — 3/3 green, zero
`#[ignore]` left in the file. `cargo test --all` zero failures,
`cargo test --doc` 57 passed, fmt clean, `clippy --all-targets
--all-features` clean.

Mutation-verified, each one precise: trusting the provisional
measurement → RED; removing the render clamp → RED; reverting the
warm-start guard to sentinel-only turns ONLY
`extent_signal_warm_start_does_not_protect_offset` red and leaves the
other two green.

The thing this had to not break, checked rather than assumed: both
first-app 0281 tests asserting the repair WRITES the bound signal still
pass (`shrink_below_offset_reclamps_and_repaints_without_a_gesture`;
`viewport_growth_reclamps_a_hint_mode_offset`, 26 → 18), as does
`restored_offset_survives_startup_measurement`.

### For the field

The app-side workaround above can go: a drawer page can host a
`Scroll` with a bound `offset_y` and keep both the scrollbar and the
engine's clamp across reopen. That deletion is agora-tui's to make
against their real reader page; this item closes on the engine side.
