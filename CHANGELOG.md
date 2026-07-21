# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-07-21

First public release.

### Added

- **Terminal kernel** — raw mode, alternate screen, and terminal capability
  detection (colors, graphics protocols, keyboard protocol, clipboard,
  notifications) with Unix and Windows backends.
- **Input** — byte-stream parser producing structured key, mouse, paste, and
  resize events; kitty keyboard protocol support with legacy fallback.
- **Compositor** — z-ordered layers with alpha blending, damage tracking,
  diff/present with minimized escape traffic, per-cell shaders, and scroll
  optimization.
- **Reactive runtime** — fine-grained signals, memos, effects, scopes, and a
  scheduler; updates repaint only what changed.
- **Layout** — flexbox-style solver with grid tracks, wrapping, and gaps.
- **Widgets** — 20+ built-ins (text input, list, table, tabs, buttons,
  checkboxes, radios, progress, spinner, charts, markdown, rich text, code,
  scroll views, images, 3D viewport, and more), styled through theme tokens
  only.
- **Themes** — design-token system with 26 built-in themes (including
  Catppuccin, Rosé Pine, Tokyo Night, Nord, One, Dracula, Monokai, Gruvbox,
  Solarized, Everforest families) and a tested contrast audit.
- **Images** — PNG and JPEG decoding delivered over four channels: kitty
  graphics, iTerm2 inline images, sixel, and unicode mosaic fallback.
- **3D** — GLB (glTF 2.0) loading and software rasterization with animation
  support, rendered into the same composited scene.
- **Boot** — AbstractTUI visual identity splash.
- **Testing** — headless test terminal, VT interpreter, golden snapshot
  harness, and randomized-input utilities, usable by downstream crates.
- **Examples** — 12 runnable examples, from `hello` to a full dashboard,
  theme browser, and 3D viewer.

[0.1.0]: https://github.com/lpalbou/abstracttui/releases/tag/v0.1.0
