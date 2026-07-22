# 0560 — App header + banner primitives (attention, context, actions)

## Metadata
- Created: 2026-07-22
- Status: Proposed
- Track: app-kits
- Completed: N/A
- Depends on: 0540 (the account chip IS a 0540 Chip). Cross-band,
  non-blocking: control-plane 0300 typed events are the notices
  bridge's eventual classifier (v1 is string-convention).
- Validator (0590): `examples/admin_console` (header + pinned
  admin-context banner) + `examples/triage_shell` (attention banner
  with action).
- Promotion trigger: the 0590 admin-console validator (its header +
  admin-context banner are the acceptance surface), or any dogfood app
  needing a persistent attention line.

## ADR status
- Governing ADRs: None — no ADR system in this repo yet (see 0170).
  ADR impact: none (two small composition widgets over existing
  primitives).

## Context
Two chrome primitives recur in every reference UI and currently have no
home. (1) The APP HEADER: the admin console's top bar with title,
ACCOUNT CHIP, and sign-out action (A); the wizard's step-context header
(B); the file manager's path/actions bar (D). (2) The BANNER: the admin
console's persistent ADMIN-CONTEXT banner ("you are acting as admin on
tenant X") (A); the chat shell's ATTENTION banner ("69 threads need
vigilance — Review") with an action button (C); degradation/offline
notices any live-data app must show honestly (the roadmap's
honest-degradation principle — capability gaps "surface as notices",
0001 roadmap "Design principles" §4). A banner differs from a `Toast`
in exactly one property: it PERSISTS until its condition clears or the
user acts — toasts are fire-and-forget chips that remove themselves
(`src/app/popups.rs:136-159`).

## Current code reality
- **No header widget**: the dashboard builds its header ad hoc
  (`examples/dashboard/main.rs:3` names "header/sidebar/chart grid/…"
  as hand-composed layout; the header fn composes `Block`/text/`Logo`
  directly). `Logo` exists for the brand slot
  (`src/widgets/logo.rs` via mod.rs:57), `Badge`/0540's Chip covers
  the account capsule, `Button` the sign-out — the missing piece is
  the standardized three-region bar (leading / center / trailing) with
  overflow priority (which region truncates first).
- **No banner widget**: `Toast` is transient by design (auto-dismiss
  timer, slide out, layer removed — popups.rs:140-214) and lives in
  the overlay bands (TOAST_Z=2000, popups.rs:31-32); a persistent
  banner is IN-FLOW chrome (a row above content that reflows the
  layout), not an overlay — nothing renders that today.
- **Startup notices have a reactive source and no chrome**:
  `use_startup_notices(cx)` exposes labeled degradations as a signal
  (`src/app/notices.rs:40-45`); the dashboard drains it into toasts
  (`examples/dashboard/main.rs:105-121`) — transient rendering of
  persistent facts, which the example itself had to special-case
  (skipping `caps:` lines). A banner is the honest renderer for the
  persistent subset.
- **Tone vocabulary + tokens are ready**: `Tone::{Info,Warn,Error,Ok}`
  (`src/widgets/badge.rs:22-30`), `surface_raised` ground, semantic
  inks with audited floors (docs/theming.md:50-53,163-183). The 0240
  `shrink(0.0)` rule for one-row chrome (mod.rs badge comment,
  badge.rs:70-77) applies to both widgets — headers and banners must
  never be crushed by overflowing content.
- **Action slots**: `Button` in-flow; banner actions are ordinary
  focusable children — the focus order takes care of reachability
  (`src/ui/view.rs:203-208`).

## Problem
Every app hand-assembles its header, and attention/context facts either
borrow `Toast` (wrong: they vanish) or squat in ad-hoc rows with
invented colors. The chat brief's attention banner — a persistent,
counted, actionable line — has no engine expression at all, and
"admin context" (a safety-relevant fact) deserves a standard, loud,
theme-audited rendering rather than per-app improvisation.

## What we want
1. **`AppHeader`**: a one-row (optionally two-row) bar with three
   slots: `leading: View` (brand/`Logo`/title), `center: View`
   (breadcrumb/context text, truncates FIRST), `trailing: View` (chips
   + actions, `shrink(0.0)` — never crushed). Ground `surface_raised`,
   bottom hairline `border` optional. It is a layout+truncation-policy
   widget, deliberately thin: the slots are ordinary Views (compose.rs
   props convention, `src/ui/compose.rs:5-10`), so the account chip is
   a 0540 `Chip`, sign-out a `Button` — no bespoke sub-widgets.
2. **`Banner`**: an in-flow row (not an overlay): tone (Info/Warn/
   Error/Ok → semantic ink + a tinted-ground recipe within the audited
   floors), message (truncates with ellipsis; optional detail line),
   optional leading glyph per tone, `actions: Vec<(label, Callback)>`
   rendered as compact buttons, optional dismiss (`✕`) firing
   `on_dismiss` — the APP owns the visibility signal; the banner never
   hides itself (persistence is the point; the condition's owner clears
   it). Height `Cells(1)` (+1 with detail), `shrink(0.0)`.
3. **`BannerStack`**: orders multiple live banners by severity
   (Error > Warn > Info > Ok) with a `max_visible` and an honest
   `+N more` collapse row (activatable → the app shows a list); the
   admin-context banner class pins itself above severity ordering via
   an explicit `pinned` flag (context outranks noise).
4. **Notices bridge (recipe, not machinery)**: a documented pattern
   mapping `use_startup_notices` entries to Banner/Toast by class —
   persistent capability gaps → banners, one-shot events → toasts —
   replacing the dashboard's hand special-case; lives in the docs page
   with the 0590 validators consuming it. Honest limit (PLATFORM
   cycle-2 F11): v1 classifies notice STRINGS by the documented
   "area: state (detail)" convention (src/app/mod.rs:196-200) —
   brittle by nature; when control-plane 0300's typed lifecycle/
   degradation events land, the recipe migrates to typed
   classification rather than growing its own string parser.
5. **Theming law (RESOLVED per PLATFORM cycle-2 F8 — the cycle-1 text
   hid a contradiction)**: "derive at theme-build with no new tokens"
   was incoherent — derived values need token SLOTS to land in
   (`TokenSet` is a fixed 36-token model), and computing tints in the
   widget is lint-forbidden (src/widgets/mod.rs:8-15). **v1 commits to
   the existing vocabulary**: `surface_raised` ground + semantic ink +
   a tone-colored leading glyph and hairline — all already-audited
   pairs, zero governance change. Per-tone tinted GROUNDS are a
   THEME-lane follow-up, handed to the integrator as a one-line note
   (final wording in reviews/study/appkits-cycle4.md "Integrator
   handoff block"; first recorded in appkits-cycle3.md — this band
   invents no tokens): if validator use proves the v1
   rendering insufficiently loud, the theme lane adds a banner-ground
   token family + its contrast-audit pairs across all 26 built-ins,
   under 0170's budget.

## Scope / Non-goals
Scope: AppHeader, Banner, BannerStack, the notices-bridge recipe,
contrast validation, gallery + validator adoption.
Non-goals: menus in the header (0500's popup core grows a Menu later;
until then trailing actions are buttons); marquee/animation; global
banner state management (visibility signals are app-owned; the stack
renders what it is given); toast changes (Toast stays exactly as is —
the two widgets split transient/persistent duties).

## Expected outcomes
The admin console's header (title + account chip + sign-out) and its
always-visible admin-context banner are two calls; the chat shell's
"N threads need vigilance — Review" line is a Banner with an action;
capability degradations get a persistent, honest, theme-audited home
instead of vanishing toasts.

## Validation
- Unit: truncation priority (center first, trailing never); severity +
  pinned ordering in the stack; `+N more` math; dismiss fires callback
  without self-hiding.
- CaptureTerm acceptance: header + two banners + content — content
  overflow squeezes content, never chrome (0240 semantics); banner
  action reachable by Tab and fires; theme switch restyles tones; a
  40-col terminal still shows the trailing account chip (center gave
  way).
- Contrast: banner ink/ground pairs measured against
  `theme::contrast::floors` across all 26 built-ins (the audit exists —
  docs/theming.md:165-171 — extend its pair list if a derived ground
  ships).

## Progress checklist
- [ ] AppHeader (three slots, truncation priority, shrink rules)
- [ ] Banner (tones, actions, dismiss-without-self-hiding)
- [ ] BannerStack (severity + pinned ordering, +N collapse)
- [ ] Notices bridge recipe (persistent→banner, transient→toast)
- [ ] Contrast measurements across built-ins; gallery + validator use
