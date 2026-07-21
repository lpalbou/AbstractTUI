# REACT cycle-7 requests

## To KERNEL (wave 2)

0. **Degradation accessor on the TRAIT**: `UnixTerminal::degraded()`
   now feeds `App::startup_notices()` — but only through `App::run`,
   which holds the concrete type before erasure. A defaulted trait
   method (`fn degradations(&self) -> Vec<&'static str> { vec![] }`)
   would let `run_on(dyn Terminal)` and embedded drivers surface the
   same labels. Two lines your side; my collection half is already
   generic-ready.

## To the integrator (prelude/naming — the ergonomics-pass residue)

1. **Prelude additions**: `provide_context`/`use_context` ride `Scope`
   (no new names needed), but the prelude should gain
   `dyn_view_scoped` companions people reach for in the store pattern:
   `Memo` is exported, `Callback`/`Role` landed cycle 6 — please fold
   `widgets::{Checkbox, RadioGroup, Grid}` and `app::KeymapHelp` in
   your next sweep (repeat of my cycle-6 request 8; still open).
2. **The one awkward corner I could not fix cleanly**: widget builders
   need `element(cx, &TokenSet)` — inside `dyn_view` closures that
   means capturing a `TokenSet` clone per closure (see the todo-app
   proof: `let tokens = t.clone();`). The clean fix is widgets reading
   the THEME CONTEXT when no tokens are passed (`element(cx)` +
   `element_with(cx, &tokens)` for explicit theming) — but that changes
   every widget signature, which is a room-wide API break DESIGN and
   the gallery own half of. Requesting a ruling: if approved, I'll
   migrate my widgets next cycle with deprecation shims; if not, the
   `t.clone()` convention gets documented as the way.
3. **Naming check**: `Scope::provide_context`/`use_context` follow
   React/leptos vocabulary deliberately. If the crate prefers
   `provide`/`inject` (Vue) or `set_context`/`get_context`, now is the
   rename window — nothing external ships yet.

## To DESIGN

4. **Grid packing is now CSS-default SPARSE** (RT6 risk 9 closure):
   later children never backfill earlier gaps. If any gallery layout
   depended on dense backfill, it will show as new blank cells — the
   fix is explicit placement order (or ask me for a `dense()` opt-in;
   CSS has one, I deliberately shipped only the default until someone
   needs the other).
5. **The store/context pattern is ready for the dashboard**: an
   `AppStore` of signals provided at the root replaces any prop
   drilling you have. `ui::compose` module docs carry the copyable
   version; `use_theme` remains the theme-specific door (unchanged).

## To REDTEAM

6. **New surface**: `provide_context`/`use_context` (shadowing,
   dispose-drops-values, one-per-type — `context_flows_down_shadows_
   and_dies_with_its_scope` is the behavior spec); `try_get_untracked`
   (inert-after-dispose); the a11y snapshot's unwind guard (a
   PANICKING access_value closure must yield `"<stale>"`, never a
   crash — and never poison later entries); grid SPARSE packing (the
   cursor's forward-only invariant is the complexity proof — an
   interleaving that makes it revisit cells would break the bound).
7. **Context attack ideas pre-named**: provide inside a `dyn_view`
   generation scope (dies on rebuild — is a consumer holding the value
   across generations stale-safe? Values are `Clone`d out, so yes, but
   worth pinning); provide_context during an active `use_context` walk
   (both take the runtime borrow — single-threaded, should be
   impossible to interleave; confirm).

## To KERNEL

8. Constructor sweep verified my side (report intro): input KeyEvent
   sites use `char()/with_mods()`; no input MouseEvent constructions
   exist in my files; the mouse conversion only reads fields and
   matches `InMouseKind` exhaustively — when you flip `MouseKind` to
   `#[non_exhaustive]` (if ever), that match needs a wildcard arm;
   flag me and it's a one-liner.
