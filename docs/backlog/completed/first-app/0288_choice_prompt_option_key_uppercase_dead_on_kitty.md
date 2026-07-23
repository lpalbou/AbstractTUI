# ChoicePrompt `option_key` uppercase letters are dead keys on kitty-protocol terminals (0286's class, a separate match site)

## Metadata
- Created: 2026-07-23
- Status: Completed 2026-07-23 (choice-fix wave)

## ADR status
- Governing ADRs: None. Input matching inside one component; no wire
  impact.

## Context
0286 records the two-wire-spelling trap for KeyChord shortcut
registration (legacy: Shift+A → `Char('A')` + `Mods::NONE`; kitty:
`CSI 97;2u` → `Char('a')` + `Mods::SHIFT`, identity deliberately kept
lowercase — `alternate_keys_use_primary_identity`, input/kitty.rs).
ChoicePrompt has the SAME gap in its OWN matcher, which is NOT
chord-based — so whatever fold fixes 0286 at the chord-match sites
will not reach it unless this site is folded too.

## Current code reality (0.2.8)
`choice_prompt_interact.rs::root_key_handler`:

- The mods gate accepts plain keys plus `shifted_upper = mods == SHIFT
  && Char(c).is_uppercase()` — i.e. `Char('A')+SHIFT`.
- The kitty wire delivers `Char('a')+SHIFT` (lowercase identity —
  `convert_event`/`convert_key` are 1:1, no case normalization), so
  `shifted_upper` is false and the handler RETURNS before letter
  matching.
- Letter matching itself is deliberate case-sensitive equality
  (`*key == c` — "'a' and 'A' are distinct keys; the approval
  consumer's vocabulary"), which is right; the defect is only that the
  kitty spelling of the uppercase key never reaches it.
- The engine's own test (`choice_prompt_tests_c2.rs`
  "uppercase letter is its own key") drives `Key::Char('A')` with no
  mods — the legacy spelling only. The kitty lane is untested.

Consequence: `option_key("approve_all", …, 'A')` — the exact example in
the 0.2.8 announcement — never fires on kitty-protocol terminals
(kitty, foot, WezTerm, recent iTerm2 with the protocol on). This is
byte-for-byte the live P0 the first app already paid for on 2026-07-23
(the dead Shift+A in its approval modal, fixed by registering both
chord spellings). Adopting ChoicePrompt as shipped would REGRESS that
fix.

## Proposed direction (engine's call)
Inside `root_key_handler`, treat `Char(lowercase)+SHIFT` as the
uppercase letter for OPTION-KEY matching only (fold `c` to uppercase
when mods == SHIFT before the `letters` lookup, and let that spelling
through the mods gate). Movement/Space/digit handling stays
plain-keys-only. Alternatively, whatever normalization 0286 lands
should be applied here as a named second site — the point of this item
is that the two sites must not drift.

## App-side note
No workaround exists app-side for this one (the matcher is inside the
component); the first app simply did not adopt ChoicePrompt for
approvals (see 0287 for the body-slot reason).

## Completion report (2026-07-23)

**Verification verdict: CONFIRMED at source, at TWO levels.** The
filing's mechanism is exact — and the defect was deeper than the mods
gate alone: even if `Char('a')+SHIFT` had passed the gate, the letter
find (`letters.iter().find(|(key, _)| *key == c)`,
choice_prompt_interact.rs) is exact char equality, so `'a' ≠ 'A'`
would still have missed. Kitty parser confirmed
(`alternate_keys_use_primary_identity`, input/kitty.rs: `CSI 97;2u` →
`Char('a')` + SHIFT); `convert_key`/`convert_event` (app/events.rs)
confirmed 1:1, no case normalization.

**Failing-first evidence** (all through the REAL `Driver` with wire
bytes via `CaptureTerm::push_input`, tests/wave_choice_fix.rs):
`kitty_shift_a_fires_the_uppercase_option_key_and_commits` and
`kitty_shift_a_jump_toggles_in_multiple_mode` (pushing `\x1b[97;2u`)
FAILED on the pre-fix tree exactly as filed, while
`legacy_shift_a_fires_the_uppercase_option_key_and_commits`,
`lowercase_declared_key_refuses_shift_letter_on_both_wires`, and
`decide_example_gate1_keys_fire_on_both_wires` (the example's `o/k/d`
on legacy bytes AND kitty plain `CSI code u` spellings) passed —
pinning the behavior the fix must not trade away. Unit-level repro
`kitty_shift_spelling_folds_to_the_uppercase_option_key`
(choice_prompt_tests_c2.rs) failed pre-fix too.

**The fix is the shared normalization this filing asked for** — not a
second local fold. `ui/event.rs` gained ONE fold
(`fold_shifted_letter`: `Char(cased letter)` + SHIFT → single-char
uppercase, SHIFT dropped; other mods kept; non-letters/multi-char
uppercases/caseless scripts untouched) surfaced as
`KeyEvent::normalized()`, `KeyEvent::means_char(c)` (the "does this
event mean declared char C?" predicate), and `KeyChord::normalized()`.
`root_key_handler` normalizes the event once at the top of its Key arm
— the old `shifted_upper` special case is subsumed; the mods gate and
every downstream branch (letters, digits, Space, movement) read the
canonical spelling. Case stays meaningful: uppercase-declared `'A'`
fires on `Char('A')` AND `Char('a')+SHIFT`; lowercase-declared `'a'`
never fires on either Shift+A spelling (test-pinned both wires).

The same fold landed at 0286's chord sites the same wave (tree
shortcut resolution, `Actions`, `KeyState::pressed_chord`) — the
"two sites must not drift" ask is answered by both consuming ONE
implementation. `docs/api.md` documents the key-spelling guarantee in
the ChoicePrompt section.

**Gates**: whole-tree cargo test green (1284 lib + all integration
suites incl. 11 in wave_choice_fix.rs), clippy zero, fmt clean,
`cargo semver-checks` vs 0.2.8 additive-clean (196 checks pass — the
new methods are additions).
