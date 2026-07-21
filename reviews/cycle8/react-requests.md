# REACT cycle-8 requests

## To the integrator — THE FINAL PRELUDE PROPOSAL (item 5)

What a real app imports, grouped; everything below exists today. The
test: the 16-line first app and the compose-doc store app compile from
`use abstracttui::prelude::*;` plus ONE `widgets::` line.

```rust
// geometry + color
pub use crate::base::{Point, Rect, Rgba, Size};
// reactivity (the six things every app touches)
pub use crate::reactive::{batch, untrack, Callback? -> no, ui owns it, Memo, Scope, Signal};
// NOTE: create_root/on_cleanup/Effect stay importable but OUT of the
// prelude — App::mount owns root creation in app code; tests import
// them explicitly.
// layout
pub use crate::layout::{Align, Dimension, Display, Edges, Justify, LayoutStyle, Overflow, Track};
// ui
pub use crate::ui::{
    dyn_view, dyn_view_scoped, text, Callback, Element, Key, KeyChord, Role, View,
};
// NOTE: styled_text/Canvas/UiTree/Mods drop to explicit imports —
// UiTree is test/embedding surface, not app-code surface.
// paint (rename decision pending RENDER; until then the full path
// `render::Style` beats an ambiguous bare `Style` next to LayoutStyle)
pub use crate::render::Surface;
// theme
pub use crate::theme::{Theme, TokenId, TokenSet};
// app
pub use crate::app::{
    current_theme, set_theme, set_theme_by_id, use_startup_notices, use_theme,
    use_viewport, App, Modal, Quitter, RunConfig, Toast,
};
// widgets: the interactive core
pub use crate::widgets::{
    Button, Checkbox, Grid, List, RadioGroup, Scroll, Table, Tabs, TextInput,
};
// anim
pub use crate::anim::{Easing, Transition};
```

Deltas from today's prelude, with reasons:

1. REMOVE `render::Style` from the prelude (keep `Surface`): two
   `Style`s one glob apart is the top newcomer trap; `LayoutStyle` is
   the one apps write hourly, paint styles appear only inside draw
   closures where the full `render::Style` path reads clearer. (If
   RENDER ever renames — `Paint` was floated — re-add under the new
   name.)
2. REMOVE `create_root`, `on_cleanup`, `Effect`, `FrameRequester`,
   `PixelSize`, `Canvas`, `UiTree`, `styled_text` — engine/test
   surface, not app-code surface.
3. ADD the interactive widget set + `Modal`/`Toast` +
   `use_viewport`/`use_startup_notices` + `Key`/`KeyChord` (every app
   with shortcuts needs the chord types) + layout enums
   (`Dimension`/`Align`/... appear in any nontrivial style).
4. KEEP the display widgets OUT (`Badge`/`Block`/`Spinner`/... —
   DESIGN may disagree; their call, one line each).

## To RENDER

5. `render::Style` naming: my prelude proposal drops it rather than
   renames it (your file). If you want a user-facing alias (`Paint`?),
   cycle 9 is the window — I'll adopt in widget docs the same day.

## To DESIGN

6. `use_startup_notices(cx)` shipped exactly as asked — signal read,
   late engine pushes propagate (test pins your failure case). The
   notice-bar recipe is one dyn_view; example in the module docs.
7. `Widget::view(cx)` is the new canonical build (theme from context).
   Your gallery can drop every `&theme.tokens` argument at leisure —
   `element(cx, &tokens)` still works unchanged, so no forced
   migration. Fair warning: gallery code that passes a NON-active
   TokenSet deliberately (theme previews!) must KEEP `element_with`-
   style explicit calls — `view(cx)` always resolves the ACTIVE theme.

## To REDTEAM

8. Your suites pass unchanged — the one candidate breakage (generic
   `List::new`) was caught by YOUR `.collect()` call sites failing to
   infer, and reverted in favor of an additive `of()`. That's the
   compatibility net working as designed; keeping those call sites in
   your files is genuinely useful.
9. New surface: `App::simple` (loop behavior identical to run —
   nothing new to attack there), `Widget::view` theme-context read
   (probe: theme switch inside a dyn that ALSO builds widgets — double
   rebuild?), `use_startup_notices` (thread-global signal: two Apps on
   one thread SHARE the store — documented as per-thread, worth a
   pin), `LayoutStyle::fill/line` (trivial), panic messages (the FIX
   lines claim things — verify each suggestion actually compiles away
   the panic it names).
