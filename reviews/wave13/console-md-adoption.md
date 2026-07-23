# Gateway console: model output needs the markdown machinery (wave 13, engine evidence)

For the gateway seat, from the abstracttui markdown lane — the
operator's "the markdown viewer is absolutely terrible and it doesn't
even scroll" screenshot traced to BOTH sides; here is the console's
half, with the engine's half already shipped.

**The adoption gap, precisely located**: the console renders model
output as bare truncated text. `console-tui/src/ui/review.rs::sandbox_result`
prints the reply as `ellipsize(o.response.trim(), 90)` into a single
plain `line(...)` — no markdown parse, no tables, no fences, no scroll,
90 characters then an ellipsis. Every markdown reply a model produces
(headings, tables, ```json fences) renders as raw pipe-and-hash soup,
which is exactly the screenshot. The engine ships the machinery the
console skipped: `MarkdownView` typesets the full doc vocabulary (GFM
tables with alignment, task lists, fenced code with diff/json/yaml
tinting as of wave 13), and `Feed` renders the same vocabulary
streaming (closed blocks freeze, the open tail re-typesets — a
streamed table renders live).

**The one-liner, after wave 13**: `MarkdownView` and `CodeView` now
carry an intrinsic measure, so the composed pane scrolls out of the box
— `Scroll::new(MarkdownView::new(reply).view(cx)).view(cx)` is the
complete scrolling markdown result pane (no `content_size` hint, no
app-managed offset; wheel/keys/scrollbar work; tables floor their
columns at crush widths and degrade to a labeled record list instead of
vanishing). For a live-generating sandbox, prefer a one-item `Feed`
with `stream_append`. Recommendation: route `sandbox_result`'s Ready
arm (and any other surface showing model text — entity chat, run
output) through one of those two; keep `ellipsize` for the one-line
summary row only.
