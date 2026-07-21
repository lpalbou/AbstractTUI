# DESIGN → docs cycle handoff

Doc-ready material for the coredoc set. Sources of truth cited per
section; where prose below disagrees with code, the code wins and the
docs agent should ping DESIGN. Everything here is shipped and test-pinned
as of cycle 9.

---

## 1. Theming guide (for a "Theming" docs page)

### The token model

Widgets never name colors — they name ROLES. A `TokenSet` is 36 resolved
`Rgba` values (`src/theme/tokens.rs`, `TokenId::ALL`):

- **Grounds**: `bg` (app field), `surface` (panels), `surface_raised`
  (chips, tracks, code grounds), `overlay` (modal scrim).
- **Text tiers**: `text` (body), `text_muted` (labels), `text_faint`
  (placeholders/disabled — deliberately sub-AA decoration grade).
- **Strokes**: `border`, `border_focus` (the focus ring ink).
- **Voice**: `accent` (THE brand ink — one per screen region works
  best), `accent_alt` (companion), `link`.
- **Semantics**: `ok`, `warn`, `error`, `info`.
- **Selection pair**: `selection_bg`/`selection_fg` — always used
  together, never mixed with other grounds.
- **Cursor**: `cursor` (caret ink).
- **Shadow**: `shadow` (alpha multiplier) and `shadow_ground` (shadow
  pre-composited over `bg`, opaque — what `Block::shadow` paints;
  widgets never do color math themselves).
- **Charts**: `chart[0..8]` — series pick a SLOT, never a color.
- **Syntax**: `syntax_keyword/string/number/type/func/punct/comment` —
  derived per theme from the audited accent/semantic family,
  contrast-guarded against `surface_raised` (comments deliberately
  recede at the 3:1 class).

By-id access exists for tooling: `TokenId` (36 variants), `t.get(id)`,
`t.set(id, rgba)`, `TokenId::by_name("accent")`.

### The built-in themes (26)

`theme::themes()` returns the registry; `get(id)`, `resolve(id)` (falls
back to the default with a labeled `#FALLBACK` warning string),
`default_theme()` = `abstract-dark`. The family: the AbstractUIC ports
(abstract-dark/light, observer-night, catppuccin mocha/macchiato/frappe/
latte, rose-pine + moon + dawn, tokyo-night, nord, one-dark, one-light,
dracula, monokai, gruvbox, solarized dark/light, everforest dark/light)
plus the originals (abstract-aurora, paper, abstract-midnight,
abstract-dawn, and the abstract pair). Every hex from theme.css was
ported VERBATIM; only missing roles were derived, by documented rules
(`docs/design/theme-identity.md` §1.4). The generated reference table
with every token value: `docs/captures/themes-table.md`.

### Contrast floors (audited, test-pinned)

`theme::audit(id, &tokens)` measures WCAG contrast ratios
(`contrast_ratio(a, b)`) and returns structured violations. Floors
(`contrast.rs::floors`):

| pair | floor |
| --- | --- |
| text / bg | 4.5:1 (7:1 aspirational, reported not enforced) |
| text_muted / bg | 3.0:1 |
| text_faint / bg | 2.5:1 (the deliberate decoration tier) |
| accent, semantics, link / bg | 3.0:1 |
| selection_fg / selection_bg | 4.5:1 |
| border / bg | 1.5:1 |
| border_focus / bg | 2.0:1 |
| cursor / bg | 3.0:1 |
| syntax inks / surface_raised | 4.5:1 (comments 3.0:1) |

Plus polarity decisiveness: a theme's measured ground luminance must
agree with its declared `dark` flag by a margin (`DECISIVENESS_MARGIN`).
One named exception exists (`AUDIT_EXCEPTIONS`): everforest-light's
text/raised pair measures ~4.25:1 — both values are verbatim ports, the
rule is our own extension beyond the mandated floor, and 4.25:1 clears
WCAG AA-large. Exceptions are named per (theme, rule), never blanket.

### Registering a custom theme

`theme::register(candidate, mode)` is the runtime door
(`src/theme/register.rs`):

- `ThemeCandidate { id, label, dark, tokens }` — id must be kebab-case
  `[a-z0-9-_]`; built-in ids are reserved (shadowing `nord` is spoofing,
  not customization → `RegisterError::ReservedId`).
- `RegisterMode::Strict` — audit violations REFUSE registration and the
  caller gets the structured list (`RegisterError::Rejected {
  violations, hygiene }`), not a boolean.
- `RegisterMode::Labeled` — registers anyway; every finding comes back
  as a `#FALLBACK:` warning line on the `Registration`. Callers surface
  these.
- Registered themes are `&'static` (leaked once, stable for the app's
  life) and visible to `user_get`/`user_list` and theme cycling.

### Deriving tokens for a custom theme

`theme::derive` helpers (used by our own registry build): `mix(a, b, t)`,
`lighten`/`darken` (perceptual honesty documented at the definition —
lerp toward white/black), and the contrast-guarded walks
`mix_until_contrast(...)` / `tint_until_readable(...)` that nudge a
candidate ink until it clears a floor against its ground. The intended
recipe for users: start from your four anchor colors (bg, text, accent,
one semantic), derive surfaces with `lighten`/`darken` steps, then run
`audit` and let the walks fix what it names.

### Applying themes at runtime

One theme signal, whole-app restyle (damage-contract §5):
`use_theme(cx)` to read reactively, `set_theme_by_id("nord")` /
`set_theme(&Theme)` to switch — every `dyn_view` reading the signal
rebuilds with fresh tokens. `Theme::is_dark()` for polarity-conditional
choices. `ABSTRACTTUI_THEME=<id>` is the examples' env convention.

---

## 2. Widget style guide, distilled (for a "Widgets" docs page)

The binding contract is `docs/design/theme-identity.md` §3; the user
version in one breath:

**Three mechanisms, in priority order**: (1) selection pair for "this is
the thing keys act on", (2) `border_focus` stroke for "this pane owns
the keyboard", (3) `accent` ink for hover garnish. Never two selection
pairs at different strengths — one pair, one meaning.

**The state table** (§3.2): normal = content inks on their ground; hover
recolors the actionable ink to `accent` and carries NO information focus
doesn't; focus = `border_focus` stroke (bordered) or selection pair
(borderless); disabled = `text_faint` and OUT of the focus order
(focused-disabled cannot exist); selected persists when unfocused — the
owning pane's stroke says where keys go.

**Hard rules**: tokens only (a lint forbids raw hex in `src/widgets/`);
no color arithmetic in widgets (pre-composited tokens like
`shadow_ground` exist for exactly this); placeholders disappear on first
input; underline-as-affordance must be drawn as cells, never SGR-only
(survives 16-color); every widget draws inside its rect (long spans clip
— `richtext::print_span_clipped` is the shared helper).

**Per-widget token map**: §3.3 has the full table (block, separator,
badge, progress, spinner, logo, button, input, list/table, tabs,
scrollbar, chart) — lift it verbatim.

---

## 3. Examples catalog (for a "Examples" docs page)

`examples/README.md` is final and doc-ready: a status table, then per
example — prose, Keys, Needs, Looks-like. The one-line versions:

- `hello` — the <60-line ergonomics acceptance; one panel, one signal.
- `dashboard/` — the flagship ops screen: charts, log tail, sortable
  table, toasts, modal, spatial pane nav (Alt+arrows), reactive startup
  notices, `--caps`.
- `gallery` — the whole design system on one screen; the theme-switch
  acceptance and the marketing shot.
- `themes` — the gallery picker with live preview pane (miniature app
  mock in the SELECTED theme) + measured contrast ratios.
- `widgets` — every widget state, §3 rendered.
- `effects` — compositor layers: shaders, transforms, toasts.
- `images` — four mosaic families side by side + dither + protocol
  placement, degradation always named.
- `viewer3d` — orbit a GLB with measured fps and honest notices.
- `components` — the shareable-component reference (props/children/
  events), heavily commented.
- `grid` — track grid reflow, three recipes over the same children.
- `splash` — the 2-second identity, both sources, all gates.
- `capture` — the screenshot tool (below).

All exit 0 headless with a one-line notice; `--caps` on dashboard/
viewer3d/images prints the capability report anywhere.

---

## 4. The visual identity story (for a "Brand"/README section)

**The mark**: an abstract "A" formed by three ascending planes —
`boot/identity.rs` is the single source (geometry constants, timings,
easings, colors); GFX3D's renderer and the 2D fallback both read it
(the drift test pins the shared beats — neither can grow a private
timeline).

**The splash**: 2.0 s total (`SPLASH_TOTAL_MS`), four beats — arrival
(0–0.9 s, planes fly in staggered 120 ms apart on an ease-out-expo
curve), alignment (0.9 s: the planes lock into the A; a 12-spark burst
fires — `BURST_AT_MS`/`BURST_PARTICLES`, gravity-arced, 450 ms lives),
reveal (1.4 s: the wordmark tracks open from 4 cells of letter-spacing
to 1 — `WORDMARK_TRACKING`, ease-in-out-quint), hold (1.85 s: settle,
then done). The 2D fallback additionally kicks 5 sparks as each mark
line lands (`LAND_SPARKS`), drawn BENEATH the strokes so the letterform
never breaks. Skip: any key, 120 ms fade (`SKIP_FADE_MS`); hard 2.5 s
wall cutoff; gates = tty + `ABSTRACTTUI_NO_SPLASH` + `TERM=dumb` +
`NO_COLOR` (`boot::should_splash`).

**Brand constants**: accent `#e94560`, companion `#60a5fa`, deep field
`#0f3460`, 5-stop ramp (`BRAND_RAMP`/`brand_ramp(t)`), wordmark
"AbstractTUI", tagline "the terminal, composed".

**IDENTITY LOCKED** (cycle 9): timings, easings, ramp, geometry and both
render paths are final. Final pty runs this cycle: 3D and 2D complete at
2.0 s with wordmark + tagline + hint; captures of the burst beat
(t = 1.0) and settled reveal (t = 1.95) live in `docs/captures/`.
Post-freeze changes to `boot/identity.rs` need a DESIGN sign-off.

---

## 5. Captures (embed-ready visual material)

`docs/captures/` — regenerate with
`cargo build --examples && cargo run --example capture`:

- Per example: `<name>.txt` (plain screen render — embed as a fenced
  block, it IS the screenshot) and `<name>.styled.txt` (deterministic
  styled dump: text rows + style runs, for color-accurate reference).
- `dashboard-dark` / `dashboard-dawn` — the flagship in both polarities
  at 120x35, fixed clock 12:34:56.
- `splash-2d`/`splash-3d` (burst beat) and `*-reveal` (settled wordmark).
- `themes-table.md` — all 26 themes × 36 tokens, generated verbatim.
- `README.md` in the directory indexes everything and states the honest
  determinism caveat (fixed data, wall-clock tick wobble — regenerate
  deliberately, diff by eye).

Known limits for the docs to state plainly: pty shots need unix
`script(1)` (Windows gets themes/splash artifacts); viewer3d's shot
requires the workspace GLB assets and is skipped with a note otherwise.
