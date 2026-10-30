---
name: abstracttui
description: Build terminal interfaces with the abstracttui Rust library. Use for reactive widgets, application shells, Markdown and chat panels, keyboard and mouse controls, or terminal rendering.
license: MIT
metadata:
  package-version: "unreleased"
---

# Use AbstractTUI

AbstractTUI is a Rust terminal UI library. It renders components and dispatches
input; your application owns transport, inference, permissions and persistence.
Chat widgets do not make network calls or load models.

## Workflow

1. Add `abstracttui` to your Cargo dependencies. Use `abstracttui::prelude::*`.
2. Create `App::new(Size::new(width, height))`, then `app.mount(move |cx| { ... })?`.
3. Allocate persistent state with `cx.signal(value)` outside reactive child rebuilds.
   Read signals inside `Element::dyn_child` to rebuild only their dependent region.
4. Build widgets with `.view(cx)`. A layout column or row owns sizing; grow only the
   content region that should consume remaining space.
5. Call `app.run()` in an interactive terminal. Use `term::have_tty()` when a command
   should print an explanation and exit in a noninteractive environment.
6. Keep blocking work off the UI thread. Post completion through a captured
   `reactive::wake_handle()`; never access UI signals directly from worker threads.

## Choose components

- `TextInput`: single-line editor; `TextArea`: multiline editor. `.inset(1, 1)`
  gives a cell of space inside the side strokes without moving the outer frame.
- `List`: selectable items with keyboard and mouse-wheel navigation.
- `MarkdownView`: document reader. Constrain its layout when it is an introduction
  above another growing region.
- `Feed`: measured, virtualized transcript blocks with follow-tail behavior.
- `ChatTranscript`: role-labelled message bubbles, Markdown and scrolling when
  the host provides its own composer.
- `ChatPanel`: transcript plus bound multiline composer and Send button.

## Host-owned chat

Inside the mount callback:

```rust
let messages = cx.signal(vec![ChatMessage::assistant("How can I help?")]);
let draft = cx.signal(String::new());
let panel = ChatPanel::new(draft, move || messages.get(), move |question| {
    messages.update(|m| m.push(ChatMessage::user(question)));
    draft.set(String::new()); // Clear only after the host accepts the message.
})
.assistant_label("Helper")
.placeholder("Ask a question…")
.view(cx);
```

The host appends the assistant reply when its own backend completes. For an async
request, bind `.busy(move || busy.get())`; this prevents duplicate sends while
allowing preparation of the next draft. Requests, cancellation, retry, history,
authentication and persistence remain host responsibilities. Keep distinct
conversations in distinct host state. `ChatMessage::author` and `.meta` accept
labels from the host; the widget does not invent timestamps.

Run `cargo run --example chat` for a complete local echo application with no
framework packages or network. Use `cargo run --example hello` for the basic shell.

## Sharp edges

- Nested `grow` regions divide available space: a brief introduction should not
  grow alongside the transcript.
- Set `.busy` from real backend state; a rendering indicator alone cannot protect
  application actions against duplicates.
- Default input inset is zero. Explicitly set it on composers that need spacing.
- Terminal image support depends on the terminal; use cell-based rendering when
  pixel protocols are unavailable. The library supplies presentation, not speech
  synthesis, microphone recognition or an assistant service.

See [API](../../docs/api.md), [getting started](../../docs/getting-started.md),
[architecture](../../docs/architecture.md) and [graphics](../../docs/graphics-and-3d.md).
