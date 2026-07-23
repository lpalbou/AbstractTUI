# 0274 — app::notify(): a presenter-custody emitter for the detected OSC 9/99 channels

- Status: proposed
- Born: 2026-07-25 (abstractcode-tui visibility wave, review P2-6)
- Renumber note (2026-07-25, backlog hygiene): filed as first-app/0290,
  colliding with `../../completed/first-app/0290_selection_copy_keys_linger…`
  — renumbered to 0274 per the 0299/0291 precedent (zero inbound
  references at the time of the move).
- Relation: this is the NOTIFY leg of planned app-widgets **0150**
  ("terminal verbs — notify/bell/title reachable from components";
  clipboard leg already shipped) with its first named consumer —
  execute them as one design (the presenter-custody emission path is
  shared).
- Owner ask: first consumer is abstractcode-tui's run-conclusion notification

## The gap

The caps probe already DETECTS desktop-notification channels — `osc9_notify`
(iTerm2 convention: iTerm2/WezTerm/ghostty) and `osc99_notify` (kitty) — but
no public API EMITS on them. The damage contract gives the presenter byte
custody (`external_write` only), so an application cannot write the OSC
sequence itself without violating the one-flush rule; the capability
knowledge is engine-side while the send half does not exist.

## The consumer story

abstractcode-tui is a thin client for long agent runs (9-minute runs are
routine). The run concluding while the operator is in another window/pane
announces nothing today — "did it finish?" is answered only by looking.
The visibility wave shipped the in-app halves (a `✓ done · elapsed · calls ·
tools · tokens` transcript marker + idle-strip `last run:` segment); the
out-of-band half is blocked on this seam.

## The ask

`abstracttui::app::notify(title: &str, body: &str)` (or a `Notify` request
posted like `request_full_redraw`):

- picks the channel from the live caps (`osc99_notify` > `osc9_notify` >
  fallback BEL, or no-op when nothing is detected — honest degradation);
- emits through the presenter's external_write custody (never an app-side
  stdout write);
- documents the truthfulness bound: delivery is the terminal's business
  (focus rules, notification permissions) — the API promises the ESCAPE
  SEQUENCE, not the popup.

## Notes

- BEL (`\x07`) as the last-rung fallback is worth including: every terminal
  rings or flashes something, and "quiet ping on conclusion" is the actual
  operator need.
- kitty's OSC 99 supports ids/updates; v1 needs none of that — fire-and-forget
  title+body is the whole consumer ask.
