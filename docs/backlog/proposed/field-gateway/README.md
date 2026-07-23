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
Each item carries a severity in its Metadata and in the row here:
**P1** blocked the build / **P2** cost real time, workaround holds /
**P3** paper cut.

| ID | Title | Class | Severity |
| --- | --- | --- | --- |

(No findings filed yet — the build has not started. This table is the
landing zone; the directory is the truth.)
