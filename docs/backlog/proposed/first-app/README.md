# first-app — findings from the first real application

Bug reports and footguns discovered while building the first shipped
application on AbstractTUI: `abstractcode-tui` (the AbstractGateway coding-
agent client, ../../../../abstractcode-tui in the AbstractFramework
workspace). Every item here was hit live during that build, reproduced
against the published `abstracttui` 0.1.0, and worked around app-side; each
item records the workaround so the engine fix can delete it.

| ID | Title | Class |
| --- | --- | --- |
| 0260 | Disclosure widget — graphical per-item fold/unfold (maintainer ask) | feature |
| 0280 | Feed custom blocks cannot host widgets — protocol images degrade to mosaic | capability gap |
| 0281 | Scroll never re-clamps a bound offset when content shrinks under it | API gap |
| 0282 | `FeedState::sync` source shape too narrow — fold-shaped stores cannot adopt (borrow-based `sync_with` ask) | API gap |
| 0283 | Capped preview blocks — width-aware `max_rows` + honest overflow marker on Text/Rich feed blocks (+ hang-indent, tight-rhythm notes) | capability gap |
| 0284 | TextArea/TextInput placeholder paints unclipped past the widget rect (both branches; surfaced by the 0291 adoption) | rendering defect |
| 0292 | Completion triggers fire on any mid-text token — no position policy (renumbered from 0300: band collision with control-plane) | API gap |
| 0294 | Anchored panel places short lists over the chrome below instead of flipping up (renumbered from 0310) | UX defect |

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

Completed 2026-07-23 (0.2.6 field wave):

| ID | Title | Class |
| --- | --- | --- |
| 0291 | Placeholder while focused-and-empty — `TextArea::placeholder_while_focused` + `TextInput` parity; autofocused composers paint their teaching beside the caret (default OFF, decision in-item) | API gap |
| 0299 | Public full-redraw verb — `app::request_full_redraw()` + opt-in `set_redraw_on_focus_gained`; per-channel image re-place folded into `resync_unknown_screen` (heals suspend-resume too) | API gap |

Table reconciliation (2026-07-23, by the consumer while filing 0281-0283):
the open table above had gone stale against the directory — 0250 (fixed
0.2.0), 0290 (0.2.2), 0297 (0.2.3) and 0298 all live in
`../../completed/first-app/`; their rows are removed from the open table.
The directory is the truth.
