# Wave 11 — DOCS → CODE handoff

From the docs seat's audit (stranger's walkthrough + api.md end-to-end +
examples audit). Nothing here blocks docs; all items are code-side calls.

## 1. App doctest carries a redundant import (cosmetic, optional)

`src/app/mod.rs` (~line 190, the `App::simple` doctest) imports
`use abstracttui::widgets::Button;` even though `Button` rides the
prelude. README.md and docs/getting-started.md now show the one-import
version (the prose right under the snippet says "one import covers the
common path", and the second import contradicted it — the walkthrough's
first stumble). Aligning the doctest keeps the "snippets are lifted from
the crate's own doctests" claim byte-exact. `src/widgets/button.rs`'s own
module doctest importing Button explicitly is fine as is (natural for the
widget's own docs).

## 2. New example `examples/activate.rs` (DOCS-owned) — live-smoke parity is your call

Wave 11 added `examples/activate.rs`: selection-vs-activation on `List`
and `Table` side by side, incl. the 0.2.16 double-click convention — the
one major recent feature that had zero example coverage (verified: no
example used `Table::on_activate` or `click_count()`; reader.rs only uses
`List::on_activate`). It follows the guard pattern (headless exit 0,
verified), is fmt-clean, and is referenced from api.md's Double-click
section and examples/README.md's learning path. If you want pty coverage
parity with the other demos, a `live_activate` entry in
`tests/live_smoke.rs` is the missing half — tests/ is yours.

## 3. Compile-verification status of doc snippets (FYI, no action)

All 33 non-ignored ```rust blocks in docs/api.md and all 3 in
docs/graphs-and-diagrams.md were extracted and compiled against HEAD
(free `cx: Scope`/`&App` provided; obviously-user-named free fns like
`overview`/`inspector_page` accepted as fragments). Two genuine defects
found were BOTH doc-side and are fixed in api.md (the canvas example
called `dyn_view(cx, |_| ...)` — real signature `dyn_view(style, || ...)`;
the Drawer snippet was uncompilable as printed). No code-side drift found:
the 0.2.17 public surface matches the prose everywhere it is named.

## 4. Heads-up on files docs touched that live near your territory

- Every example source (root + extension crates) gained a `//! Docs:`
  header line; `examples/activate.rs` is new. `cargo build --examples
  --workspace`, headless runs of all 23 runnable examples (+ 3 `--caps`
  variants + `capture -- themes`) exit 0, and `cargo fmt --check` is
  clean on all of them.
- docs/api.md was REORDERED (pure section moves, content-preserving):
  PageHost now sits with the widget library, canvas with the render
  family, "Stability and limits" closes before the extensions coda.
  If any of your in-flight doc references cite api.md by line number,
  re-anchor on headings.
