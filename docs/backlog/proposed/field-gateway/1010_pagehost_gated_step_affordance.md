# 1010 — PageHost: no per-tab locked/disabled affordance for gated (wizard) flows

- **Status**: proposed (field report — abstractgateway-console 0.3.0, PageHost adoption)
- **Severity**: P3 visual honesty gap (the behavior is enforceable app-side; the LOOK is not)
- **Engine**: abstracttui 0.2.12
- **Context**: filed per the v2 upgrade prompt's §4/§7 ("if the greyed-future-step look matters, that is a real gap worth filing").

## What the migration kept and what it lost

The console's wizard mode is a GATED linear walk over the same six
pages browse mode jumps around freely. PageHost adopted cleanly for
both — controlled `active`, `.number_jump(false)` + `chords(&[], &[])`
disarm free navigation in wizard, and the app's gate logic keeps
writing the screen signal. Behaviorally nothing was lost.

Visually, one thing was: the old hand-rolled bar rendered FUTURE
wizard steps in `text_faint` while unconnected ("these are locked
until the gate passes"). PageHost has one active/idle style; a locked
step now looks exactly like a reachable one, and only pressing a digit
(refused with a reason) or Ctrl+N (gated) reveals the difference. The
affordance taught the gate before the user hit it; its absence makes
the wizard's linearity discoverable only by refusal.

## Ask

A per-tab presentation state, smallest possible surface:

```rust
.page_state(id, PageState::Locked)   // or Enabled(bool), or a
                                     // .locked(id, Fn() -> bool) getter
```

- Locked tabs render `text_faint` (the badge machinery shows the
  pattern: a reactive getter running TRACKED in the bar region would
  let lockedness follow the gate signal live).
- Engine enforcement is NOT asked for: clicking/jumping to a locked
  tab can stay app policy (our digit-refusal-with-a-reason already
  handles it, and §0's free/gated split stays honest). Presentation
  only — the bar telling the truth about reachability.

## Workaround shipped meanwhile

None — accepted the gap. The wizard's refusals carry reasons (toast +
footer), so the gate is discoverable, just not visible in advance.
