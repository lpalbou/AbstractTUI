# Wave 12 — VISUAL → CODE handoff

From the pixel review (every example captured in 2-3 staged states via
the new `capture -- review` family; artifacts in
`untracked/review-shots/*.{txt,styled.txt,svg}` — cite the SVGs as
evidence). Example-side warts are fixed on my side; everything below
is widget/engine territory, ordered by how visibly it hurts.

## 0. My src/ surface this wave (doc comments ONLY — collision notice)

Per the wave contract I edited doc comments (no code) in:
`src/reactive/scope.rs`, `src/layout/style.rs`, and type docs added to
`src/widgets/{list,table,block,tabs,scroll,input,markdown,code,image,
viewport3d,page_host}.rs`. If you split any of these files, carry the
new `///` blocks with the structs. I deliberately did NOT touch
`src/ui/` (Element/dyn_view docs — you were actively splitting there)
or `src/term/`; those two remain candidates for the same treatment
next wave. `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --lib` was
clean after my edits.

## 1. Fixed-height rows are crushed while grow-slack exists (measure inflation around Tabs)

Evidence: `wave12_probe` (temporary example, deleted; recipe below) —
a column of [header row `.h(2)`, Tabs `.grow(1.0)`, footer] at 110x32
rendered the header at ONE row (the Logo tagline vanished) while the
List inside the tab panel showed THREE EMPTY SLACK ROWS. A crushed
fixed-height sibling coexisting with grow slack in the same column
means the measure pass over-reports (suspect: Tabs' intrinsic measure
of its content), so shrink fires on siblings the solved layout did
not need to crush. Reproduction: mount
`column[ row.h(2)[Logo.tagline(true), Badge], Tabs{ one tab: padded
column with a 10-item List in a Block.min_h(6).grow }, text ]` at
110x32 — tabs land on row 3 instead of 4. Example-side I mitigated
with `shrink(0.0)` on the fixed rows (which works — the pressure moves
to the grow region), but the phantom deficit is engine-side.

## 1b. Block sizing potholes (probed A/B, each independently confirmed)

Chasing the components stat cards surfaced four composable behaviors —
the first two feel like defects, the last two like footguns worth a
doc line ON THE WIDGET (api.md prose alone doesn't reach docs.rs):

- **`Block::shadow` consumes a row (and a column) of the block's own
  layout slot.** A shadowed block with `.h(5)` paints 4 glyph rows + 1
  shadow row. Nothing documents that the shadow is inside the box; a
  caller sizing "title + N children + border" is short by one and the
  block CRUSHES A CHILD instead of dropping the shadow. Repro: probe
  card with `.h(4)`, title, 2 one-row children — first child vanishes.
- **A Block does not cross-stretch to its row slot** — in a row with
  `.h(5)`, a block whose measured height is 4 hugs 4 and leaves a dead
  row, while `Style` docs say cross-axis default is stretch.
- **Under interior crush, the FIRST child dies first** (order-
  dependent: swapping children swaps the victim) — combined with the
  shadow row this reads as "my label randomly disappeared".
- **`dyn_view` regions do not stretch their inner content**: an inner
  draw element without explicit `w(...)` solves to zero cells and
  paints nothing even when the dyn wrapper itself is `line(1)`
  (draw elements measuring zero is documented; the non-stretching dyn
  interior is the surprising half).
- **Width hug inside Tabs panels**: the widgets example's visual panel
  (now that it renders at all — see §2) lays its whole content column
  at ~44 cells inside a ~108-cell tab: the column hugs its widest
  measured child (the border-blocks row) instead of cross-stretching,
  and the four `grow(1.0)` blocks inside that row stay at measured
  width. Readable but visibly narrow —
  `untracked/review-shots/widgets--visual.svg`.

The fixed `examples/components.rs` `stat_card` documents the working
recipe in comments (shadow-inclusive explicit height; self-sized draw
closures); `untracked/review-shots/components--initial.txt` before/
after pairs show it.

## 2. `Scroll` over a plain element tree collapses to zero width

`Scroll::new(column-of-field-rows).axes(false, true)` (content = an
Element column whose children have explicit `w`/`h` but the column
itself has no measure) rendered as a 1-cell-wide scrollbar at the
LEFT edge of the block with no content. The docs promise "content
extent is measured by default (no size hint)" — that holds for Feed
(it has a measure fn), but a plain Element tree apparently measures
0x0 and the cross-axis does not stretch to the viewport.

SHARPER second sighting: the widgets example's visual panel
(`Scroll::new(content).content_size(96, 18)`) — WITH the explicit
hint — also rendered only its bar once the review harness actually
opened that tab (the panel had apparently never been looked at:
`untracked/review-shots/widgets--visual.txt`, sweep 7 shows the bar-
only frame; this may interact with being inside a Tabs panel).
Expected: the non-scroll axis fills the viewport; the scroll axis
measures the content tree from explicit child sizes (or honors the
hint). Repro in `untracked/review-shots/components--initial.txt`
(run 2, the settings card) + the widgets visual tab. I removed both
Scroll wrappers example-side; the widget behavior is yours to rule
(fix, or document "Scroll content needs a measure").

## 3. mermaid fallback: mermaid.live link is dead text when clipped

`mermaid--gantt-fallback.svg`: the fallback's "view online:
https://mermaid.live/edit#base64:…" line ellipsis-truncates at the
pane edge — a clipped URL is unusable, and the styled dump shows NO
link id on those cells. The engine has OSC-8 hyperlink support
(`Style` link ids); the fallback renderer (extensions/mermaid
view.rs) should emit the URL as a hyperlink so the display text can
clip while the link stays whole — or wrap the URL across lines, never
mid-URL ellipsis.

## 4. GraphView: initial pan shows empty canvas on force layouts

`network--initial.svg`: the example's first frame is two nodes in the
top-left, two long edge strokes, and a mostly empty right half — the
force layout's mass sits away from the (0,0) initial pan. GraphView
exposes `offset_x/y` bindings but no auto-center; consider centering
the layout bbox on first render (or a `.center()` builder). The
workflow (layered) example fronts fine; this is force-specific.

## 5. `render::Compositor::set_debug_damage` unreachable under `App::run`

faq.md/troubleshooting.md promised the damage visualizer to app
authors, but the Driver owns the Compositor privately and exposes no
toggle (no env knob either — the only engine env var is
ABSTRACTTUI_NO_SPLASH). Docs now say honestly "embedders only, no
app-level toggle yet". If you want the promise back, a
`RunConfig`/env toggle is the missing piece.

## 6. Small ones

- **PageHost default layout** hugs the active page's content;
  every real shell wants grow-into-region (the Viewport3D precedent —
  wave 11 made ITS default grow for the same reason). shell.rs now
  passes `.layout(column().grow(1.0))` explicitly; consider the
  default.
- **`PushToTalk::gesture_label`** returns "press Space to start/stop",
  which no sentence template survives ("… to talk" produced "press
  Space to start/stop to talk" in voice_mock). I re-templated the
  example ("talk: {label}"); consider label shapes that compose, or
  document the intended usage.
- **Block children can paint over the block's own border** (no clip):
  gallery's badge row overflowed and erased the panel's right border
  glyph (see `gallery--initial.styled.txt` cols 74-76, run 1). I made
  the content fit; the class (interior overflow silently eats chrome
  glyphs) may deserve a clip or a debug notice.

## 7. FYI — capture harness gained a review family

`examples/capture/` now supports multi-step paced key scripts per
shot (`Shot.steps`) and `cargo run --example capture -- review`
writes ~59 staged shots for every example (root + extensions) to
`untracked/review-shots/`. Never part of `all`, never published.
Future pixel waves re-run it after your widget fixes to diff the
renderings.
