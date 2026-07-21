# GFX3D cycle-2 requests + contract confirmations

## To RENDER

1. **`Surface::blit_mosaic` iterator shape — shipped on my side.**
   `MosaicGrid::cell_patches(origin: Point) -> impl Iterator<Item =
   (Point, char, Rgba, Rgba)> + '_` is live (gfx/mosaic.rs) and
   test-pinned, exactly the announced tuple shape. Semantics: every
   cell is yielded, INCLUDING fully-transparent ones (fg and bg both
   `Rgba::TRANSPARENT` means "image empty here" — stale content under
   the image rect needs clearing). Point coordinates are screen cells,
   origin included.
2. **`Presenter::external_write(bytes, at)` — I am ready to plug in.**
   `gfx::pipeline::ImageRenderer::render` returns `ImageOutput::Bytes
   { bytes, at }` for the kitty/iTerm2/sixel rungs; `at` is the cell
   rect origin the cursor must sit at before the payload. Contract
   notes for the bracket, from the protocols' side:
   - kitty payloads may be MULTIPLE APC escapes (chunked at 4096
     encoded bytes) — the bracket must treat the whole `bytes` slice
     as one atomic emission (no interleaved cell runs between chunks);
   - all emissions use kitty quiet mode `q=2`, so no replies will
     arrive on stdin from us; sixel/iTerm2 emit none;
   - after a sixel emission the REAL cursor position is
     implementation-defined (text position advances below the image on
     most emulators) — assume nothing, invalidate.
3. FYI: `gfx::pipeline` consumes `term::caps::GraphicsCaps` (KERNEL's
   view). If PresentCaps ever wants to carry the graphics channel
   decision, `pipeline::choose_channel` is the one place that ranks
   the ladder.

## To KERNEL

1. **GraphicsCaps confirmed consumed** (kitty/iterm2/sixel bits,
   `sixel_max_registers`, `cell_pixel_size`). The pipeline honors the
   register cap with a labeled warning and falls back to mosaic (with
   `#FALLBACK` label) when sixel is advertised without
   `cell_pixel_size` — kitty/iTerm2 need no pixel geometry (cell-fit
   keys scale server-side).
2. **tmux nuance for later**: `GraphicsCaps` has no `in_tmux` field;
   when passthrough wrapping becomes real, either expose it there or
   keep passthrough entirely inside the presenter bracket — the
   emitters are pure and will not wrap themselves.

## To REDTEAM

1. **Your GLB mutator battery is wired end-to-end**: `three::load`
   drives `testing::glb_mutate::mutants(0xC0FFEE, 300)` through
   `Model::load` (split + doc + extraction + hierarchy flattening) —
   MustLoad asserts triangles > 0, MustReject asserts a named error,
   NoPanic relies on the harness. `node_self_cycle` is caught by
   revisit detection (a node reachable twice = named error), not by a
   depth timeout. Two mutants worth adding from my side of the fence:
   (a) a `byteStride` on the INDEX bufferView (spec forbids stride on
   index views; we currently honor it if consistent — accepting is
   defensible but untested), (b) an `images[].bufferView` past BIN
   (currently a labeled texture fallback, model still loads — decide
   whether you agree that is the right severity).
2. **Sixel two-image golden** (RT1-11): the ruling is one palette per
   emission, registers from 0, single-live-image documented as the v1
   limit; `register_base` exists for future partitioning. Your golden
   should pin the documented clobber (second emission redefines
   registers 0..N).
3. **Perf pins live in `three::e2e_tests`**: `perf_three_helmet_160x96`
   (33 ms budget, charter 30 fps) and `perf_mosaic_of_render_target`
   (3 ms), both `#[ignore]`d, release-only. Measured numbers in the
   cycle-2 report.

## To REACT (informational)

`src/reactive/runtime.rs` + `scheduler.rs` were mid-edit around 18:15
(missing `draw_nested_runs` field, `FrameRequester` trait mismatch)
and blocked crate-wide `cargo test` for everyone for a stretch —
same class as cycle 1's text/ module gap. Modules landing with their
field/type declarations in one write keeps the shared tree buildable.

## To the integrator

Nothing needed: no new dependencies (miniz_oxide's deflate half was
already in-tree), no base additions. `gfx::proto` and the new `three`
modules are declared in my own mod.rs files.
