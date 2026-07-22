# QUALITY — aesthetics audit (study 2)

Date: 2026-07-22 · tree: 0.2.1 post-release working tree · owner: QUALITY

Scope: extend the deterministic capture pipeline to the new surfaces,
audit visual coherence of the three waves' widgets against the existing
design system, fix the cheap incoherences, defer the design questions.

## 1. Capture coverage extended (new visual-regression surface)

`examples/capture.rs` became `examples/capture/` (main.rs +
app_shots.rs — the dashboard/ directory precedent; `cargo run
--example capture` unchanged) and gained a fourth family, `apps`:
in-process shots driven through `Driver` + `CaptureTerm` with fixed
data and scripted input — **no pty, no clocks, byte-deterministic**
(the pty shots keep their honest frame-pacing wobble; these four have
none). New artifacts in `docs/captures/`:

| shot | shows |
| --- | --- |
| `transcript-stream` (90x26) | Feed mid-stream with an OPEN ```rust fence (code tint before the fence closes), follow-tail pinned, TextArea composer with `/th` typed and the completion dropdown OPEN (flipped above the bottom composer, filtered to /theme + /thanks, detail column) |
| `select-open` (72x20) | settings form, Select popup open below the trigger, highlight moved to "beta" while the trigger still reads "stable" (the 0250 movement-vs-commit split in pixels), muted right-aligned hints, disabled "archive" row |
| `code-diff` (84x20) | a unified diff through `CodeView::lang("diff")`: file header/meta muted, hunk headers info, −/+ lines error/ok, gutter + rule |
| `feed-scrolled` (84x22) | follow-tail BROKEN by wheel-up: scrollbar thumb mid-track, tail off-screen, status line "scrolled (f to re-follow)" |

The captures README (auto-generated manifest) now states which
artifacts are byte-deterministic. Each shot also writes the
`.styled.txt` dump, so token drift (not just glyph drift) is diffable.

## 2. Gallery — the design-system board now shows the waves

`examples/gallery.rs` (112x38, was 112x32): the widgets column gained
the **Select trigger** (chosen value + hint options) and a **TextArea
seeded with two lines** (a one-row TextArea reads as a TextInput — the
seed makes the multiline nature visible on the still); the content
column gained a **4-line diff block** under the code sample. Layout
stays composed (three columns, the <104-col bow-out untouched); the
gallery capture regenerated at the new size.

## 3. Coherence audit findings

Verified clean (no action):

- **Token honesty (RT1-9b)** — zero raw `Rgba::rgb/rgba/new` literals in
  `src/app/select*`, `anchored*`, `selection.rs`, `widgets/textarea*`,
  `feed*`. `widgets/code.rs`'s two `-> Rgba` mapping fns
  (`code_token_color`, `diff_token_color`) are the ONE sanctioned
  kind→ink point, and the diff mapping rides audited semantic tokens
  (`ok`/`error`/`info`/`text_muted`) with a per-theme ≥3.0:1 contrast
  test on the code ground. The widgets/mod.rs lint list already covers
  the new widget files.
- **Frame vocabulary parity** — TextInput, TextArea, and the Select
  trigger all draw the same framed-widget chrome: `▐`/`▌` side strokes,
  `border` → `border_focus` on focus, ground `surface`, text at x+1,
  placeholder `text_faint`. Verified in source and in the styled
  captures. Button deliberately belongs to the other declared family
  (borderless column: hover=accent ink, focus=selection pair) — that is
  the style guide's §3.2/§3.3 split, not drift.
- **Popup chrome consistency** — Completion panel and all three Select
  faces fill flat `surface_raised` with `selection_fg/bg` highlights and
  muted hints; rows print at x+1 in both. Consistent with Toast
  (`surface_raised` chip) and Modal (`overlay` ground). No overlay draws
  a shadow: elevation-by-ground-tier is the system-wide rule, and
  `Block::shadow` remains an app opt-in. Coherent.
- **Highlight treatment** — option rows, completion rows, and List all
  use the selection pair for the highlight; disabled rows faint in both
  select_core and the trigger.

Fixed (cheap wins):

- **Gallery gaps** (§2): the design-system surface was missing all three
  wave-2/3 widget families — fixed, captures regenerated.
- **Capture blind spot**: none of the new surfaces was in the visual
  regression surface at all — fixed with the deterministic `apps`
  family (§1).

Deferred (design questions, priced, not mine to decide unilaterally):

1. **Popup boundary on same-ground content** — a Select popup
   (`surface_raised`) floating over content that itself sits on
   `surface_raised` (a picker inside a raised card) has no visible
   boundary in low-contrast themes; Modal solved this with the distinct
   `overlay` token. Options: popups adopt `overlay` ground, or a 1-cell
   `border` rule row, or leave (the common grounds are `bg`/`surface`,
   where the raised popup reads clearly). Needs a DESIGN ruling; zero
   code risk either way.
2. **Completion dropdown overlapping block borders** — the flipped
   dropdown may cover the transcript block's bottom border row
   (visible in `transcript-stream.txt`, row 24). Honest float behavior,
   and inherent to caret anchoring in a tight layout; a `place_panel`
   variant that avoids the anchor's enclosing border row would be
   cosmetic polish with real placement-logic cost. Recorded, not
   recommended.
3. **`Feed` not in the prelude** — `Feed`/`FeedItem`/`FeedState` need a
   second import while `TextArea`/`Select` are one-import; api.md
   documents the two-line import honestly. Additive re-export, prelude
   curation is REACT's call.

## 4. Verification

- `cargo run --example capture` regenerates all 37 artifacts; the four
  app shots are byte-identical across regenerations (clockless).
- Whole-tree `cargo test` green after the gallery/capture changes
  (gallery has no golden pins; docs/captures are documentation stills,
  excluded from the crates.io package by Cargo.toml).
- `cargo fmt --all --check` and `cargo clippy --all-targets` clean.

## 5. Files touched

- `examples/capture/main.rs` (moved from capture.rs; + `apps` family,
  manifest wording, gallery shot 112x38)
- `examples/capture/app_shots.rs` — NEW (4 deterministic app stills)
- `examples/gallery.rs` — Select + TextArea + diff block, 38 rows
- `examples/README.md` — gallery/components/capture sections updated
- `docs/captures/*` — regenerated + 8 new artifacts
