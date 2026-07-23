# field-gateway — findings from the gateway console build

Bug reports and footguns discovered while building the second-wave
validator app on AbstractTUI: `abstractgateway-console` (the
AbstractGateway configuration wizard TUI,
`../../../../../abstractgateway/console-tui` in the AbstractFramework
workspace; epic: `../../planned/ports/0215_gateway_config_wizard_app.md`).
This app exercises the form/wizard/table class the first consumer
(abstractcode-tui, a chat/composer app) barely touched — Select/Combobox
pickers, masked token input, multi-step validation gates, wide data
tables — so its findings are the field evidence for app-kits
0510/0520/0530.

House rules (same as `../first-app/`): every item is hit live during the
build, reproduced against the published `abstracttui` crate (0.2.8+),
and worked around app-side; each item records the workaround so the
engine fix can delete it. One file per finding
(`09NN_snake_case_title.md`), citing engine `file:line`, classed as
bug / footgun / API gap / capability gap / UX defect / feature.

Band: **0900–0990** (this track owns it; leave gaps for insertion).
Overflow: the band filled and continued into **1000–1050** (rule
recorded in `../field-core/README.md`; field-core starts at 1100).
Each item carries a severity in its Metadata and in the row here:
**P1** blocked the build / **P2** cost real time, workaround holds /
**P3** paper cut.

| ID | Title | Class | Severity |
| --- | --- | --- | --- |
| 0900 | Table — oversubscribed fixed columns silently starve the Flex column to zero | footgun | P2 |
| 0905 | Select/Combobox same-value re-commit is unobservable — "re-pick to retry" cannot be built | API gap | P2 |
| 0910 | Shortcuts on elements outside the focus path silently never fire | footgun | P2 |
| 0920 | Wizard/tab navigation needs an input-immune key lane (0520 evidence) | capability gap | P3 |
| 0930 | Widget `disabled` is build-time only — validation gating forces focus-dropping rebuilds (0510 evidence) | API gap | P2 |
| 0935 | Dirty-form tracking is hand-rolled per form (0510 evidence) | capability gap | P3 |
| 0940 | Modal::open builds content before the Modal exists — self-closing forms need an external-slot dance | API gap | P3 |
| 0945 | ChoicePrompt shares MODAL_Z with app modals — no stacking policy, no is-a-prompt-open introspection | footgun / API gap | P2 |
| 0950 | reactive::connection assumes a persistent transport — probe-shaped clients cannot adopt it | API-fit evidence | P3 |
| 0960 | Element::draw closures paint past their own rect — hand-rolled text rows bleed over borders | footgun | P2 |
| 0970 | Table never clamps a bound selection when rows shrink — stale selection goes silently dead | API gap | P2 |
| 0980 | Table consumes `s` (sort cycling) even when no sort handler is registered | footgun | P3 |
| 0990 | No engine pattern for routing one-shot write completions back to forms (0510 evidence) | capability gap | P3 |
| 1000 | Dead-keys WINDOW when a modal's only focusables mount after an async load (extends first-app 0230 with the async variant) | footgun | P1 |
| 1010 | PageHost: no per-tab locked/disabled affordance for gated (wizard) flows | capability gap | P3 |

(Ledger repair 2026-07-25: the 1000/1010 rows were missing here —
both files existed since the 0.2.9/0.2.12 console builds. Number
collision on record: 0900/0905/0910 also exist in `../field-agora/`
— that track's overflow entered this band before the overflow rule
was written down; track+number is the working key, renumbering is the
owners' call — the wave11/0990 precedent.)
