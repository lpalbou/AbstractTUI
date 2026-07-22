# Theming

AbstractTUI widgets never name colors — they name **roles**. Every drawable
surface resolves a semantic token against the active theme, so an entire
application restyles from a single switch, and every built-in palette is
held to measured, test-enforced contrast floors.

This page covers the token model, the 26 built-in themes, runtime
switching, the contrast guarantees, registering your own themes, and the
styling conventions widget authors should follow. The complete hex value
of every token in every theme lives in the generated reference:
[`captures/themes-table.md`](captures/themes-table.md).

## The 36-token semantic model

A theme's palette is a `TokenSet`: 36 resolved `Rgba` values, one per
`TokenId`. The tokens are grouped by the job they do, not by hue:

**Grounds** — the layered backgrounds an app is built on.

- `bg` — the application field; the deepest layer, fills the terminal.
- `surface` — panel and card ground.
- `surface_raised` — raised chrome: popovers, menus, active tabs, chips,
  and the declared ground for code blocks.
- `overlay` — the modal scrim; deliberately carries alpha for the
  compositor to blend over whatever it covers.

**Text tiers** — three levels of copy, each with its own contrast floor.

- `text` — body copy.
- `text_muted` — secondary copy: labels, descriptions, timestamps.
- `text_faint` — the decoration tier: placeholders, disabled glyphs,
  watermark art. Deliberately below the accessible-text grade; never used
  for information-carrying text.

**Strokes**

- `border` — hairline strokes: pane separators, boxes, rules.
- `border_focus` — the focus-ring ink; must read stronger than `border`.

**Voice** — where the theme's personality lives.

- `accent` — the theme's identity color: primary actions, active states,
  brand marks. One accent per screen region works best.
- `accent_alt` — a curated companion accent (gradients, secondary
  emphasis).
- `link` — hyperlink ink (the underline comes from the style attribute,
  not the color).

**Semantic states**

- `ok`, `warn`, `error`, `info` — success, caution, failure, and
  informational marks.

**Selection pair**

- `selection_bg` / `selection_fg` — always used together, never mixed with
  other grounds. The pair means "this is the thing keys act on".

**Cursor and shadow**

- `cursor` — the caret/block-cursor ink when the engine draws its own.
- `shadow` — a dim multiplier for cell-space drop shadows (carries alpha).
- `shadow_ground` — `shadow` pre-composited over `bg` at theme build, so
  it is opaque. This is what `Block::shadow` paints: widgets never do
  color math themselves.

**Chart ramp**

- `chart[0..8]` — eight hue-separated series colors, all legible on `bg`.
  Chart series pick a **slot**, never a color: slots 0–4 follow the
  accent/info/ok/warn/error family and slots 5–7 are curated companions,
  with a separation pass that keeps every series tellable-apart even in
  palettes where two source colors coincide. `TokenSet::chart(i)` clamps
  out-of-range indexes to the last slot, so indexing from arbitrary data
  can never panic.

**Syntax family**

- `syntax_keyword`, `syntax_string`, `syntax_number`, `syntax_type`,
  `syntax_func`, `syntax_punct`, `syntax_comment` — code inks derived per
  theme from the audited accent/semantic family and contrast-guarded
  against `surface_raised` (the code ground). Comments deliberately recede
  at the 3:1 class; the other inks target 4.5:1.

By-id access exists for tooling (theme editors, debug overlays, config
files): `TokenId::ALL` (all 36, stable order), `tokens.get(id)`,
`tokens.set(id, rgba)`, `TokenId::from_name("accent")`, and
`tokens.iter()` for `(id, color)` pairs.

## The 26 built-in themes

`theme::themes()` returns the built-in registry; `theme::get(id)` looks a
theme up by id (also honoring the `"dark"`/`"light"` aliases for the house
pair); `theme::resolve(id)` falls back to the default for unknown ids and
returns a labeled warning string alongside; `theme::default_theme()` is
`abstract-dark`. `theme::list()` yields `(id, label, dark)` for every
visible theme, built-ins first, then runtime registrations — the picker
surface.

The family:

| family | themes |
| --- | --- |
| Abstract originals | `abstract-dark`, `abstract-light`, `abstract-aurora`, `abstract-paper`, `abstract-ember`, `abstract-midnight`, `abstract-dawn` |
| Observer | `observer-night` |
| Catppuccin | `catppuccin-mocha`, `catppuccin-macchiato`, `catppuccin-frappe`, `catppuccin-latte` |
| Rosé Pine | `rose-pine`, `rose-pine-moon`, `rose-pine-dawn` |
| Tokyo Night | `tokyo-night` |
| Nord | `nord` |
| One | `one-dark`, `one-light` |
| Dracula | `dracula` |
| Monokai | `monokai` |
| Gruvbox | `gruvbox` |
| Solarized | `solarized-dark`, `solarized-light` |
| Everforest | `everforest-dark`, `everforest-light` |

The ported families keep every hex value their upstream palette defines,
verbatim. Tokens the upstream source does not define (borders, selection
tints, focus rings, the chart ramp, the syntax family) are derived by
documented, contrast-guarded rules — for example, borders composite the
theme's own text ink over the ground so gruvbox gets warm cream strokes
rather than clinical gray.

Every token value of every theme, generated straight from the registry:
[`captures/themes-table.md`](captures/themes-table.md).

## Switching themes at runtime

There is exactly one app-level theme signal. Reads are reactive, writes
restyle the whole application:

```rust
use abstracttui::prelude::*;

// Inside a component: read reactively. Any dyn_view that reads the
// signal rebuilds with fresh tokens when the theme changes.
fn header(cx: Scope) -> View {
    let theme = use_theme(cx);
    dyn_view(LayoutStyle::line(1), move || {
        let t = theme.get(); // &'static Theme: t.tokens, t.is_dark()
        text(format!("{} ({})", t.label, if t.is_dark() { "dark" } else { "light" }))
    })
}

// Anywhere: switch. Returns false (and changes nothing) for unknown ids.
set_theme_by_id("nord");

// Or with a handle from the registry / a runtime registration:
set_theme(abstracttui::theme::get("catppuccin-mocha").unwrap());
```

Mounting an app installs a watcher on the signal that damages the whole
tree on switch, so even static text repaints, while regions that read the
signal inside `dyn_view` re-render fine-grained. `Theme::is_dark()` is the
supported way to make polarity-conditional choices (shadow strength, image
dithering, artwork variants).

The shipped examples honor `ABSTRACTTUI_THEME=<id>` as a startup
convention — `set_theme_by_id` at boot is all it takes to adopt the same
convention in your app.

## Contrast guarantees

Every registered theme must pass `theme::audit(id, &tokens)` — a WCAG
contrast audit that measures each documented pair with
`theme::contrast_ratio(a, b)` and returns structured `Violation`s (theme,
rule, token, measured value, required floor). The built-in family passes
with zero violations as a test invariant; the floors are public in
`theme::contrast::floors` so your tooling audits against the same numbers:

| pair | floor |
| --- | --- |
| `text` / grounds | 4.5:1 (7:1 is the target, reported not enforced) |
| `text_muted` / `bg` | 3.0:1 |
| `text_faint` / `bg` | 2.5:1 (the deliberate decoration tier) |
| `accent`, `accent_alt`, semantics, `link` / `bg` | 3.0:1 |
| `selection_fg` / `selection_bg` | 4.5:1 |
| `border` / `bg` | 1.5:1 |
| `border_focus` / `bg` | 2.0:1 |
| `cursor` / `bg` | 3.0:1 |
| syntax inks / `surface_raised` | 4.5:1 (comments 3.0:1) |

Syntax floors are additionally capped at what the theme's own body text
achieves on the code ground — code can never be more readable than text,
which matters for deliberately soft palettes.

Beyond the pairs, grounds must be **decisive**: a theme's measured ground
luminance must agree with its declared `dark` flag by a margin
(`|L(bg) − 0.5| ≥ 0.15`). A mid-gray ground makes both text polarities
marginal and breaks everything downstream that groups by polarity.

Audit exceptions are named per `(theme, rule)` pair, never blanket, and a
stale exception fails the test suite. Exactly one exists:
`everforest-light`'s text on raised chrome measures ~4.25:1 — both values
are verbatim upstream colors, the rule is stricter than the mandated
text/ground floor, and 4.25:1 still clears WCAG AA-large.

## Registering a custom theme

`theme::register(candidate, mode)` is the runtime door:

```rust
use abstracttui::theme::{register, RegisterMode, ThemeCandidate, TokenSet};

let candidate = ThemeCandidate {
    id: "my-theme".into(),        // kebab-case: [a-z0-9-_], non-empty
    label: "My Theme".into(),
    dark: true,                   // audited against measured luminance
    tokens: my_tokens,            // a full TokenSet
};

match register(candidate, RegisterMode::Strict) {
    Ok(reg) => set_theme(reg.theme),
    Err(e) => eprintln!("{e}"),   // structured violations, not a boolean
}
```

The audit always runs; the mode declares what happens to findings:

- **`RegisterMode::Strict`** — findings refuse the registration. The
  error carries the structured violation list plus role-hygiene findings
  (`RegisterError::Rejected { violations, hygiene }`), so a theme file can
  be treated as code: fix what the audit names.
- **`RegisterMode::Labeled`** — the theme registers anyway, and every
  finding comes back on `Registration::warnings` as a `#FALLBACK:`-prefixed
  line. Use this for user-supplied themes where refusing would strand the
  user — and surface the warnings, never swallow them.

Identity problems refuse in **both** modes: an empty or malformed id is
`RegisterError::InvalidId`, and shadowing a built-in id or one of its
aliases is `RegisterError::ReservedId` — a user theme silently replacing
`nord` would be spoofing, not customization.

Accepted registrations are `&'static` (leaked once, stable for the app's
life, ~300 bytes each), visible to `theme::get`, `theme::list`, and theme
cycling. Re-registering an id replaces it for future lookups while old
handles stay valid; re-registering a byte-identical candidate returns the
existing handle without allocating.

### Deriving tokens

You rarely design 36 colors by hand. `theme::derive` provides the same
helpers the built-in registry uses:

- `mix(a, b, t)`, `lighten(c, t)`, `darken(c, t)` — sRGB-space mixes
  (the perceptual limits are documented at the definitions; these are for
  small nudges within one theme, not long decorative gradients).
- `mix_until_contrast(base, ink, anchor, t0, step, floor)` — walk a mix
  upward until it clears a contrast floor against its ground (how borders
  are derived).
- `tint_until_readable(base, tint, fg, t0, step, t_min, floor)` — walk a
  tint downward until the foreground stays readable on it (how selection
  backgrounds are derived).

The intended recipe: start from your four anchor colors (ground, text,
accent, one semantic), derive surfaces with `lighten`/`darken` steps, fill
the rest with the walks, then run `theme::audit` and fix what it names.

## Design guidance for widget authors

Widgets built on AbstractTUI should speak tokens and nothing else — the
engine's own widget sources are lint-checked for raw hex. The conventions
that keep a screen coherent:

**Three focus/selection mechanisms, in priority order.**

1. The **selection pair** says "this is the thing keys act on"
   (list rows, table rows, selected text).
2. A **`border_focus` stroke** says "this pane owns the keyboard"
   (bordered widgets and panes).
3. **`accent` ink** is hover garnish.

Never render two selection pairs at different strengths — one pair, one
meaning.

**The state table.**

- *Normal*: content inks on their ground.
- *Hover*: recolors the actionable ink to `accent` — decoration only; a
  hover state must never carry information focus does not.
- *Focus*: `border_focus` stroke on bordered widgets; the selection pair
  on borderless ones.
- *Disabled*: `text_faint`, and out of the focus order entirely — a
  focused-disabled widget cannot exist.
- *Selected*: persists when the pane is unfocused; the owning pane's
  stroke says where keys go.

**Hard rules.**

- Tokens only; no color arithmetic in widgets — pre-composited tokens like
  `shadow_ground` exist precisely so widgets never blend.
- Placeholders (`text_faint`) disappear on first input.
- Underline-as-affordance is drawn as cells, never as a text attribute
  alone, so it survives 16-color terminals.
- Every widget draws inside its rect; long spans clip rather than leak.

For a live rendering of all of this, run the `widgets` and `gallery`
examples (`cargo run --example gallery`), and see
[`../examples/README.md`](../examples/README.md).
