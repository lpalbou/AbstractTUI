# Wave 13 — the "modal crashes the background" report (gateway console)

Seat: MODAL-BG. Consumer: the gateway seat (console-tui owner).
Status: **diagnosed — console-side composition; engine pipeline proven
honest at HEAD with a driver-level regression net.** No engine code
changed this wave.

## The report

Operator screenshot 2026-07-25, the console's Review & Test wizard
screen with the sandbox-test modal open: "we have a graphical bug in
console-tui of gateway with that modal that crash the background."
Around the modal the page reads as BLANKED — large empty regions above
and below where the wizard page content should be — while the sandbox
result block + footer appear below where they belong, as if the page
relaid out or was partially painted.

## Verdict up front

1. **The engine's incremental pipeline is NOT at fault.** The console's
   exact screen shape was rebuilt from engine widgets and driven
   through the real `Driver`/`CaptureTerm` along the operator's whole
   session — open, provider-Select popup over the modal, toast raised
   and expired over the modal, typing, page-store updates re-rendering
   the page's dyn regions UNDER the open modal, busy-line ticking,
   resize excursions (including width- and height-clamped panels and a
   page in clean-absence crush), close — at 110x32 (the console's boot
   size), 100x40, 100x28, 100x14, 80x24, 46x28 and 46x24. At EVERY
   step two oracles held byte-for-byte:
   - a forced `request_full_redraw()` changes not one cell (the
     incremental frame equals the fresh paint);
   - no cell outside the modal panel ∪ the step's legitimately-changed
     bands ever changes.
   Net: `tests/wave_modal_bg.rs` (5 driver-level tests, all green at
   HEAD).
2. **The screenshot is the DESIGNED composition of an oversized,
   undressed, translucent modal panel over a dark page** — three
   console-side choices compound into "crashed background". The fix is
   one chokepoint in the console (`open_form_guarded`), detailed below.

## The screenshot, decoded (engine-level reproduction)

The fixture at the console's own boot size (110x32), fresh wizard
(empty journal), sandbox modal (the console's `Size::new(76, 18)`)
open. This is the composed screen the referee terminal shows —
structurally the operator's screenshot:

```text
╭ Changes this session (apply → verify via GET) ──────────────────────────────────────────────╮
│no changes applied this session — the wizard only writes when you save                       │
│                                                                                             │
│                                                                                             │   ← panel top edge (row 7, invisible)
│                 Sandbox test — real generation                                              │
│                 provider           ▐choose a provider…                   ▾▌                 │
│                 model              choose a provider first                                  │
│                 prompt             ▐Reply with one short sentence: what model are you?▌     │
│                 no test run yet                                                             │
│                                                                                             │   ← 3 more result rows (h(4) slack)
╰────────────────  Generate    Close (Esc)                                    ───────────────╯   ← modal buttons ON the journal's border row
                                                                                                  ← panel void rows (17..24): nothing but dim
╭ Live test (sand                                                             ───────────────╮   ← block title torn at the panel's left edge
│run a REAL text                                                                              │
│no test run yet                                                                              │
│                                                                                             │
│ Run a test (g)                                                                              │   ← below the panel: intact again
╰─────────────────────────────────────────────────────────────────────────────────────────────╯
 Finish — switch to browse mode
 Step goal: run one real test (g) to prove a provider, then Finish.
```

Every pixel of that render is honest per the engine's documented
veil semantics — and it is exactly what the operator described:

- **"large empty regions above and below"** — the panel is 18 rows for
  ~11 rows of content: 7 rows of featureless dim void sit INSIDE the
  panel below the buttons. Above, the fresh wizard's journal block is
  legitimately empty.
- **"the sandbox result block appears below where it belongs"** — the
  panel's lower void rows erase the TOP of the live-test block (title
  and first rows torn mid-row at the panel columns); the block's
  visible remnant starts lower, so it reads as pushed down. Nothing
  moved: the full-redraw oracle proves the layout identical.
- **"crash the background"** — the modal panel has NO visible
  boundary. Its ground is the `overlay` token, rgba(0,0,0,0.45),
  composited over the console's dark page: page bg #16213E darkens to
  ≈ #0C1222. Two near-black navies side by side, no border, no title —
  the eye cannot see "a dialog"; it sees the page with a torn dim hole
  and stray button text sitting on a block border row
  (`╰──── Generate  Close (Esc) ────╯`).

Why engine prompts never look like this: `ChoicePrompt` computes its
panel FROM its content (no void rows), so its dim rectangle hugs a
dense card and reads as a dialog. The console's forms float ~11 rows
in an 18-27 row panel.

## Version facts

- The console's `Cargo.toml` pins `abstracttui = "0.2.20"` (caret);
  its `Cargo.lock` resolves **0.2.21** — that is what the operator
  runs.
- HEAD (0.2.22) differs from 0.2.21 only by `Block::on_close`
  (additive, widgets/block.rs) and 0.2.21 itself added only the theme
  polarity/switcher surface — nothing in the modal/overlay/compositor
  path changed since the operator's binary. The green net at HEAD
  therefore speaks for 0.2.21 as well; no behavior in this class
  regressed OR was fixed between the pin and HEAD.

## The console fix (exact, one chokepoint)

All 19 console modals flow through ONE wrapper:
`console-tui/src/ui/mod.rs::open_form_guarded` (line ~493). Dress the
panel there — visible boundary first, size trim second:

```rust
let modal = Modal::open(&ctx.overlays, cx, viewport, size, move |mcx| {
    let t = use_theme(mcx).get().tokens;
    Element::new()
        .style(LayoutStyle::fill())
        .shortcut(KeyChord::plain(Key::Escape), move |_| {
            if let Some(g) = guard_esc.borrow().as_ref() {
                if g() {
                    return;
                }
            }
            c_esc()
        })
        .child(
            // The dialog SURFACE the operator can see: an opaque
            // raised ground + border + title. The engine's translucent
            // panel ground stays visible only as the 1-cell margin
            // around this block (the Modal's own padding) — the torn-
            // block/void-band perception disappears.
            Block::new()
                .border(BorderKind::Rounded)
                .fill(t.surface_raised)
                .layout(LayoutStyle::column().grow(1.0).padding(Edges::all(1)))
                .child(build(mcx, closer.clone(), guard.clone()))
                .element(&t)
                .build(),
        )
        .build()
});
```

Notes for the gateway seat:

1. `t.surface_raised` is opaque — lower page glyphs stop mattering
   entirely inside the dialog; the border gives the eye the boundary
   the screenshot lacked. (`Block::title(...)` per call site if you
   want named dialogs; the form's own heading row already serves.)
2. **Size trim (second-order polish):** with the border the content
   budget inside the panel is `size.h - 4` (panel padding 2 + border
   2). The sandbox modal's tallest arm (failed model discovery: input
   + error + retry button) is 11 rows → `Size::new(76, 15)` replaces
   `(76, 18)`. Audit the other 18 sizes with the same formula; the
   routes editor (78x27) has the most void today.
3. Nothing engine-side blocks this: `Block` + tokens are already in
   the console's imports, and the change is inside the one wrapper —
   `open_form` delegates to it, so both doors get the dressing.
4. If you prefer the translucent look, the alternative is sizing every
   panel to content (the ChoicePrompt discipline) — but the boundary
   is what kills the "crashed background" reading; do that one first.

## Engine follow-ups (recommendations only — not shipped this wave)

- **Backlog seed:** an opt-in dressed panel on `Modal` (e.g.
  `Modal::open` with a builder carrying `.frame(BorderKind)` /
  `.ground(TokenId::SurfaceRaised)`), semver-additive, so app authors
  fall into the visible-dialog pit of success instead of discovering
  this composition themselves. Every in-repo consumer (ChoicePrompt,
  dashboard help, attachments picker) hand-rolls content dense enough
  to survive the naked panel; the first sparse-form app (the console)
  is the one that got burned.
- **api.md line (docs seat):** the Modal section should state plainly
  that the panel ground is translucent and borderless by design, and
  that form-style content should bring a `Block` (or size the panel to
  content) — with the console incident as the motivating example.

## The regression net (stays green, pins the class)

`tests/wave_modal_bg.rs` + `tests/wave_modal_bg_parts/fixture.rs`
(console-shaped fixture: PageHost root, review page, sandbox modal,
notice→Toast effect, busy/tick footer):

- `modal_open_never_disturbs_the_page_outside_its_panel` — open at
  100x40 AND at 100x14 (panel height-clamped, page in clean-absence
  crush): outside the panel not one cell changes; fresh-paint equal.
- `typing_in_the_modal_leaves_the_page_untouched` — per-keystroke
  outside-panel integrity; fresh-paint equal after.
- `page_dyn_update_under_open_modal_stays_honest` — the page's h(4)
  result dyn re-renders UNDER the open modal (Loading → Ready);
  fresh-paint equal at each flip; only panel ∪ the live-test band may
  differ from the post-open screen.
- `resize_while_modal_open_then_close_matches_fresh_paint` — resize
  ladder (90x30 → 120x44 → 100x40) with fresh-paint oracles, then
  close: the final screen equals the pre-open screen byte-for-byte.
- `operator_session_full_walk_across_sizes` — the full session
  (open with the store reset riding the same batch, Select popup
  open/move/commit over the modal, toast parked + expired over the
  modal, Generate + busy ticking, result Ready, resize down/up, close)
  at 110x32 / 100x28 / 80x24 / 46x28, fresh-paint oracle at every
  step, byte-integrity vs pre-open at close (PageHost's sticky tab
  window and the footer status row are the two documented stateful
  exclusions).

Method note for future waves: the two oracles (forced-full-redraw
equality + outside-band byte integrity) decided engine-vs-console here
without reading a line of console code on faith; keep them as the
first tool whenever "the background broke under an overlay" reports
arrive.
