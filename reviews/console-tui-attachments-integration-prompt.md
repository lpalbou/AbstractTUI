# Prompt: attachments in the console-tui sandbox chat (AbstractTUI 0.2.20)

Copy everything below the line into the console-tui session.

---

Your sandbox-test modal (`src/ui/review.rs:210` — the "Live test
(sandbox generate)" prompt chat) should accept FILE ATTACHMENTS the
way code-tui's composer now does: pick files explicitly, accept
drag&drop, upload at send, and ride the run as `context.attachments`.
**Everything you need shipped in abstracttui 0.2.20** (crates.io; the
same version that carries your select-popup P1 fix — one bump covers
both). code-tui proved this exact stack end-to-end the day it shipped
(commons #5455: picker + drop + upload + model answering planted facts
from the attached file), so you are integrating a proven contract.

**Dependency first**: `abstracttui = "0.2.20"` (Cargo.toml:23, from
0.2.12; MSRV stays 1.87 — refresh the comment at Cargo.toml:5). No
breaking changes across 0.2.12 → 0.2.20 (semver-checks clean at every
hop). Note your hand-rolled workarounds that can die in the same bump:
the P1 displaced-popup class is engine-fixed, and `Table` no longer
eats `s` without a sort handler.

## The three engine surfaces (exact signatures)

1. **Paste interception** — drops arrive as bracketed paste; hook the
   prompt field BEFORE insertion:

```rust
use abstracttui::widgets::PasteAction;
use abstracttui::input::paste;

TextInput::new()
    .value(prompt)
    .on_paste(move |s| match paste::classify(s) {
        // classify owns the cross-terminal drop spellings:
        // shell-escaped spaces, 'single'/"double" quotes, file:// URLs,
        // multi-file space-joined lists (iTerm2/kitty/WezTerm/
        // Terminal.app/Ghostty corpus-tested). Existence/kind gating
        // is YOURS (the engine never touches the filesystem here).
        Some(paths) => {
            let ok: Vec<_> = paths.into_iter()
                .filter(|p| std::path::Path::new(p).is_file())
                .collect();
            if ok.is_empty() { return PasteAction::Insert; }
            attachments.update(|a| a.extend(ok));
            PasteAction::Consume       // nothing inserted into the field
        }
        None => PasteAction::Insert,   // prose pastes byte-identical
    })
```

2. **Explicit picking** — `FilePicker` in a Modal (your house
   pattern), bound to a key in the sandbox modal (suggest `Ctrl+A`,
   footer-hinted):

```rust
use abstracttui::widgets::{FilePicker, StdFileSource};

FilePicker::new(StdFileSource::new())   // std::fs source; hidden files skipped
    .multi_select(true)
    .on_pick(move |paths| { attachments.update(|a| a.extend(paths)); close(); })
    .view(mcx)
```

   Full builder (start dir, filtering, cancel) in `docs/api.md`
   "FilePicker". Esc-close at the Modal host composes cleanly —
   code-tui's report: "no gaps hit".

3. **The chips row** — render `attachments: Signal<Vec<String>>` as a
   row under the prompt field (filename + size; a remove affordance —
   your `field()` idiom + Badge works; keep it one line with
   `max_rows`-style honesty if many).

## Upload at send (the gateway half — your own API)

At sandbox-run submit, for each attached path:
`POST /api/gateway/attachments/upload` (multipart: `file` +
`session_id`) → `{"$artifact": ..., "artifact_id", "filename",
"sha256"}` — then the run's `input_data` carries
`context.attachments = [refs]`. This is the assistant precedent and
exactly what code-tui shipped; your gateway serves it today. Size
honesty: gate large files client-side with a labeled refusal (do not
silently truncate).

## Tests (the code-tui pattern, 6 headless tests through the real engine)

Drive bracketed-paste BYTES through the harness (`CaptureTerm` +
`term.feed(b"\x1b[200~/tmp/report.pdf\x1b[201~")`): drop-attaches
(chip appears, field stays empty), prose-inserts (field gains text,
no chip), multi-file drop, quoted-path drop, picker pick via keys,
remove-chip. The engine's own corpus already pins the spellings — your
tests pin YOUR gate + chips + upload wiring.

Everything above verified against abstracttui 0.2.20 source on
2026-07-25. Engine docs: `docs/api.md` §"Paste interception & file
drops" + §"FilePicker"; working demo: `examples/attachments.rs`.
