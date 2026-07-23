# 0905: Drawer needs vertical insets so docked chrome stays visible

- **Status:** proposed
- **Band:** field-agora (agora-tui field reports)
- **Engine:** abstracttui 0.2.12
- **Severity:** P3 (app has a policy workaround)

## What happened

agora-tui's right Drawer (message reader / files / help) spans the
full terminal height. The app's bottom chrome — the composer's prompt
row, the input, and the key legend — sits UNDER the open drawer: a
send outcome (`⚠ post failed: …`) lands beneath the overlay, clipped
mid-word (adversarial design review P2-6). The composer stays live
under the drawer by design (passive focus), so the act surface and
its feedback are both occluded.

## Ask

`Drawer::inset_bottom(n)` / `inset_top(n)` (cells) so an edge drawer
can leave the app's docked rows visible — same spirit as the existing
size/edge knobs, zero default change.

## Workaround (shipped)

`i` (compose) closes the panel, making composing and the drawer
mostly exclusive; statuses also render post-close. Costs the
reference-while-composing use case (type while reading a file), which
the inset would restore.
