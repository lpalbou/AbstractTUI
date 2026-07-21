# DESIGN cycle-7 — visual pass notes (per example, before -> after)

Method: pty runs (`script -q` + `stty`, truecolor env, held-open stdin —
NOTE for everyone's harnesses: KERNEL's keyboard fix means a `/dev/null`
stdin now delivers a REAL Ctrl+D through the pty and correctly skips/
quits; feed `sleep N |` instead) at 80x24, 120x35, 160x45, plus headless
BufferCanvas row-dumps for pixel-exact inspection (the throwaway
`zz_probe` pattern — deleted after use, worth institutionalizing in the
rig).

## gallery (new this cycle)

- BEFORE: third column collapsed to a 2-cell sliver at 112 cols — flex
  distributed leftover by GROW but the widgets column's intrinsic basis
  (badges row ≈ 45 cells) ate the space; token rows overflowed their
  34-col panel ("● info" bled through the border, selection chip
  clipped mid-word).
- AFTER: columns use `basis(Cells(0))` + weighted grow (1.2/1.0) —
  pure-share distribution, no intrinsic hostage-taking; token rows
  rewritten to fit 30 interior cells (`●ok ●wrn ●err ●inf`, `▅▇` ramp
  pairs, " sel " chip). Responsive: below 104 cols the content column
  bows out entirely (viewport signal) instead of squeezing everything
  — 80x24 shows tokens+widgets, still composed.
- Root-cause fix shipped for everyone: rich spans now CLIP at their
  rect's right edge (`richtext::print_span_clipped`, used by richtext/
  markdown/code) — a long code line was eating its Block's right border.
  Draw closures see the whole canvas; rect discipline is the widget's
  job, and two of mine were sloppy.

## dashboard

- BEFORE: flat panels — surface fill only, no depth; visually a demo.
- AFTER: all five panels (nav, traffic, load, events, sessions) carry
  `Block::shadow(shadow_ground)` elevation. With header strip + chart
  inks + selection rows unchanged, the accent economy holds: accent
  appears exactly in the mark, focus strokes, chart-0 and key legend.
  Startup notices now surface as staggered auto-dismissing toasts
  (honest-degradation UX); `--caps` prints the capability report and
  exits (no tty).
- At 80x24: the log pane tail + table clip rows before the guard fires —
  acceptable (guard is 40x10), rhythm holds.

## themes

- AFTER: gains the live PREVIEW pane (≥96 cols): a miniature app mock —
  title strip, three text tiers, semantic dots, selected row, action
  chip, link, progress bar, border-vs-focus strokes — rendered entirely
  in the SELECTED theme's tokens on its own ground before you apply.
  Selection stays instant (pure draw, no widget rebuilds).

## splash (2D + 3D)

- 2D fallback now runs a REAL deterministic ParticleField: 5-spark kicks
  as each mark line lands (alternating stroke edges) + the 12-spark
  alignment burst at 0.9s, gravity-arced, drawn BENEATH the strokes so
  the letterform never breaks; mark strokes draw stroke-cells-only so
  sparks show through the counters. Fixed-step simulation = pixel-equal
  replay at any t (test-pinned). Pty: 185 spark glyph occurrences across
  the 2s run, beats unchanged, drift test green.
- 3D path re-eyeballed same harness: mark + wordmark + hint all present,
  clean bracketing, `completed (2.0s)`. Constants stay LOCKED.

## hello / components / grid / effects / viewer3d / images / widgets

- hello: no change — 54 lines is the point.
- components/grid: born this cycle-6/7 with elevation + spacing already
  in house style; verified at 3 sizes, no nits worth churn.
- effects: verified; layers are launch-viewport-static (known,
  documented) — unchanged.
- viewer3d/images: `--caps` added; visual state good. images' protocol
  placement is still fixed-position (known).
- widgets: badges row clips first at narrow widths (before the input) —
  acceptable order of loss; noted, no change.

## For other owners (cite §3)

- REACT/TextInput: at very tight widths the placeholder vanishes before
  the frame does — consider eliding placeholder text (…) rather than
  dropping it (§3.2 placeholder row). P3, cosmetic.
- Spatial nav (`focus_next_in`) had not landed by my close — dashboard
  panes still Tab-only; adoption is a two-line change when it arrives.
- REACT/notices: `push_startup_notice` + `startup_notices()` work, but
  the ENGINE's own pushes (input degradation, caps summary) happen in
  `run_prepared` — AFTER `mount`, so a mount-time snapshot can never
  see them. Examples currently surface app-pushed notices only
  (dashboard toasts, viewer3d warn footer) and derive their own caps
  lines. Request: a reactive read (`use_startup_notices(cx) ->
  Signal<Vec<String>>`) or an on-notice callback, so engine lines reach
  the UI without polling.
- Harness note: a plain `cargo test` with a never-EOF stdin (agent
  shells, some CI wrappers) hangs — something under the test tree reads
  stdin. `cargo test </dev/null` completes in seconds. Worth a hunt
  (REDTEAM) — bounded stdin is a rig invariant. (Observed mid-session;
  final tree runs 1184/0 with stdin closed.)
