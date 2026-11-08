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
- `FramePlayer`: dependency-free presentation of live `Signal<Arc<Bitmap>>` RGBA frames.
  The producer owns generation and timing; hidden views do not subscribe to frames.
- `Avatar`: opt-in convenience widget through Cargo feature `avatar`, using the
  independent AbstractAvatar catalog. The host supplies pose, amplitude and visibility.

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

For `@` mentions, give `ChatPanel::mentions(...)` the host's
`MentionCandidate` list and read `.on_send(|value| ...)`: `value.text` spells
each chip `@name` and `value.mentions` lists the picks in text order with their
`index`. Only a pick from the popup is a mention; never parse `@` out of the
text yourself. Name who answered a row with `ChatMessage::authored_by(ChatAuthor)`
mapped from the gateway turn's `author`. Hosts with their own composer use
`Completion::mentions` and `TextAreaState::submission()`.

Run `cargo run --example chat` for a complete local echo application with no
framework packages or network. Use `cargo run --example hello` for the basic shell.

## Logical source selection

Use `widgets::ContentSelection::new(cx)` and bind it with
`Feed::content_selection(selection.clone())` for Markdown, plain/rich text,
code blocks and streamed Markdown. Read `selection.value()` for application
controls; release retains the selection and performs no send or clipboard action.
`clear()` cancels it; `expand(ContentKind::Sentence)` changes the unit around the
original clicked anchor. Table hits carry header/row/column identity and code hits
carry language/lines. Preserve the source snapshot and use producer-owned geometry;
never guess Markdown source from composed glyphs. Unsupported custom rows keep
screen-copy fallback. See [the API](../../docs/api.md#feed--logical-content-selection).

## Sharp edges

- Nested `grow` regions divide available space: a brief introduction should not
  grow alongside the transcript.
- Set `.busy` from real backend state; a rendering indicator alone cannot protect
  application actions against duplicates.
- Default input inset is zero. Explicitly set it on composers that need spacing.
- Terminal image support depends on the terminal; use cell-based rendering when
  pixel protocols are unavailable. The library supplies presentation, not speech
  synthesis, microphone recognition or an assistant service.

See [API](https://raw.githubusercontent.com/lpalbou/AbstractTUI/main/docs/api.md), [getting started](https://raw.githubusercontent.com/lpalbou/AbstractTUI/main/docs/getting-started.md),
[architecture](https://raw.githubusercontent.com/lpalbou/AbstractTUI/main/docs/architecture.md) and [graphics](https://raw.githubusercontent.com/lpalbou/AbstractTUI/main/docs/graphics-and-3d.md).

## Activity avatar recipe

Enable `abstracttui`'s default-off Cargo feature `avatar` before importing the
Avatar convenience types. Base TUI applications can use `FramePlayer` with any
RGBA producer and carry no avatar generator in their compiled dependency graph.

Inside an application mount callback, allocate `pose = cx.signal(AvatarPose::Ready)`,
`level = cx.signal(0.0f32)`, `visible = cx.signal(true)` and `motion = cx.signal(false)`.
Build `Avatar::new(pose).style(AvatarStyle::Halo).level(level).active(visible)
.animated(motion).view(cx)`. Poses also include Thinking, Working, Listening,
Speaking, Waiting and Error; styles include Halo, Hover, Vortex, Eclipse, Eye and Rover. Bind active to real
host visibility and motion to user preferences. Enabled Ready breathes at 2 fps,
Waiting pulses at about 6 fps, active phases run at roughly 12 fps, and Error is static.
Animation stops its clock when hidden,
reduced-motion or disposed. Compose in a host Button for activation and collapse
below useful cell sizes. `cargo run --features avatar --example avatar` demonstrates every input.
Allocate about 28×15 cells for recognizable art or 48×25 for a detailed preview.
Colour terminals use shaded quadrants, with braille as a monochrome fallback
and half blocks for legacy environments. Use `.mode(MosaicMode::HalfBlock)` to
select a specific glyph family, or sextants only when the terminal font supports
them. The widget uses shared composited RGBA, antialiasing and theme-cached ramps;
it does not require or automatically place a terminal image protocol.
When composing inside Button, position Avatar absolutely at left/top zero with
full percentage bounds; do not add it beside Button's existing label in row flow.
Keep the button label at the bottom for visible keyboard focus. Hover's Listening
leans, Waiting pulses in a lower posture with sequential dots, and Working bobs
with deterministic thruster sparks. Halo Listening takes rings inward and Speaking
emits them outward; fixed audio input still gives distinct active motion.

For shape-only overlays, opt into Avatar::transparent_background(true), remove any
host rectangle fill, and apply Button::transparent_background(true) to a wrapping
picture button. Empty cells preserve underlying glyphs/styles; occupied cells
replace a whole terminal glyph, including antialiased edges. Keep the underlying
view in the paint tree for restoration after motion/hide. Opaque previews remain
the default; focus/press on the button retains a visible selection cue.

Ready pointer tracking is opt-in: create `AvatarPointer::new(cx)`, pass
`.follow_pointer(pointer)`, and forward actual `MouseKind::Move` positions through
`pointer.moved(pos)`. Forward actual presses through `pointer.pressed(pos)` without
consuming the host event. It expires after 2 s, ignores non-Ready/reduced motion and samples
at 12 fps without polling. Clear on host hide. Error faces show red X eyes. Geometry
and the 44-design catalog live in the independent AbstractAvatar repository,
usable through Rust/C/Wasm/Python. Never copy generation equations into a client.
See [the API](https://raw.githubusercontent.com/lpalbou/AbstractTUI/main/docs/api.md) and ADR-0006.
