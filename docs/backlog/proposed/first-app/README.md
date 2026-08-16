# first-app — findings from the first real application

Bug reports and footguns discovered while building the first shipped
application on AbstractTUI: `abstractcode-tui` (the AbstractGateway coding-
agent client, ../../../../abstractcode-tui in the AbstractFramework
workspace). Every item here was hit live during that build, reproduced
against the published `abstracttui` 0.1.0, and worked around app-side; each
item records the workaround so the engine fix can delete it.

| ID | Title | Class |
| --- | --- | --- |
| 0272 | ChoicePrompt aux-key vocabulary — no non-option key surface, hint row closed to callers (split out of 0271 at its completion; the `f` cards↔JSON toggle ask) | API gap |
| 0274 | `app::notify()` — presenter-custody emitter for the detected OSC 9/99 notification channels (the notify leg of planned 0150, first consumer named; RENUMBERED from 0290 2026-07-25 — collided with completed 0290) | API gap |
| 0280 | Feed custom blocks cannot host widgets — protocol images degrade to mosaic | capability gap |
| 0289 | Typed uppercase inserts lowercase on kitty-spelling wires — `convert_event` drops the kitty `text` field; TextInput inserts the base char (found during the 0286/0288 verification) | bug |

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

Completed 2026-07-23 (wave-5 field fix):

| ID | Title | Class |
| --- | --- | --- |
| 0285 | Selection layer eats drag-less clicks — click-through shipped (layer claims only once the gesture DRAGS) + pointer-capture heal; buttons clickable with select mode on | UX defect |

Completed 2026-07-23 (choice-fix wave):

| ID | Title | Class |
| --- | --- | --- |
| 0286 | KeyChord shifted-letter two wire spellings — ONE fold (`KeyChord::normalized`) at every chord-match site (tree shortcuts, Actions, pressed_chord); both registration directions fire on both wires; the app's double registration can be deleted | footgun |
| 0287 | ChoicePrompt body slot — `.body(\|mcx\| view)` + `body_rows(n)`: scrollable/reactive display region between prompt and options; options allocated first (0240), wheel-over-body scrolls the body, keys stay with the options | API gap |
| 0288 | ChoicePrompt `option_key` uppercase dead on kitty — the same fold at the letter matcher (`KeyEvent::normalized`/`means_char`); `option_key(…, 'A')` fires on `CSI 97;2u`, lowercase-declared keys still refuse Shift+letter | bug |

Completed 2026-07-23 (choice-0271 wave):

| ID | Title | Class |
| --- | --- | --- |
| 0271 | ChoicePrompt approval-gate adoption gaps — `body_width(cols)` (body participates in the panel's measure), `dismiss_label(label)` (button + hint + advertised Esc follow the caller's vocabulary; outcome stays `Cancelled`), `handle.retire()` (host close without resolving — distinct from user-Esc). The aux-key gap (gap 2) was split out UNSHIPPED to 0272 above. | API gap |

Completed 2026-07-25 (attachments wave — filed directly in completed/,
the 0535 precedent):

| ID | Title | Class |
| --- | --- | --- |
| 0273 | File-attachment surfaces — `TextInput`/`TextArea` `on_paste` intercept (`PasteAction`), `input::paste::classify` cross-terminal drop classifier, `FilePicker` over the `FileSource` seam (+ `examples/attachments.rs`, api.md drop-reality table) | feature |

Completed 2026-07-24 (disclosure wave, jointly with field-agora 0850):

| ID | Title | Class |
| --- | --- | --- |
| 0260 | Disclosure widget — standalone fold/unfold card shipped (`▸`/`▾` title row + detail slot, click/Enter/Space toggle, `max_body_rows` cap + auto-hiding scrollbar, uncontrolled + `folded(Signal<bool>)` modes, folded = body unmounted) with the Feed enablers (`on_item_press`, `item_at_row`) and the documented message-card recipe. The feed-NATIVE card kind stays future work behind 0280's draw-only block boundary. | feature |

Completed 2026-08-16 (clipboard-truth wave — band continuation):

| ID | Title | Class |
| --- | --- | --- |
| 1310 | Codex-compatible navigation chords in both text widgets — Codex's `EditorKeymap` defaults adopted verbatim (word motion in all three spellings, line start/end incl. Ctrl+A/Ctrl+E, the delete-word family), pinned by a parity test. A parser-level meta-prefix decode was built and REVERTED as out of scope: Codex covers that terminal at the keymap level. Adversarial review also caught a phantom-selection bug and a no-op `on_change`. | feature |
| 1300 | A live selection freezes follow-tail — screen-space selection over a pinned `Scroll` copied whatever the stream had put on those cells by release (`widgets::scroll::freeze_follow_tail` + the `app::selection` publisher). Filed with the second half of the same report, which was a version fact: 0.2.20 had no host-clipboard fallback, so every copy died with OSC 52 unadvertised (fixed by 0.2.25/0.3.0 — abstractcode-tui bumped). | bug |

**Band continuation**: 0220–0299 is full, so this track continues at
**1300–1340** (the next-free-fifty rule, stated in
`../field-core/README.md`; 1250–1290 is app-kits' continuation).

Table reconciliation (2026-07-23, by the consumer while filing 0281-0283):
the open table above had gone stale against the directory — 0250 (fixed
0.2.0), 0290 (0.2.2), 0297 (0.2.3) and 0298 all live in
`../../completed/first-app/`; their rows are removed from the open table.
The directory is the truth.

Table reconciliation (2026-07-23, by the consumer while filing 0271):
0281-0284, 0292 and 0294 live in `../../completed/first-app/` (the
0.2.6-0.2.9 fix waves); their rows are removed from the open table
above. The directory is the truth.
