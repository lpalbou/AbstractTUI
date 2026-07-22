# first-app — findings from the first real application

Bug reports and footguns discovered while building the first shipped
application on AbstractTUI: `abstractcode-tui` (the AbstractGateway coding-
agent client, ../../../../abstractcode-tui in the AbstractFramework
workspace). Every item here was hit live during that build, reproduced
against the published `abstracttui` 0.1.0, and worked around app-side; each
item records the workaround so the engine fix can delete it.

| ID | Title | Class |
| --- | --- | --- |
| 0250 | `List::on_select` fires on arrow movement — no activation concept | footgun/API |
| 0260 | Disclosure widget — graphical per-item fold/unfold (maintainer ask) | feature |
| 0280 | Feed custom blocks cannot host widgets — protocol images degrade to mosaic | capability gap |
| 0290 | Selection region lingers after the release-copy — `c`/Enter keep being swallowed | footgun/UX |
| 0292 | Completion triggers fire on any mid-text token — no position policy (renumbered from 0300: band collision with control-plane) | API gap |
| 0294 | Anchored panel places short lists over the chrome below instead of flipping up (renumbered from 0310) | UX defect |
| 0297 | Disposal safety engine-wide — the 0250 ruling stopped at List/Table; Button's post-callback write forces the consumer's one-tick retire deferral (FIELD §3.1, filed convergence cycle 2) | footgun/API |
| 0298 | Stale frame band above the live frame after a workflow-picker close (resize suspected) | rendering bug |

Completed (moved to `../../completed/first-app/`, 2026-07-21 wave cycle 1):

| ID | Title | Class |
| --- | --- | --- |
| 0220 | `autofocus` inside a dyn_view regeneration panics the reactive runtime | bug |
| 0230 | Modal content shortcuts are dead until focus enters the modal tree | bug |
| 0240 | Overflowing modal content silently shrinks fixed rows to zero | footgun/docs |

Completed 2026-07-22:

| ID | Title | Class |
| --- | --- | --- |
| 0270 | Text selection + copy from rendered screens (mouse capture blocks native selection) | feature |
| 0293 | Kitty enter-flags never follow the probe — flags now push at the probe upgrade; WezTerm claim evidence-gated (fix wave cycle 3) | capability bug |
| 0295 | Public runtime capabilities accessor — `use_caps`/`current_caps` shipped, converged with media-av 0685; Ctrl+J newline folded into TextArea (fix wave cycle 3) | API gap |
| 0296 | Select faces programmatic open — `SelectHandle` + `.handle(&h)` on all three faces (fix wave cycle 3) | API gap |
