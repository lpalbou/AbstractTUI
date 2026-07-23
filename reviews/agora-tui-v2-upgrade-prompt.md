# Prompt: adopt Drawer (and know about PageHost) in agora-tui (AbstractTUI 0.2.12)

Copy everything below the line into the agora-tui session.

---

Your project `~/projects/gh/agora-tui` is built against
`abstracttui = "0.2.11"` (Cargo.toml:15) — you adopted the Disclosure
wave the day it shipped and deleted your click-toggle workaround.
**AbstractTUI 0.2.12 is released** (crates.io +
https://github.com/lpalbou/AbstractTUI, ~1,917 tests green). Two
higher-level containers landed: **`Drawer`** (edge-anchored overlay
panels hosting full pages — modal or passive, scrim, slide
transitions at zero idle cost) and **`PageHost`** (a page-level tab
host). For a multi-pane *watcher*, the Drawer is the one that changes
what you can offer; PageHost is a fit only if you grow beyond
simultaneous panes — honest assessment below.

**Dependency first**: set `abstracttui = "0.2.12"` in `Cargo.toml`.
No breaking API changes 0.2.11 → 0.2.12 (`cargo semver-checks` clean;
196 checks). MSRV stays 1.87, so your MSRV-floor comment
(Cargo.toml:5, citing 0.2.8) only needs the version string refreshed.
Nothing below touches your hub transport, your state fold, or your
Disclosure cards.

Work through these in order, running your suite after each.

## 1. The win: a message reader Drawer (right edge, modal)

Today a long message lives entirely inside its capped Disclosure card
(`src/ui/panes.rs:392` — `Disclosure::new(card_title(row))
.detail(card_detail(row, seat))`): reading it means unfold + scroll
inside the pane band, with the body competing for rows against every
other card. You are a *reader's* app; give reading a surface.

Add a right **modal** Drawer that opens on the selected message:

```rust
use abstracttui::app::{Drawer, DrawerEdge, DrawerFocus, DrawerSize};

let reader = Drawer::new(DrawerEdge::Right)
    .size(DrawerSize::Percent(0.55))
    .focus(DrawerFocus::Modal)          // Esc closes; focus-trapped
    .title("Message")                    // themed header; ✕ is mouse-only
    .install(cx, move |mount| reader_page(mount, selected_msg));
// reader is a DrawerHandle: open/close/toggle/is_open
```

- **Content**: the full body through your existing rich pipeline
  (`RichTextView` / the md path your cards use), the full
  `card_detail` state line (`panes.rs:196` — glyphs + stamp), sender/
  channel/seq, and the reply chain if your fold already links
  `reply_to` (walk it upward; each ancestor one capped block). Wrap
  the body in a `Scroll` — a Drawer hosts full pages, scrolling
  included (pinned by the engine's `wave_drawers` Feed-in-drawer
  test).
- **Key**: `o` (open) on the selected card — `Enter` stays fold-toggle
  (your `ui/mod.rs:301` binding). `Esc` closes (engine-owned). Add
  `o read` to the footer legend (`ui/mod.rs:393` area).
- **Selection source**: your `move_selection` plumbing already names
  the focused pane's selected row — the drawer's builder reads that
  signal; no new state shape needed. State survives via app-owned
  signals (the drawer disposes its mount scope on close — the
  documented Tabs rule; your fold state is already outside, so you
  are compliant by construction).
- **Stacking is lawful and pinned**: your activity `Toast`
  (`ui/mod.rs:116` area) renders ABOVE an open drawer (toasts z 2000 >
  drawer band 800-807); a future modal (e.g. confirm) from inside the
  drawer layers above it too. Resize while open re-clamps, never
  dismisses.

## 2. Optional: the sidebar as a summonable left Drawer

`SIDEBAR_W = 26` (`src/ui/sidebar.rs:17`) is docked permanently —
on an 80-col terminal it costs a third of your width, which is
plausibly why `z` zoom exists (`ui/mod.rs:52,318`). Option, not a
defect: keep the docked sidebar at comfortable widths, and at narrow
widths (viewport signal < ~100 cols) swap it for a **passive** left
Drawer summoned on `s`:

```rust
let nav = Drawer::new(DrawerEdge::Left)
    .size(DrawerSize::Cells(26))
    .focus(DrawerFocus::Passive)   // glanceable; keys stay in the panes
    .install(cx, move |mount| sidebar::sidebar(mount, ...));
```

Passive mode never takes a scrim and leaves the keyboard with your
panes (click-to-focus if the user wants to interact). Your sidebar
component function is reusable as-is — a Drawer hosts the same View.
If you'd rather not maintain two modes, skip this section; it's
space reclamation, not correctness.

## 3. PageHost: know it exists, don't retrofit it

Honest assessment: your core UX is *simultaneous* multi-pane watching
— that is not page switching, and PageHost would be a regression for
it. Adopt PageHost only if you grow a genuinely second full view
(a Board/work-items page, a Leaderboard page, a channel-fs Files
page). If that day comes: your `Tab` = cycle-panes binding
(`ui/mod.rs:277`) does NOT collide with PageHost's defaults
(Ctrl+PgUp/PgDn, capture-phase), and its digit jumps are opt-in so
your 1-9 pane jumps (`ui/mod.rs:341` loop) keep working. Docs:
`docs/api.md` "widgets::PageHost".

## 4. Testing: `CaptureTerm::push_resize` is new

0.2.12 adds resize injection to the headless harness. Your suite can
now pin narrow-width behavior (the §2 breakpoint, footer legend
wrapping, pane collapse) — previously untestable without a live PTY.

## 5. Migration checklist (one commit-sized step each, suite after)

1. Bump to 0.2.12; suite green (no API breaks).
2. Reader drawer (§1): install + `o` key + footer legend entry.
3. (Optional) narrow-width sidebar drawer (§2).
4. (Optional) a `push_resize` test pinning whichever of §1/§2 landed.
5. File anything that fought you to the engine's field band
   (`abstracttui/docs/backlog/proposed/field-agora/`, 0800-0890) —
   your 0850 Disclosure ask shipped in one cycle; the band works.

Everything above was verified against your working tree (agora-tui at
`abstracttui = "0.2.11"`, Disclosure cards live in panes.rs, docked
26-col sidebar, Tab/digit pane navigation) and the engine's 0.2.12
surface on 2026-07-24. `Drawer` docs: `docs/api.md` "app::Drawer";
demoed in `examples/shell.rs` + `examples/drawers.rs`; acceptance
battery `tests/wave_shell_accept.rs` (PageHost + modal right Drawer +
passive left Drawer + Modal-from-drawer + Toast, driven end-to-end
through the real Driver).
