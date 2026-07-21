# AbstractTUI Theme Tokens & Visual Identity

Owner: DESIGN. Code: `src/theme/**` (tokens, seeds, registry, derive,
contrast) and `src/boot/identity.rs`. Everything normative in this document
is enforced by unit tests in those files; if the doc and a test disagree,
fix the drift — do not ship it.

---

## 1. The token model

### 1.1 Principles

1. **Widgets speak tokens, never hex.** A widget resolves `TokenId` against
   the active theme's `TokenSet`. Theme switching is a signal write; nothing
   re-styles by string replacement (the abstractcode retint lesson: find/
   replace over shared hexes breaks the moment two roles share a value).
2. **Faithful porting.** Values that exist in AbstractUIC `theme.css` are
   copied hex-for-hex (`src/theme/seeds.rs` is a diffable data table).
   Values the CSS lacks are *derived by documented rules* — never invented
   ad hoc per theme.
3. **Floors are tested, not aspirational.** Every registered theme passes
   `theme::contrast::audit` in CI. Exceptions live in one named list
   (`AUDIT_EXCEPTIONS`) with inline justification and a staleness guard; an
   exception that stops firing fails the build.
4. **Opaque by default.** Only `overlay` and `shadow` carry alpha — they are
   compositor inputs. Everything else is pre-composited at theme build time
   so contrast is a static property, not a function of what happens to be
   underneath.

### 1.2 The TokenSet (28 slots)

| token | role | source |
| --- | --- | --- |
| `bg` | app ground, fills the terminal | `--bg-primary` |
| `surface` | panels, cards | `--bg-secondary` |
| `surface_raised` | popovers, menus, active tabs | `--bg-tertiary` |
| `overlay` | modal scrim (alpha, composited) | `.af-connect-overlay` rgba(0,0,0,.45) |
| `border` | hairlines, pane separators | derived (1.5:1-guarded ink wash) |
| `border_focus` | focused-element stroke | derived (= accent, see 1.4) |
| `text` | primary copy | `--text-primary` |
| `text_muted` | secondary copy, labels | `--text-secondary` |
| `text_faint` | placeholders, disabled, watermark | `--text-muted` |
| `accent` | brand/identity color, primary actions | `--accent` |
| `accent_alt` | curated second accent (gradients) | `theme.ts` swatches[4] |
| `ok` | success state | `--success` |
| `warn` | caution state | `--warning` |
| `error` | failure/destructive state | `--error` |
| `info` | informational state | `--info` |
| `selection_bg` | selected row/text ground | derived (readability-guarded accent tint) |
| `selection_fg` | text on selection | derived (= text) |
| `cursor` | soft/block cursor | derived (= accent) |
| `link` | hyperlinks (+ underline attr) | derived (= info) |
| `shadow` | cell-space drop shadow (alpha) | `--ui-shadow-1` (adapted, see 1.4) |
| `chart[0..8]` | categorical data ramp | derived (see 1.4) |
| `syntax_keyword/string/number/type/func/punct/comment` | code inks (cycle 6) | derived (see 1.4a) |

Text tier mapping is deliberate: theme.css has **three** text levels
(`primary`/`secondary`/`muted`) and so do we (`text`/`text_muted`/
`text_faint`). The rename marks intent: `text_faint` is decoration-grade and
never carries information — that is `text_muted`'s job. This is why
`text_faint` has a *lower* floor than the web kit implies (see 1.3).

Not ported (out of scope for an engine): entity-semantic tokens and
syntax-highlighting tokens (`--entity-*`, `--syntax-*`) are application
vocabulary; a future `syntax` extension can layer on the same registry
pattern. Web-only chrome (chips, pills, code-block washes) collapses into
`surface`/`surface_raised`/`border` — cell grids don't have nine grades of
translucent panel.

### 1.3 Contrast floors (test-pinned in `contrast.rs`)

| pair | floor | note |
| --- | --- | --- |
| text / bg, surface, surface_raised | **4.5:1** | 7:1 is the target; 4.5 is binding (WCAG AA) |
| text_muted / bg | **3.0:1** | must stay readable as real copy |
| text_faint / bg | **2.5:1** | decoration tier; never information |
| accent, accent_alt / bg | **3.0:1** | interactive marks (WCAG non-text) |
| ok, warn, error, info / bg | **3.0:1** | semantic marks |
| link / bg | **3.0:1** | renders as text + underline attr |
| selection_fg / selection_bg | **4.5:1** | selected text is still text |
| border / bg | **1.5:1** | hairline visibility; audited < 3.2:1 too (borders must not shout) |
| border_focus / bg | **2.0:1** | focus must beat plain border |
| cursor / bg | **3.0:1** | block cursor visibility |
| chart[i] / bg | **3.0:1** | every series legible on the ground |

**Current exception list (one entry):** `everforest-light` misses
`text/surface_raised` at ~4.25:1. Both hexes are verbatim theme.css ports
(everforest is deliberately soft), the rule is our own extension beyond the
mandated `text/bg` floor, and 4.25:1 still clears WCAG large-text AA — so
it is named in `AUDIT_EXCEPTIONS` with this justification rather than
"fixed" by editing upstream values. The staleness guard deletes it the day
upstream moves. *Cycle-2 re-audit:* the elimination path was re-examined —
both colors in the failing pair are verbatim ports, and every derived token
was re-checked as unrelated to the pair; the only fixes are editing
upstream values (breaks faithfulness) or weakening the rule (breaks the
audit). The exception stands, still the only one across 21 themes.

**Why 7:1 is a target, not a floor:** the family's identity palettes
(one-dark's `#abb2bf`, everforest's soft cream-on-sage) sit in the 4.5–7
band *by upstream design*. Forcing 7:1 would mean editing the palettes we
promised to port faithfully. The default `abstract-dark` clears ~12:1;
themes are user choices.

**Dark/light decisiveness:** `|L(bg) − 0.5| ≥ 0.15` — dark grounds must
measure L < 0.35, light grounds L > 0.65 (WCAG relative luminance). A
mid-gray ground makes both text polarities marginal and breaks dark/light
grouping for downstream consumers (image dithering, shadow strength, the
boot splash's vignette). The declared `dark` flag must agree with the
measured polarity — both are asserted per theme. (abstractcode used the
same idea with an `L < 0.40` threshold on a 4-token model; with full
surface stacks we also need the *margin*, not just the sign.)

### 1.4 Derivation rules (the exact rules, as implemented)

theme.css expresses several roles as **alpha washes** (`rgba(white, 0.12)`
borders, 12% accent tints). An alpha wash has no color until composited, so
the registry composites them over the theme's own grounds at build time —
same math the browser does, but frozen into opaque tokens the audit can
check. All mixing is sRGB lerp (`base::Rgba::lerp`); documented limit: sRGB
midpoints between hue-distant saturated colors drift toward gray, so
derivation only takes *short* hue trips (ground↔ink within one theme) and
long decorative gradients use curated stops (`boot::identity::BRAND_RAMP`).

- **border** = `mix(bg → text)` starting at t = 0.12 (the theme.css wash
  alpha), stepping +0.02 until ≥ 1.5:1 against bg. Using the theme's own
  text as ink keeps strokes hue-honest (gruvbox gets warm cream hairlines,
  not clinical white). Very dark grounds (abstract-dark, observer-night)
  land a step or two above 0.12; most themes keep the source alpha.
- **border_focus** = `accent`, full strength. Deliberate terminal
  adaptation: the web kit dims focus rings to a 55% wash because 1-px
  hairlines glare on high-DPI; glyph borders are already visually thin and
  the focused pane must win at a glance. (tmux/neovim convention agrees.)
- **selection_bg** = `mix(bg → accent)` starting at t = 0.30, walking
  *down* by 0.02 (floor t = 0.10) until `text` reads ≥ 4.5:1 on it;
  **selection_fg** = `text`. The web's 12% `--accent-subtle` is invisible
  at cell scale (no ::selection chrome, no border around the row); ~30% is
  the terminal equivalent, and the down-walk protects readability on light
  themes with mid-luminance accents.
- **cursor** = `accent`; **link** = `info`. Documented aliases (one role
  wearing its natural color). Underline comes from the style attr.
- **overlay** = black @ 0.45 alpha — exact port of the connect-modal scrim
  (the kit uses black over light themes too; it reads as "the app dimmed").
- **shadow** = black @ 0.35 (dark themes, the `--ui-shadow-1` color);
  text-ink @ 0.22 (light themes — the web uses ink @ 0.12 *with 30px of
  blur*; a hard cell shadow has no blur, so equal perceptual weight needs
  more alpha. Value chosen by eye; revisit on the real compositor).
- **accent_alt** = theme.ts `swatches[4]` (the family's curated companion
  accent), nudged toward `text` in 4% steps if it misses 3:1 (in the
  current family exactly one theme needs it: solarized-light's cyan at
  ~2.9:1). In several families the curated companion *is* `info` — a
  source fact, kept.
- **chart[0..8]** = `[accent, info, ok, warn, error, accent↔info mid,
  ok↔info mid ("teal"), error↔warn mid ("orange")]`, then a separation
  pass: any slot within 24/255 max-channel-delta of an earlier slot is
  nudged toward `text` (≤ 4 nudges). This is what keeps the ramp honest in
  families that curate `accent_alt == info` — a chart must never hand two
  series the same pen. Slot order is stable API: series 0 is always
  brand-colored, 1–4 follow semantic hues.

### 1.4a Syntax inks (cycle 6 — the theming of syntax highlighting)

Code sits on `surface_raised` (code fences, code views — the declared
code ground). The seven inks derive per theme from the audited family —
no new hex anywhere:

| ink | source | convention it honors |
| --- | --- | --- |
| keyword | `accent` | keywords are the skeleton — brand-colored |
| string | `ok` | strings-are-green |
| number | `warn` | amber literals |
| type | `accent_alt` | the companion accent |
| func | `info` | call sites |
| punct | `text_muted` | structure: quiet but legible |
| comment | `text_faint` | comments recede |

Each walks toward `text` (the derive.rs contrast walk) until it clears
its floor **against `surface_raised`**: primary inks 4.5:1, comment
3:1 — both CAPPED at what the theme's own `text` achieves there
(`registry::syntax_floor`). The cap is the honest ceiling: code can never
be more readable than body text, and soft palettes (everforest-light's
4.25:1 text-on-raised) must not force fake exceptions. A chart-style
separation pass de-clones near-identical inks (green-on-green families
like everforest keep keyword/string tellable-apart by stepping later
inks toward `text`). Audited per theme in `contrast::audit`; all 24
themes pass with zero exceptions.

Mapping from a lexer's `TokenKind` to these inks is widget policy, one
place: `widgets::code::code_token_color` — lexers never mint colors.

### 1.5 Runtime registration (RT1-9a, `register.rs`)

Apps and users add themes at runtime through `theme::register`, which runs
the FULL audit (contrast floors + role hygiene + decisive ground +
declared-vs-measured polarity) on every candidate:

- **`RegisterMode::Strict`** — any finding refuses the registration; the
  error carries the structured violation list (nothing stored, nothing
  leaked). For apps that treat theme files as code.
- **`RegisterMode::Labeled`** — the theme registers anyway; every finding
  comes back as a `#FALLBACK:`-prefixed warning the caller must surface.
  For user themes where refusing would strand the user.
- Identity problems refuse in BOTH modes: malformed ids, and ids that
  collide with a built-in theme or an upstream alias (a user theme
  silently replacing `nord` is spoofing, not customization).

Storage: accepted registrations are `Box::leak`ed to `&'static Theme`.
Justification: the damage contract §5 fixes the theme signal payload to
`&'static Theme`; leaking means a handle captured by any view can never
dangle across re-registrations; cost ~300 bytes per accepted registration,
and byte-identical re-registrations dedup to the existing handle (editor
loops leak per change, not per save). Re-registering an id replaces it for
future lookups; built-ins always win lookups.

### 1.6 Registry invariants (test-pinned in `registry.rs`)

- ids unique; upstream ids `"dark"`/`"light"` alias to
  `abstract-dark`/`abstract-light`.
- unknown id → default theme + a `#FALLBACK`-labeled warning string
  (`resolve()`); lookups never invent themes.
- role hygiene: no ground (`bg`/`surface`/`surface_raised`) may equal any
  ink (text tiers, accents, semantics, focus, cursor); text tiers pairwise
  distinct; selection and border must not vanish into the ground.
  Deliberate aliases (`cursor`=`accent`, `link`=`info`) are exempt and
  documented above.
- alpha discipline: exactly `overlay` and `shadow` blend; everything else
  opaque.
- 21 themes registered: the full AbstractUIC family.

---

## 2. Visual identity — the boot splash

Machine-readable constants: `src/boot/identity.rs` (timeline, easing
beziers, camera, ramp, wordmark, fallback mark). GFX3D implements the 3D
mark against those constants; DESIGN owns this storyboard and the 2D
fallback. Total 2000 ms, skippable at any moment (any key → 120 ms fade),
auto-skipped when stdout is not a TTY or `ABSTRACTTUI_NO_SPLASH` is set.

### 2.1 The mark

**Three ascending planes forming an "A".** Three thin parallelogram slabs
(width 1.0, height 0.62, thickness 0.04 scene units), stacked with a
z-offset of 0.22 and a rising stagger of 0.18, each pitched 12° — when the
camera settles near-frontal they optically fuse into a stylized "A": outer
plane = left stroke, middle plane = right stroke, smallest plane = crossbar.
The geometry is the pitch: AbstractTUI is a *layered compositor*, and its
monogram is literally layers aligning into a letter. (A wireframe tesseract
was considered and rejected: generic sci-fi, no letterform, no story.)

Materials: emissive gradient along each plane's length via
`brand_ramp(t)` — house red `#e94560` at the leading plane through violet
bridges to house blue `#60a5fa` at the trailing plane. Depth fog toward the
active theme's `bg` (the splash sits on the user's theme; only the mark
carries brand color). Vignette: radial `bg → BRAND_FIELD` at 12% opacity.

### 2.2 Storyboard (frame-accurate)

| t | frame |
| --- | --- |
| **0.0 s** | Theme `bg` fill + faint vignette. Planes off-screen bottom-right, yaw −35°, camera dolly 5.2. Nothing else — one quiet beat (≤ 2 frames) so the arrival reads as *arrival*. |
| **0.4 s** | Planes mid-flight (stagger 120 ms, each tween 780 ms, `EASE_ARRIVAL`), ~60% traveled, additive afterglow trails behind each plane (trail layer opacity ×0.72 per 100 ms). Camera yaw easing toward −6°. Skip hint fades in bottom-right (`text_faint`, from 300 ms). |
| **0.9 s** | Impact: planes overshoot the A-alignment by ~6% and settle back (`EASE_SETTLE`, y₁ = 1.56). Glow peaks; 12 cell-sized spark particles burst from the crossbar plane (450 ms lifetime, colors sampled from `brand_ramp`, additive, gravity-free drift outward). |
| **1.4 s** | Mark locked frontal (yaw −6°, dolly 4.4). Wordmark "AbstractTUI" begins: per-letter fade-in left→right (30 ms/letter), letter-spacing collapsing 4 → 1 cells (`EASE_TRACKING`). Accent underline sweeps left→right under the wordmark. Tagline "the terminal, composed" fades in at `text_muted` under it. |
| **2.0 s** | Composition complete and held since 1.85 s; splash cross-fades (`EASE_FADE`, 150 ms) into the application's first frame. On skip: current frame fades out in 120 ms, no re-layout jank — app renders beneath. |

### 2.3 Fallback 2D splash (no graphics protocol / dumb terminal)

Pure cell rendering, same timeline compressed to the same 2000 ms:

- Ground: vertical gradient `bg → surface` (per-row lerp, cheap).
- Mark: the 5-line pure-ASCII A (`MARK_ASCII`), revealed line-by-line
  bottom-up (0.0–0.9 s), each line fg-colored by `brand_ramp(row/4)`.
- Wordmark: same tracking collapse in whole cells (4 → 1), same per-letter
  fade (approximated by `text_faint → text_muted → text` stepping where
  truecolor is unavailable).
- 16-color terminals: brand ramp quantizes to red/magenta/blue; NO_COLOR
  or `TERM=dumb`: skip the splash entirely (a splash without color or
  timing is a delay, not an identity).

### 2.4 Skip affordance & liveness (RT1-10, implemented in `boot/player.rs`)

`press any key to skip` in `text_faint`, bottom-right, visible from 0.3 s.

- **Skip** = any deliberate input: key press/repeat, mouse press/wheel,
  paste. The event is consumed (never leaks into the app). Exit is a
  120 ms fade — cells lerp toward the theme ground (cheap post-process; at
  cell scale it reads as an opacity ramp) — and a SECOND deliberate input
  during the fade cuts instantly. Worst-case cost to an impatient user:
  two taps, < 150 ms.
- **Non-deliberate input never skips and is never lost**: capability-probe
  replies, focus events and unknown sequences arriving mid-splash (the
  splash starts on env-pass caps and never waits for the probe — RT1-6)
  are retained by the terminal adapter and handed to the app afterward,
  parser state included.
- **Pacing**: wall-clock `t`, frame DROP (a slow terminal skips ahead,
  never queues), one flush per frame, skip checked between every frame.
- **Hard cutoff**: 2.5 s wall ceiling, checked before every render, beats
  every other exit — a stalled terminal can never hold the app hostage.
- **Gate** (checked before entering the terminal): render-handle ttyness
  (the handle frames are written to — KERNEL opens `/dev/tty`, so
  `isatty(stdout)` is the wrong question), `ABSTRACTTUI_NO_SPLASH` set to
  anything but `"0"`, `NO_COLOR`, `TERM=dumb`.

---

## 3. Widget style guide (binding for `src/widgets/**`, both owners)

Filed early cycle 3 for REACT's interactive widgets (button/input/list/
scroll/tabs/table). Every rule below uses ONLY audited tokens — no widget
may mix, tint or invent colors (RT1-9b; the `widgets::lint_tests` grep is
the enforcement).

### 3.1 The three mechanisms (in priority order)

1. **The selection pair** — `selection_bg` + `selection_fg` — means "you
   are here / this acts on Enter". Keyboard focus on borderless widgets,
   pressed/active states, selected rows. Audited ≥ 4.5:1 on all 21 themes
   and protected by the joint fg/bg downlevel quantizer, so it survives
   256/16-color where underline-color downlevels away (RENDER request 6:
   focus is ALWAYS expressed in fg/bg, never underline-color alone).
2. **The focus stroke** — `border_focus` — focus on bordered containers
   (panes, blocks, framed inputs). Full accent, ≥ 2:1 on bg, quantizer-
   protected. Underline-color may garnish it where supported; never
   carry it.
3. **Ink shifts** — secondary cues: hover = `accent` ink on the hovered
   actionable; disabled & placeholders = `text_faint`; secondary copy =
   `text_muted`.

### 3.2 State table

| state | bordered widget (pane, framed input) | borderless widget (button, row, tab) |
| --- | --- | --- |
| normal | `border` stroke; content inks | content inks (`text` on `surface`/`surface_raised`) |
| hover | stroke unchanged; hovered actionable ink -> `accent` | ink -> `accent`; bg unchanged |
| focus | stroke -> `border_focus`; title ink -> `text` | bg/fg -> selection pair |
| press / active | stroke `border_focus` + actioned part selection pair | selection pair (+ BOLD where a rich canvas exists) |
| disabled | stroke stays `border`; content -> `text_faint`; NOT focusable | `text_faint`; NOT focusable |
| selected (persistent) | n/a (containers don't select) | selection pair even when unfocused; the OWNING pane's `border_focus` says where keys go |

Rules that make the table safe:

- **Hover is garnish.** Every interaction must work mouse-free; hover
  never carries information that focus doesn't also carry.
- **Disabled is decoration-grade on purpose** (`text_faint`, floor
  2.5:1 — the deliberate sub-AA tier). Disabled elements leave the focus
  order entirely; a focused-disabled state cannot exist.
- **Selected + focused compose**: the row keeps the selection pair; the
  container's stroke says which pane owns the keyboard. Never two
  selection pairs at different strengths — one pair, one meaning.
- **Placeholders** (`text_faint`) must disappear on first input, never
  co-render with user text.

### 3.3 Per-widget token map (shipped set)

| widget | tokens |
| --- | --- |
| block | `border`/`border_focus`, title `text_muted` -> `text` (focused), fill = caller's surface token |
| separator | `border` stroke, label `text_muted` |
| badge | tone token (`accent`/`ok`/`warn`/`error`/`info`/`text_muted`) on `surface_raised` |
| progress | fill `accent`, or ramp `ok` -> `warn` (0.65) -> `error` (0.85); track `surface_raised` |
| spinner | glyph `accent`, label `text_muted` |
| logo | "Abstract" `text` + "TUI" `accent`; tagline `text_muted` |

| button | §3.2 borderless row: `text` on `surface_raised`, hover `accent` ink, focus/press selection pair, disabled `text_faint` |
| input | framed: `border` -> `border_focus` strokes, `text` on `surface`, placeholder `text_faint`, caret `cursor`, selected text = selection pair |
| list / table | rows `text` on `surface`; selected row = selection pair (kept when unfocused — the owning pane's `border_focus` says where keys go); table header labels `text_muted` on `surface_raised` (sorted column may step to `text`) |
| tabs | active `text` + `border_focus` strip drawn as cells (never SGR underline — survives 16-color); inactive `text_muted` |
| scrollbar (list/scroll/table) | track `border`, thumb `text_muted`; no hover state required |
| chart | series = `chart[slot]` (slot in, color never); axes `border`, range labels `text_faint` |

## 4. Extension points (later cycles)

- `syntax` token block (code viewers) layered on the same seed/derive/audit
  pattern — theme.css already carries per-theme `--syntax-*` overrides to
  port when the widget exists.
- User themes: a parsed seed (config file) run through the same `build()`
  and `audit()`; out-of-floor user values get the `#FALLBACK` warning
  treatment, never silent correction.
- Hover/active state derivation (`+4%/−4%` ink mixes) once the ui layer
  defines interaction states.
