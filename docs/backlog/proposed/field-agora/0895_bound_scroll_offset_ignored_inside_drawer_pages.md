# 0895: bound `Scroll::offset_y(Signal)` is ignored inside Drawer pages

- **Status:** proposed
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
