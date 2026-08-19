# 1320 — A scrollbar you can see, and a copy that confirms itself

Status: completed 2026-08-19
Owner: engine (widgets/list scrollbar, app/driver clipboard)
Effort: S

## The field asks

abstractcode-tui, 2026-08-19, two reports from the same session:

1. "The scrollbar is too small."
2. "We still see the clipboard OSC 52 unavailable... we shouldn't! On the
   contrary, it should show when the copy has worked, maybe showing the
   number of characters as proof."

## Ask 1 — the thumb rounded to a dot

`draw_scrollbar` sized the thumb `((h * h) / total).clamp(1, h)`. That is
correct proportion and useless output at transcript scale: a 3000-row
buffer in a 30-row pane asks for 900/3000 = 0 cells, and the clamp turned
every long buffer into a single `┃`. One cell cannot be found at a
glance, cannot be read as a position, and cannot be followed while
scrolling — the bar stops being an instrument and becomes a speck.

Shipped: a floor of `MIN_THUMB = 3` cells that yields rather than
covering the track — `MIN_THUMB.min(h - 1).max(1)` — so a short bar keeps
at least one row of travel and content that FITS still fills the bar
(the honest "nothing to scroll" answer). One shared function, so every
scrollbar in the engine gains it: `Scroll`, `List`, `Table`,
`FilePicker`.

## Ask 2 — the notice reported the route, not the result

The clipboard path announced `OSC 52 unavailable — copied via platform
clipboard` on a copy that had SUCCEEDED. It named an internal routing
detail, in the register of a failure, and said nothing about the thing
the user wanted to know. Meanwhile a copy that worked through the
advertised route said nothing at all.

The asymmetry was backwards. A copy leaves the app: the clipboard is
outside it, so the user has no way to see whether anything landed, or
landed whole. What they need is a receipt.

Shipped: every copy with a working route posts `copied N characters to
the clipboard` (plus `(N lines)` when the selection spans rows — a
multi-row copy's most common failure is arriving squashed). The warning
is reserved for the one case a user can act on: no route worked. The
route itself is no longer mentioned; it is the engine's business.

## Found while releasing this

`v0.3.2` was tagged, its workflow went green, and the crate never
reached crates.io. The core publish step asked
`cargo info "abstracttui@${version}"` first and skipped the publish when
it exited zero — which it did on the runner for a version that was not on
the index. The job printed "already on crates.io — skipping publish" and
reported success.

Fixed by inverting the shape: always attempt `cargo publish`, and consult
the sparse index only to EXCUSE a failure. A guard that decides not to
publish can no longer be the only thing between a tag and the registry.
The family job already probed the index this way; core now matches it.

## Evidence

- `widgets::scroll::tests::scrollbar_thumb_never_shrinks_to_a_dot` —
  2000 rows in a 20-row pane still draws a findable thumb that keeps
  travel room.
- `widgets::scroll::tests::scrollbar_thumb_floor_yields_to_short_tracks`
  — a 3-row track keeps at least one row of travel.
- `adv_selection::a_successful_copy_reports_its_size` — the receipt
  names the character count, the multi-row form names the rows, and no
  degradation is announced for a copy that worked.
