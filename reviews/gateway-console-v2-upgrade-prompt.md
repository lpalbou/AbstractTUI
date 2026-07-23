# Prompt: adopt PageHost + Drawer in the AbstractGateway console-tui (AbstractTUI 0.2.12)

Copy everything below the line into the console-tui session.

---

Your project `~/tmp/abstractframework/abstractgateway/console-tui` is
built against `abstracttui = "0.2.9"`. **AbstractTUI 0.2.12 is
released** (crates.io + https://github.com/lpalbou/AbstractTUI, ~1,900+
tests green). Two new higher-level containers landed in the 0.2.11 /
0.2.12 window that your shell hand-rolls today: **`PageHost`** (the
maintainer's "global tab system" — full pages behind one themed tab
bar) and **`Drawer`** (edge-anchored overlay panels hosting full
pages). You built the tab bar, its mouse hit-test, the digit loop and
the chord plumbing yourself; the engine now owns those jobs, and a
detail panel that slides from the edge is a new option you didn't
have. Adopt where the engine now carries the policy — with the honest
caveats below, because your **wizard mode is not a PageHost concept**
and that wrinkle drives the whole migration.

**Dependency first**: set `abstracttui = "0.2.12"` in `Cargo.toml`
(path dep `{ path = "../abstracttui" }` if the index lags). No breaking
API changes 0.2.9 → 0.2.12 (`cargo semver-checks` clean at every hop);
MSRV stays 1.87, so your `rust-version` comment (`Cargo.toml:5`) only
needs the version string updated. Everything below is additive — none
of it changes your store, your worker protocol, or your screen
component functions.

Work through these in order, running your suite after each.

## 0. The one thing to understand first: PageHost is FREE navigation; your wizard is GATED

Your shell has two modes over the same six screens
(`src/ui/mod.rs:29-36`, `SCREENS` — Connection, Providers, Routes,
Users & Entities, Runtimes, Review & Test): **browse** (jump anywhere:
digits 1-6, Ctrl+N/P, click a tab) and **wizard** (linear, gated —
`wizard_next` refuses to leave Connection until connected or the user
chose offline, `mod.rs:577-603`; future steps render greyed,
`mod.rs:680-682`; digit/click jumps are refused WITH a reason,
`mod.rs:413-423,707-712`).

`PageHost` is the BROWSE model exactly — chords, digits and clicks all
jump anywhere, no gate. It has **no concept of a gated linear step**,
and it will not grow one for you. So the honest migration is:

- **Browse mode → PageHost, fully.** The bar, the hit-test, the digit
  loop, the next/prev chords all delete.
- **Wizard mode → PageHost in CONTROLLED mode, with your gate logic
  kept app-side** driving the same signal, and free navigation
  *disarmed* while `wizard` is true (`.number_jump(false)` +
  `.chords(&[], &[])` — the empty-set disarm, see §3). Your
  `wizard_next`/`wizard_back` keep doing the gating; they just write
  PageHost's `active` signal instead of `ui.screen` directly.

That single decision — one PageHost, its free-navigation surface armed
in browse and disarmed in wizard — is the migration. Everything below
implements it.

## 1. Controlled `active`: bridge your `usize` screen to PageHost's id

`PageHost` addresses pages by STRING id and takes an
`active: Signal<String>` in controlled mode. Your source of truth is
`ui.screen: Signal<usize>` (`mod.rs:43`), read as an index all over
the wizard gate (`screen == 0` checks, `mod.rs:579`). Don't churn that
— bridge it. Add stable ids parallel to `SCREENS`:

```rust
pub const SCREEN_IDS: [&str; 6] =
    ["connection", "providers", "routes", "users", "runtimes", "review"];
```

Keep `ui.screen: Signal<usize>` as the truth and give PageHost a
derived string signal kept in lockstep by one two-way effect (the
pattern your store already uses):

```rust
let active = cx.signal(SCREEN_IDS[ui.screen.get_untracked()].to_string());
// usize -> id (wizard gate + digit refusals still write ui.screen)
cx.effect(move || {
    let id = SCREEN_IDS[ui.screen.get().min(5)];
    if active.get_untracked() != id { active.set(id.to_string()); }
});
// id -> usize (PageHost writes active on a browse click/chord)
cx.effect(move || {
    if let Some(i) = SCREEN_IDS.iter().position(|s| *s == active.get()) {
        if ui.screen.get_untracked() != i { ui.screen.set(i); }
    }
});
```

(Both effects are equality-guarded, so the loop settles in one hop —
no oscillation. If you'd rather not keep two signals, migrate
`ui.screen` to `Signal<String>` and drop the bridge; the gate reads
become `SCREEN_IDS.position(...)`. Your call — the bridge is the
lower-churn path.)

## 2. Replace `screen_bar` + `body` with one PageHost — delete the hit-test

Your `body` (`mod.rs:727-743`) is already the PageHost page region: a
`dyn_view_scoped` matching `ui.screen` to a screen view, with durable
state in `UiState` so remounts are lossless (`mod.rs:1-4` — that IS
the PageHost state recipe, you're already compliant). And your
`screen_bar` (`mod.rs:662-725`) is a hand-rolled tab bar whose mouse
handler **mirrors the draw arithmetic** to hit-test clicks
(`mod.rs:694-698`: "the hit-test mirrors the draw arithmetic above").
That mirror is exactly the drift class PageHost's single-plan design
exists to kill — one `plan_bar` feeds both the draw and the click, so
the arithmetic can never disagree.

**Before** (`root`, `mod.rs:426-431`):

```rust
root_el
    .child(header(cx, &ctx, theme))
    .child(screen_bar(cx, &ctx, theme))   // hand-rolled bar + hit-test
    .child(body(cx, &ctx, theme))          // dyn_view_scoped screen match
    .child(footer(cx, &ctx, theme))
    .build()
```

**After**:

```rust
use abstracttui::widgets::PageHost;

let host = PageHost::new()
    .page("connection", "1 Connection", {
        let ctx = ctx.clone();
        move |gcx| connection::view(gcx, &ctx, &theme.get().tokens)
    })
    .page("providers", "2 Providers", { /* … */ })
    .page("routes", "3 Routes", { /* … */ })
    .page("users", "4 Users & Entities", { /* … */ })
    .page("runtimes", "5 Runtimes", { /* … */ })
    .page("review", "6 Review & Test", { /* … */ })
    .active(active)                         // controlled (from §1)
    .number_jump(!wizard_now)               // see §3 (armed in browse only)
    .chords(&prev_chords, &next_chords)     // see §3
    .on_change(|_id| { /* screen-entry loads already fire via your
                          existing effect on ui.screen — nothing here */ })
    .view(cx);

root_el
    .child(header(cx, &ctx, theme))
    .child(host)                            // bar + page region, one widget
    .child(footer(cx, &ctx, theme))
    .build()
```

**Deleted**: the whole `screen_bar` function including its
mouse-hit-test (`mod.rs:662-725`, ~64 lines — the F4 drift-prone
arithmetic mirror is gone by construction); the `body` function
collapses into the `.page()` builders (its `dyn_view_scoped` is what
PageHost runs internally). Your screen `view` functions
(`connection::view`, `providers::view`, …) are UNCHANGED — they still
receive the generation scope and the tokens.

Note on the tab TITLES: PageHost renders the title string verbatim.
Your bar prefixed `N.name` (wizard) / `N name` (browse) and greyed
future steps — PageHost has one active/idle style (active `text`+bold,
idle `text_muted`) and no per-tab grey. Fold the number into the title
("1 Connection") for a stable label across modes; the wizard's
"future step is greyed/locked" affordance is a mode you'll express
differently (see §4's honest limit).

## 3. Arm free navigation in browse, disarm it in wizard

PageHost's digit jumps and chords are the browse gestures. Gate them
on `wizard`:

```rust
let wizard_now = ui.wizard.get_untracked();
let (prev_chords, next_chords) = if wizard_now {
    (Vec::new(), Vec::new())         // disarm: the wizard walks linearly
} else {
    (vec![KeyChord::new(Mods::CTRL, Key::Char('p'))],
     vec![KeyChord::new(Mods::CTRL, Key::Char('n'))])
};
```

- **Digit jumps**: `.number_jump(!wizard_now)` — off in wizard (your
  digit-refusal-with-a-reason at `mod.rs:409-425` is a wizard behavior;
  keep that refusal as a root-level shortcut ONLY while wizard, and let
  PageHost own digits in browse). PageHost digits ride the shortcut
  table, so a focused `TextInput` keeps its digits (your Connection URL
  field at boot still types "8080") — same property your loop had.
- **Chords**: your browse next/prev is Ctrl+N/Ctrl+P (`mod.rs:376-381`);
  PageHost defaults to Ctrl+PgUp/PgDn, so pass your pair via
  `.chords()`. The wizard's gated Ctrl+N/Ctrl+P (which run
  `wizard_next`/`wizard_back`) STAY as your root-level shortcuts while
  wizard is true — they must gate, and PageHost won't. Empty chord sets
  in wizard mean PageHost's Capture interceptor is fully disarmed, so
  your root shortcuts see the keys uncontested.
- **`PageHost` rebuilds when `wizard` flips**: because the builder
  reads `wizard_now` at build time, wrap the host in a `dyn_view` that
  reads `ui.wizard` so flipping mode re-arms/disarms. (One extra
  `dyn_view` around the host; the pages themselves don't remount —
  their state is in `UiState`.)

## 4. Honest limits — what PageHost will NOT do for you

- **Gated/greyed steps.** PageHost draws no "locked future step"
  affordance and enforces no progression order. Your wizard's greyed
  tabs (`mod.rs:680-682`) and the connection gate
  (`mod.rs:579-603`) are app policy and stay app policy — they now
  write `active` (via `ui.screen`) instead of being enforced by the
  bar. If the greyed-future-step LOOK matters to you, that is a real
  gap worth filing (a per-tab `enabled`/`locked` state on PageHost);
  today the wizard's linearity is enforced by your gate logic refusing
  to advance, not by the bar dimming.
- **Chord defaults.** PageHost defaults to Ctrl+PgUp/PgDn (the wire
  every terminal delivers); you MUST pass `.chords()` to keep Ctrl+N/P.
  Plain `]`/`[` (`mod.rs:367-372`) are consumed by a focused input —
  your own comment says so (`mod.rs:373-375`) — and PageHost digits
  have the identical property, so nothing regresses there.
- **Focus-init.** PageHost's chords are Capture-phase on the HOST root
  and are live only while focus is INSIDE the host; with nothing
  focused, keys go to the tree root. Your shell mounts the host under a
  header (not as the tree root), so establish focus once at boot
  (`app.tree().focus_first()` after mount, or rely on your Connection
  URL `TextInput` autofocus — it already puts focus inside the body).
  This is the documented wrapper-mount rule (api.md, "Chords"), not a
  bug. Your wizard Ctrl+N/P stay on the root element regardless, so
  wizard navigation is never focus-dependent.

## 5. The drawer opportunity: entity / provider / route detail as a right Drawer

Today your detail views are INLINE strips under the tables. The
clearest case: the entity detail on the Users screen — a fixed 3-row
strip (`users.rs:174-211`) that loads on selection (`substrate`,
`voice`, wake reasons) and competes with the roster for vertical
space. A **right `Drawer`** hosting the FULL entity inspector is the
upgrade: the roster stays on the left, the detail slides in with room
for everything (substrate, voice, reservations, wake reasons, the
manage actions) and closes with Esc:

```rust
use abstracttui::app::drawer::{Drawer, DrawerEdge, DrawerFocus, DrawerSize};

// Built once at the root (like your `modal` slot), toggled from the
// Users screen when a row is opened (Enter / a key):
let entity_inspector = Drawer::new(DrawerEdge::Right)
    .size(DrawerSize::Percent(0.42))
    .focus(DrawerFocus::Passive)     // glanceable: the roster keeps the keyboard
    .title("Entity")
    .install(cx, move |dcx| {
        // reads store.entity_detail (already loaded on selection,
        // users.rs:38-58) — the SAME view code, now with room
        entity_detail_page(dcx, &ctx, &t)
    });
// on Enter over a selected entity:  entity_inspector.open();
```

Why a Drawer and not another Modal: your Modals
(`open_form`/`open_prompt`, `mod.rs:243-283`) are DECISIONS — forms and
confirmations that own the keyboard until answered, correctly modal and
correctly single-slotted. A detail INSPECTOR is glanceable reference
you read while the roster stays live — that is `DrawerFocus::Passive`
(keys stay with the roster; click into the panel to scroll it; Esc
closes). Same shape fits **provider detail** (Providers screen) and
**route detail** (Routes screen). Honest scope: this is a NEW surface,
not a deletion — adopt it where an inline strip is cramping a table,
skip it where the strip is fine. The drawer is an OVERLAY (it covers
the page, leaves your layout untouched), so it composes with the
PageHost underneath without touching §2's work.

Drawer facts that matter for your app:
- **One drawer per edge**; opening a second on the same edge replaces
  the first. Your detail inspectors are all right-edge, so opening the
  provider inspector while the entity inspector is open is a clean
  handoff (the entity one finishes with `DrawerCloseReason::Replaced`).
- **Closed = disposed** (the Tabs/PageHost recipe again): the detail
  view rebuilds per open reading `store.entity_detail`, so the loaded
  data (app-owned in your store) survives close/reopen for free.
- **Modal-from-drawer works**: a "rotate token" confirm opened from
  inside the inspector layers above the drawer correctly (the z laws),
  and returns input to the drawer on close — so the inspector can host
  action buttons that open your existing ChoicePrompts.

## 6. What NOT to change (save the time)

- **Your Modal slot + token queue + prompt gating** (`mod.rs:118-283,
  530-560`). This is careful, correct sequencing (never bulldoze an
  unread token; same-z stacked-modal hazard avoided by one-at-a-time).
  PageHost/Drawer change nothing here — decisions stay Modals. The one
  adjacent win: the drawer's own Esc/close is built in, so an inspector
  drawer needs no `open_form`-style closer plumbing.
- **Ctrl+L → `request_full_redraw()`** (`mod.rs:403-405`) — already the
  engine verb, keep it.
- **Toast notices** (`mod.rs:513-528`) — unchanged; they layer above
  everything including drawers.
- **The header and footer** — plain rows around the host, untouched.
- **Your screen `view` functions** — PageHost calls them verbatim on
  the generation scope. Zero changes inside a screen.

## 7. Migration checklist (one commit-sized step each, suite after)

1. Bump the dep to 0.2.12; confirm the suite is green (no API breaks).
2. Add `SCREEN_IDS` + the two-way `active` bridge (§1). Suite green,
   behavior identical (nothing renders differently yet).
3. Swap `screen_bar` + `body` for one `PageHost` in browse mode only
   (wizard still uses the old bar behind an `if wizard` for one commit
   if you want a safe intermediate). Delete the hit-test. (§2)
4. Fold the free-navigation arming (§3) + the wizard root-shortcut
   retention; delete the digit loop's browse half.
5. Delete `screen_bar` entirely once wizard rides the same PageHost.
6. (Optional, separate) Adopt the entity-detail right Drawer (§5);
   then provider/route if the inline strips warrant it.
7. File the gated-step gap (§4) if the greyed-future-step look matters
   — that is the one honest thing PageHost lacks for your wizard.

Everything above was verified against your working tree
(`console-tui` at `abstracttui = "0.2.9"`, six screens, dual-mode
shell) and the engine's 0.2.12 surface on 2026-07-24. `PageHost` docs:
`docs/api.md` "widgets::PageHost"; `Drawer` docs: `docs/api.md`
"app::Drawer"; both demoed together in `examples/shell.rs` and the
acceptance battery `tests/wave_shell_accept.rs` (a PageHost + a modal
right Drawer + a passive left Drawer + a Modal-from-drawer + a Toast,
driven end-to-end through the real Driver).
