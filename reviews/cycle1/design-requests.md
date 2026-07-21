# DESIGN cycle-1 requests (cross-module needs)

Contracts the theme/boot/examples surfaces need from other owners. None
block cycle 1 (theme + identity ship standalone); all block the cycle-2/3
consumers named below.

## To RENDER (Style/Cell, anim)

1. **Style stays resolved-color POD.** `Style { fg: Rgba, bg: Rgba, attrs }`
   with tokens resolved at view-build time (ui layer), NOT `Style`
   holding `TokenId`s. Rationale: the presenter/diff hot path must never
   chase a theme lookup; reactivity already re-runs the view code on theme
   change, so resolution at build is free. If RENDER prefers token-aware
   styles instead, flag it — the tokens API (`TokenSet::get`) supports
   either, but the choice must be made once, before widgets exist.
2. **Attr bits needed by the token model:** BOLD, DIM, ITALIC, UNDERLINE,
   STRIKE, REVERSE + **underline color** (SGR 58/59) with a labeled
   downlevel (drop to plain underline when unsupported). `link` styling is
   `fg=link + underline`; focus styling uses underline-color in text-only
   widgets where a border is unavailable.
3. **Downlevel color mapping must preserve contrast ordering.** When
   truecolor degrades to 256/16, map fg and bg *jointly* (nearest-pair with
   a minimum-distance constraint), not per-color-nearest — per-color
   collapses `text_faint` vs `bg` on several dark themes (both quantize to
   the same gray cube slot). The audit floors in `theme::contrast` define
   the pairs that must survive; happy to co-own a test.
4. **anim: named easing constructors from cubic-bezier params** —
   `Ease::bezier(x1, y1, x2, y2)` or equivalent, so
   `boot::identity::{EASE_ARRIVAL, EASE_SETTLE, EASE_TRACKING, EASE_FADE}`
   plug in directly. The settle curve overshoots (y1 > 1); the tween
   evaluator must not clamp intermediate values to 0..=1.

## To REACT (reactive, ui, app)

5. **Active theme as a signal.** One app-level
   `Signal<&'static theme::Theme>` (or equivalent handle) + context access
   from any component. Registry side is ready: `theme::get/resolve/themes`
   return `'static` data precisely so the signal payload is `Copy`-cheap
   and never lifetime-bound. The themes_gallery example (cycle 3) is the
   acceptance test: Enter on a card swaps the signal and the whole screen
   restyles through normal reactivity, zero special-case invalidation.
6. **Widget styling contract:** widgets take tokens (ids or resolved
   colors) via props with theme defaults — no hex literals in
   `src/widgets/**`. DESIGN will lint by grep in cycle reviews (the
   abstractassistant 443-literal lesson: literals metastasize immediately).

## To GFX3D (three, boot rendering)

7. **Boot splash implementation against `boot::identity` constants only** —
   timings, camera, easing, `brand_ramp(t)`. If a constant is missing or
   the wrong shape for the renderer, request a change in reviews rather
   than hardcoding a sibling value; the 2D fallback reads the same
   constants and the two must never drift.
8. **Needed layer features:** additive blend mode for afterglow/particles
   (or a documented emulation: pre-lit colors over black), per-layer
   opacity animation for the trail decay (×0.72 per 100 ms), and depth fog
   toward a caller-supplied `Rgba` (the active theme's `bg`).
9. **Image/3D chrome:** viewers take the surrounding chrome colors from
   tokens (`bg` fog, `border_focus` on the focused viewport). Mosaic
   dithering should assume the theme ground as the matte for
   alpha-over-terminal edges.

## To KERNEL (term, input)

10. **Splash skip needs input before the app loop:** events must be
    readable during the boot sequence, and the skip key must be consumed
    (not leak into the app's first frame). Also needed: TTY detection
    (`is_tty`) exposed so boot can auto-skip, per the vision requirement.
11. **Capability report:** truecolor vs 256 vs 16 and graphics protocol,
    early enough for boot to choose 3D / 2D-truecolor / skip. `NO_COLOR`
    and `TERM=dumb` honored in the report (theme side already honors
    NO_COLOR in the placeholder example).

## To integrator (base) — informational, no change requested

12. `Rgba` as shipped (from_hex/lerp/over/luminance) was sufficient for the
    whole token pipeline; no base additions needed this cycle. If HSL-space
    mixing ever becomes necessary for nicer derived hover states, it will
    arrive as a `theme::derive` internal, not a base request.
