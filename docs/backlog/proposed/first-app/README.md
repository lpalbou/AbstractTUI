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

Completed (moved to `../../completed/first-app/`, 2026-07-21 wave cycle 1):

| ID | Title | Class |
| --- | --- | --- |
| 0220 | `autofocus` inside a dyn_view regeneration panics the reactive runtime | bug |
| 0230 | Modal content shortcuts are dead until focus enters the modal tree | bug |
| 0240 | Overflowing modal content silently shrinks fixed rows to zero | footgun/docs |
