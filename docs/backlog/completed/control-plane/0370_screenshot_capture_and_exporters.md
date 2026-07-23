# 0370 — Screenshot capture + exporters (text / ANSI / SVG)

Status: completed 2026-07-24 (filed and shipped in one wave)
Owner: engine (render value + app verb + testing bridge)
Effort: M

Filed in the control-plane band because a screenshot IS the observe
primitive this track exists for ("make a running AbstractTUI
application observable... by a test harness, by an agent"), shipped
ahead of 0310/0320 because it needs none of their machinery: the value
type + exporters are engine-side, and the future bus/wire "observe"
verb is a serialization of this same value (seam named in §Consumers).

## The need (four consumers, one value)

1. **Debugging** — "what does the app ACTUALLY show right now?" The
   bordered `render::snapshot` dumps answer per-surface; nothing
   captured the composed, last-presented screen of a running app.
2. **Documentation** — `docs/captures/` held `.txt`/`.styled.txt`
   stills; nothing GitHub-renderable (README/report artifacts).
3. **Test artifacts** — application seats built on this engine are
   tasked to deliver "actual tests, screenshots and reports proving it
   fully works": exporters callable from headless tests are the killer
   feature.
4. **The control-plane observe verb** (0310/0320, future) — needs a
   capture value to serialize; inventing it at wire-design time would
   invert ownership.

## Feasibility survey (the verdict that authorized the build)

**Verdict: CLEAR — two truthful capture sources exist, both already
load-bearing for other features; the exporters are pure functions over
one value type. No conflict found.**

- **The live source is `Driver.frame`** (`src/app/driver.rs`): the
  compositor's flatten target — phase P presents from it, phase S swaps
  it into `prev`. It IS "what was last presented", and the selection
  feature already reads it with exactly that claim
  (`extract_text(&self.frame, ..)` — "exactly what the highlight
  showed"). Candidates rejected: `prev` (poisoned with impossible
  colors on resize/caps-upgrade — reading it would lie), the presenter
  (holds no frame; diff runs -> bytes with a pen/cursor only).
- **The rig source is `VtScreen`'s grid** (`src/testing/`): already the
  currency of goldens and the diff/present property; `CaptureTerm`
  exposes it (`screen()`). A capture from it answers "what did the
  emitted bytes actually produce" — the byte-side truth.
- **The two sources agree by construction** for presenter-faithful
  content (the diff/present correctness property is exactly that
  claim), so one value type serves both. Divergence is possible only
  for degenerate stylings the presenter canonicalizes (underline color
  without an underline, UNDERLINE+UNDERCURL both set) — documented, not
  papered over.
- **Protocol images**: cells under kitty/iTerm2/sixel placements are
  NOT the picture. The placement bookkeeping is reachable on the live
  path (`overlays` image entries x `ImageSession::slot_info(id) ->
  (Channel, Rect)`), so the capture can carry labeled regions instead
  of lying. VT-side captures cannot see placements (the rig consumes
  protocol payloads as counted, unmodeled string frames) — documented
  asymmetry.
- **Roundtrip provability**: the VT interpreter models exactly what an
  ANSI exporter needs to emit (SGR 38;2/48;2, the 58 colon form, 4:3
  undercurl, CR/LF/CUP), and the presenter's SGR transition builders
  are `pub(crate)` — reusable rather than re-derived.

## What shipped

- **`render::Screenshot` + `ShotCell`** (`src/render/screenshot.rs`):
  row-major grid of `{glyph, fg, bg, ul, attrs}` with plain value
  semantics (`Clone`/`PartialEq` — goldens and roundtrip proofs compare
  whole captures). Colors are the wire vocabulary: `None` = terminal
  default, `Some` = the exact opaque RGB the terminal is told (compositing
  alpha resolves before presentation; the capture normalizes exactly as
  the presenter does). Blank and printed-space cells canonicalize to one
  representation (visually and wire-identical — and the roundtrip law
  REQUIRES it: a replayed blank comes back as a printed space). Wide
  glyphs keep explicit continuation cells (`width() == 0`) so the grid
  stays column-addressable. Hyperlink ids are dropped (a visual capture
  has no click surface; the styled debug dumps show them).
- **Capture surfaces**: `Screenshot::from_surface(&Surface)` (render),
  `Driver::screenshot()` (`src/app/driver_screenshot.rs` — pure read of
  the composed frame + pixel-region stamping; `frame` became
  `pub(super)` for the sibling file, the `driver_images` pattern),
  `app::request_screenshot(cb)` (`src/app/screenshot.rs` — thread-local
  callback queue in the `request_full_redraw` drain shape, served in
  phase U BEFORE the frame decision so a key handler's request is
  served the same turn with the screen as the user saw it; `request_frame()`
  wakes idle loops for posted-job requests; the served frame renders
  nothing new, so the capture emits zero bytes), and
  `VtScreen::screenshot()` (`src/testing/vt_dump.rs` — explicit
  flag-by-flag attrs mapping: the two `Attrs` types share names, NOT bit
  positions). `Screenshot` + `request_screenshot` join the prelude.
- **Exporters** (pure, deterministic): `to_text()` (plain UTF-8,
  trailing blanks trimmed, identical to `VtScreen::to_text`),
  `to_ansi()` (replayable with `cat`; minimal SGR via the presenter's
  own shorter-of incremental/reset builders; `SGR 0` + CRLF row
  separation; trailing fully-default blanks trimmed per row; no
  trailing newline), `to_svg()`/`to_svg_with(fg, bg)`
  (`src/render/screenshot_svg.rs`: merged per-run background rects,
  column-pinned `textLength` text runs — wide glyphs run alone so font
  fallback cannot shear columns — explicit underline/strike rects,
  REVERSE resolved at paint time, DIM as fill-opacity, HIDDEN paints
  background only, XML-escaped, integer cell metrics 9x18, viewBox
  sized to the grid). File conveniences `write_text/write_ansi/write_svg`.
- **Protocol-image honesty**: `Driver::screenshot()` stamps non-mosaic
  placements into `pixel_regions()` (clipped, public `add_pixel_region`
  for embedders); `to_svg` draws labeled placeholder veils ("image
  (pixels)"); text/ANSI stay cell-plane-verbatim with the limitation
  documented.

## The adversarial finding worth remembering (fixed pre-ship)

A linear ANSI export can make a terminal FUSE adjacent cells: a
trailing-ZWJ cluster arms join state that steals the next glyph
(`"a\u{200D}"` + `"x"` becomes one two-column cluster), two lone
regional indicators fuse into a flag, and ambiguous-width clusters
drift on terminals with the other width opinion. The presenter defends
by forgetting its cursor after "risky" clusters (RT1-7) — re-anchoring
breaks pending join state because any ESC clears it. `to_ansi` now
emits the same defense after any fusion-arming cluster
(`text::is_risky_cluster` + trailing regional indicator) when another
leader follows in the row — as `CHA` (column-absolute, row-RELATIVE),
not the presenter's `CUP`: the presenter owns the whole alt screen, but
a capture must replay from any scrollback position, so absolute row
addressing would jump a `cat` replay to the top of the viewer's
terminal (a second finding, caught reviewing the first fix). The hazard
is test-pinned with a negative premise (unanchored bytes DO fuse in the
VT model) and the fix with the roundtrip. Honest residue, documented:
a standalone skin-tone-modifier CELL adjacent to an emoji fuses by the
terminal's own rules — that content is unrepresentable in cells,
capture or no capture.

## Evidence

- `tests/wave_screenshot.rs` (10 tests): `roundtrip_hand_built_style_zoo`
  + `roundtrip_seeded_style_fuzz` (24 seeds x 80 random styled writes:
  export -> VT replay -> re-capture == original; zero unknown sequences),
  `adjacent_fusion_attackers_do_not_fuse_on_replay` (negative premise +
  anchored proof), `driver_and_vt_model_capture_the_same_screen` (full
  `==` between the two capture surfaces through the real driver, plus
  the roundtrip on presenter-produced bytes),
  `golden_scene_pins_all_three_exports` (byte-stable goldens:
  `tests/goldens/screenshot_scene_{text,ansi,svg}.txt`),
  `byte_channel_images_stamp_labeled_pixel_regions` (kitty placement ->
  region + SVG veil; mosaic -> no veil; VT-side empty),
  `request_screenshot_serves_key_handler_same_turn_then_idles` (e2e key
  binding; zero bytes emitted by the capture; idle stays zero),
  `unchanged_screen_captures_byte_identically`, and
  `huge_screen_export_costs_are_printed_and_sane` (300x100 dense:
  capture ~0.55 ms / text ~0.07 ms / ANSI ~0.43 ms / SVG ~0.87 ms;
  sizes 30 KB / 130 KB / 550 KB; roundtrip holds at scale). Unit pins in
  `src/render/screenshot_tests.rs` (13) + `app::screenshot::tests` (1):
  blank/space canonicalization, alpha normalization, minimal-SGR byte
  assertions, colored-trailing-run trim guard, XML escaping under
  attack glyphs, attr mapping incl. reverse/hidden/decoration colors,
  empty/1x1 grids, determinism.
- Demo: `examples/screenshot.rs` (interactive `s`-to-capture recipe;
  headless: both surfaces captured, agreement asserted, artifacts
  written, exit 0). The capture pipeline (`examples/capture`) now
  writes `<name>.svg` beside every still; the five deterministic app
  shots regenerated with SVGs (`docs/captures/*.svg`).
- Docs: `docs/api.md` "Screenshots & captures" (both surfaces, the
  key-binding recipe, the test-artifact recipe, honesty notes, the
  0310/0320 seam), README highlight bullet, `examples/README.md`
  entries, CHANGELOG under Unreleased, llms.txt / llms-full.txt
  re-spliced.

## Consumers / seams named

- The 0310 automation bus and 0320 wire protocol gain "observe" by
  serializing `Screenshot` exports (`to_text` for cheap assertions,
  `to_svg` for artifacts) — the value type is deliberately
  transport-free; nothing here binds them.
- App seats on the engine (abstractcode-tui class) get the
  test-artifact recipe: drive `Driver` + `CaptureTerm`, capture either
  surface, `write_svg` into the report directory.
