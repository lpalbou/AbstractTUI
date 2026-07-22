# App-kits backlog track — band 0500–0590

## Status
Proposed (roadmap study 2026-07-22, cycles 1–4 — final). Numbering band:
**0500–0590 only** — this track never writes outside it. Sibling study
bands: control-plane 0300–0390 (`../control-plane/`, PLATFORM),
extensions 0400–0490 (`../extensions/`, EXTENSIONS). Established bands:
live-data 0010–0090, app-widgets 0100–0190, ports 0200–0290
(`../../overview.md` "Bands"). The overview's counts/ledgers are NOT
updated by this study (three parallel authors would collide there);
folding this band into `overview.md` is a named follow-up for the
single-writer merge pass.

## Purpose
The 0001 roadmap maps the road from engine to foundation for five app
classes (dashboards, chat/feeds, editors/consoles, viewers, games). The
maintainer's brief for this study adds the breadth dimension: the same
engine must power **config/admin consoles** (sidebar nav, wide data
tables with state badges and per-row action buttons, account header,
context banners), **wizard/setup flows** (stepped forms, validation,
apply), **multi-agent chat/coordination UIs** (channel sidebar with
unread badges, filter tabs, attention banners, collapsible panel
rails), **file managers** (tree + list + preview), **monitoring
dashboards** (the shipped example is the seed), and **smart-note apps**
(outline tree, tag chips, triage lists).

Per roadmap principle 1, every item here is justified by what those
classes *share*, never by one app: the four reference UIs are evidence
and validators, not design targets. What they share, verified against
the shipped widget catalog (`src/widgets/mod.rs:19-67`), is exactly
what is missing: **choice controls** (no select/dropdown/combobox
exists), **form machinery** (deliberately absent in v1 —
`src/ui/compose.rs:100-134`), **stepped flows**, **rich table cells**
(Table cells are plain `String`s — `src/widgets/table.rs:73`),
**hierarchy** (no tree widget), and **app-shell chrome** (nav sidebar,
header, banners, split panes, panel rails — all hand-rolled today,
`examples/dashboard/main.rs:342-362`).

## Items
| ID | Title | Class-level need |
| --- | --- | --- |
| 0500 | Anchored-popup substrate (cross-track) + Select / Combobox / MultiSelect family | every config, form, filter, and picker surface; popup geometry for three bands |
| 0510 | Form kit: field rows, form state model, submit gating | admin consoles, wizards, composers, settings panes |
| 0520 | Wizard flow container (on the form kit) | setup/config flows, migrations, apply-with-summary |
| 0530 | Data-table upgrades: rich cells, activation, multi-select, row identity | admin consoles, monitors, file lists, triage tables |
| 0540 | Chip & count vocabulary: interactive chips, count badges, tag input | status columns, unread counts, tags, multi-select rendering |
| 0550 | Navigation kit: sidebar nav + filter tab strip with counts | admin nav, channel lists, triage filters, places rails |
| 0560 | App header + banner primitives | account chip/sign-out headers, attention + context banners |
| 0570 | Tree view (virtualized, keyed, lazy children) | file browse, outline/notes, grouped entities |
| 0580 | Workspace chrome: split panes + collapsible panel rail | master-detail (tree+list+preview), chat right rail, inspectors |
| 0590 | Reference-app validators: admin console + wizard + triage shell examples | apps validate, never design (roadmap principle 2) |

## Dependency shape
- **0500 is the trunk control**: 0510 field rows embed it; 0520/0530/
  0540 consume it (filter selects, cell editors, chip pickers). Its
  popup core (anchored overlay + option list) is also the future Menu.
- **0510 before 0520** (the wizard is a form-kit consumer, one page per
  step). 0510 composes 0500 but does not block on it (TextInput /
  Checkbox / RadioGroup rows work day one).
- **0540 before 0550/0560 polish**: sidebar unread badges, tab counts,
  and banner chips all speak 0540's vocabulary; 0530's badge cells too.
- **0570 and 0580 are independent** of the rest; 0580's panel rail
  cross-references first-app 0260 (Disclosure) — same fold gesture, and
  0260's non-goals explicitly leave the tree/rail generalization
  unowned.
- **0590 last**: each example lands only when its consumed items land;
  each item's completion bar requires ≥1 validator consuming it.
- Cross-band: everything here rides shipped engine surfaces (overlays,
  focus, signals, tokens). Sequencing with **0170** (API stability):
  public shapes in this band merge under the same budgeted-break rule
  the roadmap pins for 0100/0130 ("0170's rulings before public shapes
  merge"). The **anchored-popup substrate is owned here, in 0500's
  core** (cycle-2 cross-band resolution): consumers are 0500's own
  popups, the **0120 TextArea** completion dropdown (design once), 0530
  row-action menus, and the extensions band's 0430 tooltips/in-card
  dropdowns — extensions consume it as public API. **0250** activation
  semantics are RULED (PLATFORM's proposal,
  reviews/study/platform-on-appkits.md "The 0250 ruling") and adopted
  by 0530/0550/0570. Control-plane: kit state lives in app-owned
  signals with CONSTRAINED value types, which makes it REGISTRABLE via
  the 0340 declared-keys registry (signals are `dyn Any` cells — never
  "serializable by construction"; corrected cycle 2). The 0520 wizard
  is 0340's accepted first consumer. Extensions (0400s) graph/diagram
  panels are consumers of 0580's rail/panes, never duplicated here.

## Reading order
0500 → 0510 → 0520 → 0530 → 0540 → 0550 → 0560 → 0570 → 0580 → 0590.

## Governing ADRs
None — the repo has no ADR system yet (0170 proposes the first ones).
One future ADR is flagged inside 0510 (form state ownership: signals-in-
a-struct vs. a managed store) if the design debate outgrows the item.

## Scope
Reusable widgets, composition kits, and validation examples inside this
crate. Everything themable via the 36-token model (`docs/theming.md`),
zero-idle-cost preserving, and buildable from public surfaces (widgets
keep no engine privileges — `src/widgets/mod.rs:1-6`).

## Band rules (added cycle 2)
- **Semantic visibility**: every interactive affordance a kit widget
  renders — action hit-zones included — is represented in the
  accessibility snapshot (role/label/value at minimum). The snapshot is
  the machine-readable UI state the control-plane band exports
  (automation bus, wire protocol, MCP); a kit affordance invisible
  there is invisible to agents, harnesses, and screen-reader bridges.
  (PLATFORM cycle-2 F6; 0530 §8 is the first application.)
- **Secrets redact at the widget**: masked inputs substitute BOTH the
  draw and `access_value` — downstream exporters never re-filter.
  (PLATFORM cycle-2 F2; 0510 §5.)
- **Activation vocabulary**: `on_select` = selection changed;
  `on_activate` = committed; `on_change` = bound value committed;
  Space toggles wherever a toggle exists, Enter always activates;
  commit-on-move is opt-in, never default (the 0250 ruling).

## Non-goals
- The reference applications themselves (they live with their products;
  0590's examples are in-repo validators only).
- Networking/attach/serialization (control-plane band 0300–0390).
- Graph/diagram/mermaid rendering (extensions band 0400–0490).
- Markdown-table rendering inside chat messages: `render::md` parses no
  tables (`src/render/md.rs:14-17`) — widening the md subset belongs to
  the app-widgets band (0100–0190); a custom-DRAWN static table in a
  Feed block is this band's honest recipe until then (precisely scoped
  in 0530's non-goals — a draw closure cannot host the interactive
  widget).
