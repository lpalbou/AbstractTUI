# Consumer tensions: abstractcode-tui on abstracttui 0.2.1 (FIELD, study 2)

Evidence base: the consumer's full current source at
`~/tmp/abstractframework/abstractcode-tui` (commit 34b9447 "new version of
abstract code tui", the squashed 0.2.0→0.2.1 adoption wave; 10,241 lines of
Rust) read end to end, its CHANGELOG.md (which documents each adoption and
each deliberate non-adoption), and the engine source cross-checked for every
claim the consumer makes about engine behavior. Every claim below cites
file:line. Paths are consumer-relative unless prefixed `engine:`.

The headline is honest and two-sided: the 0.2.x adoption wave was a
SUCCESS — the app deleted its hand-rolled transcript column, height math,
autoscroll effects, and focus bookkeeping (CHANGELOG 0.2.0 "Changed (engine
adoption)"), and the consumer's own module doc calls the transcript "a
PROJECTION" now (src/ui/transcript_view.rs:1-16). The tensions below are
what's left AFTER a well-executed adoption — which makes them the honest
residue the engine should care about.

---

## 1. Adoption scorecard

| Engine feature | Verdict | Evidence (consumer) |
| --- | --- | --- |
| `widgets::Feed` | ADOPTED — whole transcript | `FeedState::new` ui/mod.rs:162; `Feed::new(&feed).gap(1)` transcript_view.rs:648 |
| `Scroll::follow_tail` | ADOPTED | transcript_view.rs:650; re-arm shortcuts ui/mod.rs:157-161, 217-234 |
| `Feed::total_rows()` (measured extent) | ADOPTED — replaced app measure math | ui/mod.rs:201, 227 ("feed.total_rows() replaces the old measure math") |
| `widgets::TextArea` + `TextAreaState` | ADOPTED — composer, durable state in root scope | chrome.rs:296-324; ui/mod.rs:171-173 |
| `app::anchored::Completion` | ADOPTED — `/` command dropdown | chrome.rs:348-371 |
| `reactive::interval` | ADOPTED — run ticker + idle probe | ui/mod.rs:660-696 |
| `app::selection` (0270 tier 3) | ADOPTED always-on | lib.rs:155-161 |
| `List::on_activate` (0250 fix) | ADOPTED — all five pickers | modals.rs:317-329, 360-366 |
| Diff tinting (`text::DiffLexer` via Feed fences) | ADOPTED — zero app code | CHANGELOG 0.2.1 "Diff tinting comes free"; pixel-level test noted there |
| `.autofocus()` in dyn regenerations (0220 fix) | ADOPTED — deleted `UiCtx::tree` + `focus_composer` | chrome.rs:280-284; lib.rs:226-227; CHANGELOG "Deleted:" |
| `use_startup_notices` | ADOPTED — surfaced as toasts | ui/mod.rs:700-712 |
| `Toast`, `Modal`, `Spinner`, `Sparkline`, `Logo`, `Block` | ADOPTED | ui/mod.rs:715-734; modals.rs (throughout); chrome.rs:5, 253-270; modals.rs:1306 |
| `gfx::mosaic` + `decode_image` | ADOPTED (as 0280's documented workaround) | transcript_view.rs:213-240; runner.rs:780 |
| `WakeHandle::post` threading contract | ADOPTED as the app's one rule | runner.rs:1-23; docs/architecture.md "Threading contract" |
| `bounded_source` / `channel_source` | NOT ADOPTED — deliberate, argued | runner.rs:11-23 (see §2.1) |
| `app::select` faces (Select/Combobox/MultiSelect) | NOT ADOPTED — filed 0296 | CHANGELOG 0.2.1 "Evaluated, deliberately NOT adopted" |
| `Table` | NOT USED — no table-shaped surface in this app | (pickers are lists; transcript is a feed) |
| `Image` widget in transcript | CANNOT — filed 0280 | transcript_view.rs:214-216 |
| `TextInput::masked` | NOT USED — "no in-TUI secret entry (login is CLI-only)" | CHANGELOG 0.2.1 |

Six upstream reports filed by this consumer (0260, 0280, 0290, 0292, 0294,
0296) — all six reproduce from the current source. The scorecard's value is
the pattern: everything the engine shipped as a WIDGET with owned state got
adopted wholesale; the places the consumer still hand-rolls are all places
the engine offers a MECHANISM but no POLICY (see §3).

## 2. Non-adoptions, with their reasons read from the code

### 2.1 `bounded_source`/`channel_source` — shape mismatch, correctly refused

runner.rs:11-23 is the clearest piece of consumer feedback in the repo: the
live-data sources bind "homogeneous DATA streams" to `Signal<Vec<T>>` with
overflow policies, but ledger records are "ordered STATE DELTAS folded into
`Fold` (a dropped record is a lost tool result or a lost wait —
`DropOldest`/`Coalesce` would be silent corruption, and an unbounded
`channel_source` accumulates into a Vec nobody reads)". Their verdict:
"`wake.post` of fold closures IS the sanctioned transport for this shape."

**Engine take-away**: no new source type needed — the consumer is right that
post-a-fold-closure is the correct lane — but `docs/live-data.md` should
BLESS the fold-closure pattern explicitly as the third shape next to
`channel_source`/`bounded_source`, or every stream-of-deltas consumer will
re-litigate this. What they did adopt from the wave is the worker-panic
surfacing discipline (runner.rs:19-23, 140-178).

### 2.2 `app::select` faces — programmatic open missing (filed 0296)

The pickers stay `List`-in-`Modal` because the faces "open only from their
own trigger rows — a command-summoned picker (`/theme`, `/model`) would cost
an extra keystroke and duplicate Escape-revert logic" (CHANGELOG 0.2.1).
Filed; nothing to add except a confirmation that the app's `Picker` shell
(modals.rs:330-373) is the exact API shape a programmatic open should make
deletable.

### 2.3 `List` for multi-select — the checkbox-list hole (NOT fully filed)

modals.rs:633-641 explains the hand-rolled row surface: "`List::on_select`
fires on plain arrow movement BY DESIGN … the new `on_activate` fires on
Enter AND Space, so a List-based multi-select could not tell Space-toggles
from Enter-closes either." Verified engine-side: `List::on_activate` fires
on Space too (engine:src/widgets/list.rs:167-175). 0296 asks for a
programmatic open of the popup MultiSelect — but the `/tools` and `/skills`
surfaces are EMBEDDED checklists (grouped rows, windowing, Space toggle,
`a`/`n` bulk ops), which no select face serves even with a programmatic
open. That's ~380 lines of hand-rolled widget (draw_rows modals.rs:644-731 +
the two modals 736-1022) for a shape the app-kits track almost names
(0550's NavList is single-select; 0530 is a table). **Unfiled gap: an
embeddable CheckList/MultiSelect list widget** — see §3.4.

## 3. Brittleness they did NOT file

The four filed reports (0280/0290/0292/0294, + 0296) are real; these seven
are the ones the code carries without an upstream item.

### 3.1 Button disposal hazard — the one-tick modal retire deferral (P1)

`UiCtx::retire` (ui/mod.rs:67-94): every modal close removes the layer NOW
but defers scope disposal one tick via `after(Duration::ZERO, …)`, because
"`Button`'s mouse path still writes its own `pressed` signal AFTER
`on_click` returns … a synchronous modal close from a mouse-clicked
approve/deny button would still die with 'handle used after its node was
disposed'". Verified engine-side: `fire(); pressed.set(false)`
(engine:src/widgets/button.rs:189-197). The consumer's own comment names the
engine rule that should exist: "Delete only when EVERY widget callback that
can close a modal is disposal-safe." 0250 fixed List AND Table; Button (and
any other post-callback signal write — Checkbox, Radio, Tabs need an audit)
was left out. **This is the most item-worthy unfiled finding**: the 0250
bookkeeping-before-callback ruling, applied engine-wide, deletes the retire
deferral and its two paragraphs of justification in every consumer.

### 3.2 Modal replacement semantics — atomic replace + drop-does-not-close (P1)

`UiCtx::open_modal` (ui/mod.rs:115-141) documents a live incident class: a
close-then-open sequence runs `maybe_flush` synchronously, an effect
observes "pending wait + no modal" in the gap, re-opens re-entrantly, and
the outer open then overwrites the slot — "dropping the prompt's `Modal`
handle WITHOUT closing it (drop does not close). The leaked layer swallowed
every key while the new modal painted over it: a visible, dead picker (live
2026-07-21)." The engine backlog already records the same-z stacking hazard
(overview.md "Same-z Modal stacking hazard") — this consumer independently
paid for BOTH halves of it in production. The app-side fix (a one-slot
`Rc<RefCell<Option<Modal>>>` + epoch signal, ui/mod.rs:50-64) is exactly the
"modal slot" policy every modal-using app will need. When the 0510/0520/0530
stacked-dialog story lands, `Modal` should grow either close-on-drop or a
loud debug notice for dropped-open modals.

### 3.3 Caller-computed modal sizes — six height formulas (P2)

`Modal::open` takes a fixed `Size` up front, so every surface hand-computes
chrome arithmetic: modal_size clamps (modals.rs:19-24), the approval modal's
"panel padding 2 + content padding 2 + title 1 + gaps 3 + buttons 1 + hint 1
= 10 fixed rows" (modals.rs:107-109), the sessions picker's "padding 2 +
title 1 + hint 1 + inter-child gaps 2 = 6 fixed rows" (modals.rs:1050-1052),
and the shared `Picker` shrugs: "Caller-computed (each picker's height
arithmetic differs)" (modals.rs:334). The engine already measures content
honestly elsewhere (Feed's extent, 0130); a content-sized modal (measure the
built view, clamp to viewport) deletes all six formulas. The 0240 completion
(overflow defaults + debug notice) softened the failure mode; the arithmetic
itself is still consumer-side.

### 3.4 The embedded checklist widget (P2)

§2.3 — ~380 lines including hand-rolled windowing with honest "↑ N more"
overflow markers (modals.rs:673-696), cursor-follow logic, and a documented
live defect they had to fix themselves ("Window against the RECT the layout
actually granted — never a precomputed height", modals.rs:658-663). Two of
their four 0.1.0 adversary findings (2, 6 — stale-name arithmetic) lived in
this hand-rolled surface. A `CheckList` (or an embeddable MultiSelect mode)
in the app-kits band deletes it.

### 3.5 SSE + reconnect/backoff — live-data 0040's exact evidence (P1, promotion trigger)

gateway/sse.rs (125 lines, hand-rolled SSE over ureq) + the reconnect loop
in runner.rs:923-1012: linear backoff `500ms × consecutive_errors` capped at
5s (runner.rs:1006-1008), 8-iteration REST poll fallback, terminal-status
probes, fatal-status classification (runner.rs:962-978) — and NO jitter,
which is 0040's named thundering-herd concern verbatim. Live-data 0040
(proposed) describes this machinery almost line for line. **The first
shipped app has now hand-rolled 0040 once; its promotion trigger ("starting
the watcher or either port") has effectively fired early.**

### 3.6 Feed sync machinery — fingerprint diffing + a mirror predicate (P2, design-level)

wire_feed (transcript_view.rs:502-584) + the FNV fingerprint
(transcript_view.rs:389-483) + `is_visible` (transcript_view.rs:591-599) —
~180 lines whose only job is answering "which items changed and may I append
or must I rebuild". The app carries a correctness obligation the engine
created: "feed order is PUSH order, so a key may only be appended when it
lands at the tail; mid-list visibility changes force the rebuild path"
(transcript_view.rs:13-16), plus a test pinning that a hide-predicate stays
byte-exact with the renderer (transcript_view.rs:726-784). Their truncation
work (transcript.rs:312-340, chunked drains) exists to keep this sync
amortized. This is the price of Feed's imperative push/update API against a
`Signal<Vec<Item>>` source of truth. A `FeedState::sync` adapter (key fn +
fingerprint fn + render fn over a slice, rebuild-on-shrink policy inside the
engine) would delete the whole class for every fold-shaped consumer. Not
filed by them anywhere; the module doc presents it as normal cost.

### 3.7 Small duplicated text utilities (P3)

`wrap_capped` + "… (+N more lines)" (transcript_view.rs:179-201),
`wrapped_lines` + "[#TRUNCATION…]" (modals.rs:60-87), `one_line`/`bounded`
(transcript.rs:128-150) — three spellings of "wrap, cap, and say honestly
what was cut" around engine `text::wrap`/`truncate_ellipsis`. A
`text::wrap_capped(source, width, cap) -> (Vec<String>, hidden: usize)`
serves all three. Same family: the seen-counter drain pattern (`Rc<Cell
<usize>>` over a Vec signal) appears twice (ui/mod.rs:704-711, 716-733) —
a signal-as-queue idiom with no engine sugar.

## 4. API-shape frictions (where the engine forced awkward code)

1. **Multi-ink lines need a whole custom-block card system.** The `Card`
   struct (transcript_view.rs:41-177, ~137 lines) exists because "colored
   chrome the theme-ink Text block cannot express"
   (transcript_view.rs:34-35): `FeedBlock` is Text/Markdown/Code/Custom
   (engine:src/widgets/feed.rs:74-94) — no rich-span block, though the
   engine ships `render::rich::RichText` and `RichTextView`
   (engine:src/widgets/richtext.rs:1-20) with exactly the patch-style span
   model needed. A `FeedBlock::Rich(RichText)` deletes the Card's draw
   closure and its hand-wrapping (the height/draw honesty contract,
   transcript_view.rs:100-124) for every consumer whose feed lines mix
   inks — which is every transcript, every log viewer. **This is the
   highest-leverage single addition this consumer's code argues for.**
2. **Right-aligned-tail rows are hand-drawn twice.** `print_clipped`
   (measure right side first, clip the left run under it) is copy-pasted in
   header (chrome.rs:76-97) and status bar (chrome.rs:399-434), both born
   from the same live 80-column overprint defect. The empty state hand-rolls
   centered lines the same way (transcript_view.rs:663-675). App-kits 0560
   (header/banners) covers the header shape; the primitive underneath — a
   spans-row with left/right sections and a clip policy — is what both
   surfaces actually reuse.
3. **Rc/RefCell + epoch-signal juggling for overlay state.** `UiCtx` carries
   `modal: Rc<RefCell<Option<Modal>>>`, `dismissed_wait`, `wait_modal_for`
   (ui/mod.rs:50-64) and a `modal_epoch` signal bumped on every open/close
   whose only purpose is re-running effects that read the non-reactive slot
   (ui/mod.rs:52-56, 751). RefMut-lifetime traps had to be documented twice
   ("Take FIRST in its own statement", ui/mod.rs:97-99, 130). The engine's
   own overlay layer is non-reactive by design; a tiny reactive
   `OverlaySlot` (or just documenting the epoch-signal pattern) would
   prevent each consumer rediscovering the BorrowError path.
4. **Pane-height arithmetic leaks into the app.** `CHROME_ROWS` (ui/mod.rs:
   23-34) + `(vp.h - CHROME_ROWS).max(3)` in three places (ui/mod.rs:206,
   226-228) because `Scroll` exposes no "granted viewport height" and Feed's
   `total_rows` is only half of the max-offset formula. The 33-line comment
   justifying why the estimate errs benignly is the friction made visible.
   A `Scroll` viewport-size signal (or a `page_by(±viewports)` verb and a
   clamp-external-offset-on-shrink policy) deletes both the constant and
   the shrink-clamp effect (ui/mod.rs:188-212).
5. **Interval lifecycle by hand.** Run-scoped ticking needs an effect + an
   `Rc<RefCell<Option<IntervalHandle>>>` slot (ui/mod.rs:660-686). Sugar
   like `interval_while(cx, signal, period, f)` is cheap and matches the
   consumer's exact usage (they even re-derive elapsed seconds inside to
   avoid redundant sets, ui/mod.rs:672-677).

## 5. Top-5 tensions (ranked by what the engine should do about them)

1. **`FeedBlock::Rich` is missing** — the Card system + 137 lines exist
   only because feed lines can't carry spans; the engine already owns the
   RichText model (§4.1). One additive block variant deletes an entire app
   subsystem class.
2. **Disposal safety is not engine-wide** — the 0250 ruling stopped at
   List/Table; Button's post-callback write forces a one-tick retire
   deferral in every modal-closing consumer (§3.1). Audit all widget
   callbacks, extend the ruling, then consumers delete their deferrals.
3. **Feed's sync burden belongs in the engine** — fingerprint diffing, the
   push-order/mid-list-rebuild law, and the visibility-mirror test are ~180
   lines of transferable obligation every fold-shaped consumer will
   re-implement slightly wrong (§3.6). A `sync`-style adapter over a slice
   makes the fast path the default path.
4. **Live-data 0040 has fired its trigger** — the first app hand-rolled
   reconnect/backoff/poll-fallback without jitter (§3.5); the second (any
   entity/monitoring surface, see field-app-classes.md) will too. Promote.
5. **Modal ergonomics: content sizing + slot semantics** — six hand-computed
   height formulas (§3.3), atomic-replace + drop-leak semantics every
   consumer must re-derive (§3.2), plus the known same-z hazard. One
   modal-manager pass (content-sized open, close-on-drop or loud leak
   notice, a blessed slot pattern) covers all three.

Filed-report confirmations: 0280 (mosaic-in-feed workaround verbatim at
transcript_view.rs:213-240), 0290 (the on_change selection-clear workaround
and its honest limit, chrome.rs:300-313), 0292 (the whole-draft trigger
guard, chrome.rs:326-354), 0294/0296 (CHANGELOG-documented). Nothing in the
current source contradicts a filed report.
