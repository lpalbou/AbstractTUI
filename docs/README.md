# AbstractTUI documentation

A reactive, compositor-grade terminal UI engine: fine-grained signals,
layered rendering with damage tracking, images (kitty/iTerm2/sixel/
mosaic), software-rasterized 3D (GLB), themes, and animation.

## Guides

| page | what it covers |
| --- | --- |
| [getting-started.md](getting-started.md) | Install, the first app, core concepts — the 60-second path to a running program. |
| [architecture.md](architecture.md) | How the engine fits together: signals, layout, the damage-tracked compositor, the terminal layer. |
| [api.md](api.md) | The public API surface, module by module — including the focus rule (who receives keys, and how a field asks for the caret at startup), Codex-compatible text editing chords, selection-safe follow-tail scrolling, List row-context requests with the reusable `ContextMenu`, the `DrawerDock` drawer rail with its portable stacked-label boundary, and `gfx::bigtext` for text and icons drawn several cells tall. |
| [theming.md](theming.md) | The 36-token semantic model, the 26 built-in themes, runtime switching, theme modes and the drop-in `ThemeSwitcher`, contrast guarantees, custom theme registration, deriving a full token set from your own house colours (`theme::Palette`), readable ink and ground separation for grounds your app mints, and styling rules for widget authors. |
| [graphics-and-3d.md](graphics-and-3d.md) | Images end-to-end (decode → bitmap → widget/protocols, the capability ladder, mosaic modes), the 3D pipeline (GLB loading, scenes, the Viewport3D widget, animation), the boot splash, honest limits, and measured performance. |
| [graphs-and-diagrams.md](graphs-and-diagrams.md) | The extension family (`abstracttui-graph`, `abstracttui-mermaid`): layout pass selection (layered vs force vs grid), the `GraphDesc -> Layout` contract, `GraphView` usage, the mermaid subset table (flowcharts, sequence control flow, flattened subgraphs), diagrams inside markdown documents, and the atomic fallback, install lines and worked examples. |
| [live-data.md](live-data.md) | Background threads into the UI: the ownership rule, source→signal bindings, bounded ingestion with honest drop counters, the `interval` time source, the connection lifecycle (reconnect with jittered backoff), worker lifecycle. |
| [faq.md](faq.md) | Real questions: design rationale, SSH, terminal image support, headless testing, embedding, dependencies, Windows, clipboard policy, and more. |
| [troubleshooting.md](troubleshooting.md) | Symptom → cause → fix: blank screens, dead keyboards, typing that lands nowhere until you Tab into the field, missing images, wrong colors, flicker, splash gates, slow frames, width misalignment, file drops pasting as text, images that reject by name, hanging tests. |

## Reference material

- [`../examples/README.md`](../examples/README.md) — the examples
  catalog, ordered as a learning path: 35 runnable programs from
  a 53-line hello to a full ops dashboard and an app shell (a `PageHost`
  tab bar hosting full pages, with `Drawer` panels from both edges),
  the diagram family (`workflow`/`network`/`mermaid`/`mermaid_doc`), and
  a document whose ```mermaid fences draw themselves — each documented
  with keys, requirements, and what it should look like. Every example exits cleanly without a tty, and
  `dashboard`/`viewer3d`/`images` take `--caps` to print the terminal
  capability report.
- [`captures/`](captures/) — deterministic "screenshots" of the
  shipped examples (plain text, style-annotated, and rendered SVG via
  `Screenshot::to_svg`), plus in-process
  stills of the app-layer surfaces (streaming transcript with the
  completion dropdown open, an open Select popup, a diff-tinted code
  pane, a scrolled feed, a doc-vocabulary reader table); regenerable
  with `cargo run --example capture`.
- [`captures/themes-table.md`](captures/themes-table.md) — the generated
  reference table: every token hex value of all 26 built-in themes.
- [`design/`](design/) — engineering design notes (the damage contract,
  the reactive model, rendering, terminal/input, theming, testing), and
  [`adr/`](adr/) — the architecture decision records behind the API
  stability, styling, extensibility, and extension-packaging policies.
