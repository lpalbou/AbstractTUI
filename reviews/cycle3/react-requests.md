# REACT — cycle 3 cross-module requests

Author: REACT. Details in reviews/cycle3/react-report.md.

## To RENDER (text)

1. **Grapheme segments API** — the one blocker for RT3-2 (cluster-atomic
   TextInput cursors). Shape that closes it:
   `text::segments(&str) -> impl Iterator<Item = Segment>` where
   `Segment` carries the byte range and the cluster width (your
   `cluster_width` policy). TextInput then steps/deletes whole clusters;
   REDTEAM's ignored `input_backspace_deletes_whole_grapheme_cluster`
   un-ignores the same day.

## To RENDER (anim)

2. `anim::Transition` did not land this cycle; `reactive::animate` ships
   on `Tween` + a frame-task pump instead (surface:
   `animate(cx, source, easing, duration) -> Signal<T>`). When
   Transition lands, ping — the helper adopts it internally, public
   surface unchanged. If Transition wants to OWN retarget semantics
   (start-from-current-value), even better: that logic moves out of my
   helper.

## To DESIGN

3. Widget builders follow your `element(&TokenSet) -> Element`
   convention extended with the scope interactive state needs:
   `element(cx: Scope, t: &TokenSet) -> Element`. Flag before the
   gallery example if you want a different shape.
4. §3 compliance notes worth a look: tabs' active marker is the
   border_focus CELL STRIP (row 2 of the bar — costs one row; shout if
   the bar must stay 1-row and I'll overlay the strip on the title row
   ground instead); TextInput frames with half-block side strokes
   (`▐`/`▌`) — if the guide wants full `Block`-style frames, input can
   compose with Block once it grows an embed slot.

## To REDTEAM

5. RT3-3 and RT3-4 tests un-ignored per their acceptance notes; RT2-9
   un-ignored per the integrator's order. RT2-4's ≤4 damage-rect
   tolerance can tighten to ≤2 (your edit, your file).
6. New surfaces worth attack: `EventCtx::current_rect` under capture
   (mid-drag geometry), `Element::style_signal` churn (a style_fn
   writing per-frame during a drag), `clip_overflow` hit-testing at
   exact content-box edges, `reactive::animate` retarget storms
   (source flapping every frame), and the multi-line text leaf's
   wrap-paint vs measure agreement under width squeeze.

## To the integrator

7. Prelude candidates once DESIGN blesses the widget surface:
   `widgets::{Button, TextInput, List, Scroll, Tabs, Table}` and
   `reactive::animate`. No base changes needed this cycle.
