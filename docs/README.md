# AbstractTUI documentation

A reactive, compositor-grade terminal UI engine: fine-grained signals,
layered rendering with damage tracking, images (kitty/iTerm2/sixel/
mosaic), software-rasterized 3D (GLB), themes, and animation.

## Guides

| page | what it covers |
| --- | --- |
| [getting-started.md](getting-started.md) | Install, the first app, core concepts — the 60-second path to a running program. |
| [architecture.md](architecture.md) | How the engine fits together: signals, layout, the damage-tracked compositor, the terminal layer. |
| [api.md](api.md) | The public API surface, module by module. |
| [theming.md](theming.md) | The 36-token semantic model, the 26 built-in themes, runtime switching, contrast guarantees, custom theme registration, and styling rules for widget authors. |
| [graphics-and-3d.md](graphics-and-3d.md) | Images end-to-end (decode → bitmap → widget/protocols, the capability ladder, mosaic modes), the 3D pipeline (GLB loading, scenes, the Viewport3D widget, animation), the boot splash, honest limits, and measured performance. |
| [faq.md](faq.md) | Real questions: design rationale, SSH, terminal image support, headless testing, embedding, dependencies, Windows, clipboard policy, and more. |
| [troubleshooting.md](troubleshooting.md) | Symptom → cause → fix: blank screens, dead keyboards, missing images, wrong colors, flicker, splash gates, slow frames, width misalignment, hanging tests. |

## Reference material

- [`../examples/README.md`](../examples/README.md) — the examples
  catalog: twelve runnable programs from a 53-line hello to a full ops
  dashboard, each documented with keys, requirements, and what it should
  look like. Every example exits cleanly without a tty, and
  `dashboard`/`viewer3d`/`images` take `--caps` to print the terminal
  capability report.
- [`captures/`](captures/) — deterministic text "screenshots" of the
  shipped examples (plain and style-annotated renders), regenerable with
  `cargo run --example capture`.
- [`captures/themes-table.md`](captures/themes-table.md) — the generated
  reference table: every token hex value of all 26 built-in themes.
