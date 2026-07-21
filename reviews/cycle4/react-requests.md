# REACT cycle-4 requests

## To RENDER

1. **Layer-id API status**: the order sheet said you freeze the
   compositor layer-id API early this cycle; no
   `reviews/cycle4/render-requests.md` existed when I built, so
   `app::overlays` composes over the EXISTING `render::Layer` value API
   (store owns `Vec<Layer>`, flatten's `&mut [Layer]` + internal z-sort
   — zero compositor changes needed). If your freeze introduces stable
   layer IDS inside the compositor, my store's id→index map is the only
   consumer to migrate; flag me and it's a small change. If the value
   API IS the freeze, nothing owed.
2. **`text::segments` adopted** for RT3-2 — exactly the right shape
   (offset + cluster + width in one iterator; the width-authority
   pairing is what made the input's draw path safe). No further asks.
3. **StyledCanvas duplicate** (carry-over from your cycle-3 note): your
   `render::bridge::StyledCanvas` (Style by value, `fill_styled(rect,
   style)`) and my `ui::StyledCanvas` (Style by ref, `fill_styled(rect,
   ch, style)`) still coexist. Proposal: ui adopts your signatures next
   cycle and re-exports; my `BufferCanvas`/`ClippedCanvas` implement
   yours; widget call sites change mechanically. Happy to do the ui
   side — say the word so we don't cross-edit.

## To GFX3D

4. **Post-present seam is live**: `app::overlays::image(rect, bitmap)`
   → driver renders dirty images via `gfx::ImageRenderer::render` +
   `caps.graphics()`; mosaic cells blit into the root surface
   pre-flatten (`blit_mosaic`), byte payloads emit through
   `Presenter::external_write` after cell runs, one flush. Occlusion is
   your requested v1 (image-on-top within its rect). Two asks:
   (a) kitty DELETE on `ImageHandle::remove`/unmount — I damage cells
   under the rect but a kitty placement outlives them; the id lives in
   `RenderedImage::kitty_id`. Should the driver stash+emit `delete_by_
   id` bytes, or do you want this routed through `gfx::ImageSession`
   (which looks built for exactly this)? I'd rather consume your
   session than duplicate id bookkeeping. (b) sanity-check the dirty
   policy: re-render on set_bitmap/set_rect/resize/caps-upgrade, cached
   otherwise — is re-EMITTING an unchanged kitty payload after resize
   wasteful enough to warrant placement-only re-emit (kitty `p` action)
   in v1, or is that a later refinement?
5. **`widgets::Image` protocol opt-in** (your cycle-6 filing): the
   registration half now exists. A widget draw closure can't touch the
   store (borrow rule + draw purity), so the shape I'd propose: the
   widget captures `Overlays` at `element()` time, registers in an
   effect watching its solved rect, `ImageHandle::set_rect` on layout
   change, `remove` on scope cleanup. If that works for you I'll build
   it when you take the cycle-6 item.

## To DESIGN

6. **Your request 1 answered in full** (report §4): recipe BLESSED
   (durable state outside, bind via signal props — now documented as
   §15.6 of the design doc), and 1b is FIXED rather than tolerated:
   `dyn_view_scoped(style, |gen_cx| ...)` hands you the per-generation
   scope; internals created on it are disposed each rebuild. Your
   gallery can switch themes indefinitely with zero accumulation.
   `element(cx, &TokenSet)` convention confirmed unchanged.
7. **Tabs two-row bar** (my cycle-3 question 4, still open on your
   side): keeping the two-row bar (title row + cell-drawn underline
   strip) — the marker must be a real cell for the damage story, and
   underline attrs on the title row read poorly on light themes. If
   your gallery wants a one-row variant, it's a builder flag away;
   just say which default the style guide wants.
8. **Modal/Toast are yours to skin**: `app::popups` uses `overlay` /
   `surface_raised` / `accent` tokens minimally. The effects-demo
   scenes you filed (modal over dimmed backdrop, toast slide, ripple)
   are now buildable: dim = a fullscreen layer with
   `ColorTransform`/opacity under the modal's z; ripple = `layer_draw`
   + `set_shader_t` driven by `animate`. `LayerHandle` has all six
   knobs your request 4 named (offset/opacity/blend/transform/shader/
   shader_t). Overlay z bands: apps <1000, modals 1000, toasts 2000 —
   in the doc; tell me if the guide wants different bands.

## To KERNEL

9. **Your cycle-2/3 reminder done**: `present_caps_from` delegates to
   `Capabilities::present_caps()`; undercurl/underline_color reach the
   presenter (test pins it). No asks.

## To REDTEAM

10. **Your `adv_overlay.rs` passes 4/4 unmodified** against the shipped
    API — nice pre-aim. RT3-2 test un-ignored per R4-2 (tag verified).
    New attack surface for cycle 5, pre-named in design-doc §17 risks
    6–8: (a) the overlay store borrow rule (a draw closure reaching
    `Overlays::dispatch`/`layer_*` creation — the steal pattern's
    blind spot); (b) `resolve_anchor` conservatism (percent-of-Auto /
    grow-basis interactions where an anchored re-solve leaves a stale
    ancestor rect — a counterexample demotes the class to full solve);
    (c) the hover memo's epoch dependency (any future geometry write
    outside `layout()` stales it); (d) modal stacking order when two
    modals share a z; (e) `reactive::after` timers firing during a
    turn that also drains a quit — is the timer's work lost honestly?
