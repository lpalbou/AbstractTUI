# AbstractTUI API Guide

A guided tour of the public API, module by module. This is not a reference —
the item-by-item rustdoc is the reference (`cargo doc --open`, or browse
[docs.rs](https://docs.rs/abstracttui)). The goal here is orientation: what
each module is for, the types you will actually touch, and the idioms the
engine expects. Snippets are lifted from the crate's compiled doctests
wherever possible, so they match the shipped code.

## The prelude

`use abstracttui::prelude::*;` is all an application needs for the common
path. The prelude is curated to the app-code surface only: engine and test
types (`UiTree`, `Driver`, `create_root`, canvases) stay behind explicit
imports. One deliberate absence: `render::Style` is not exported, because two
`Style` types one glob apart is a trap. Layout style is exported as
`LayoutStyle` (box geometry — direction, size, gap); paint style is spelled
`render::Style` in full, inside draw closures, where it belongs.

## reactive — signals, memos, effects

`Signal<T>` is tracked state, `Memo<T>` is derived state, and an effect is a
computation that re-runs when anything it read changes. Handles are `Copy`;
state is owned by the `Scope` that created it and dies when that scope is
disposed. `batch` coalesces writes so effects observe one consistent world;
`untrack` reads without subscribing. The model in one compiled example:

```rust
use abstracttui::reactive::{batch, create_root};
use std::{cell::RefCell, rc::Rc};

let log = Rc::new(RefCell::new(Vec::new()));
let (root, ()) = create_root(|cx| {
    let count = cx.signal(0);
    let doubled = cx.memo(move || count.get() * 2);
    let log2 = log.clone();
    cx.effect(move || log2.borrow_mut().push(doubled.get()));
    count.set(3);
    batch(|| {
        count.set(4);
        count.set(5); // coalesced: the effect sees only 10
    });
});
assert_eq!(*log.borrow(), vec![0, 6, 10]);
root.dispose();
```

(`create_root` is the standalone entry point; inside an app, `App::mount`
hands your component a ready `Scope`.) Two time-aware helpers round out the
module: `animate(cx, source, easing, duration)` returns a signal following
`source` through eased transitions (settled values cost zero frames), and
`after(delay, f)` runs a one-shot closure on the UI thread, costing zero
wakeups until due.

## reactive::connection — lifecycle + jittered reconnect

`connection(cx, backoff, dial)` owns what every networked app
hand-rolls around its transport: the state vocabulary, the retry
schedule, the armed retry timer, and cancellation. The engine does NO
network I/O — `dial` runs on the UI thread once per attempt, spawns
the app's transport work (`spawn_worker` plus the HTTP client, socket,
or subprocess of your choice — the transport stays the app's call),
and reports through the `Clone + Send` `ConnectionEvents` reporter:
`connected()`, `degraded(reason)`, `failed(reason)`, `closed()`
(clean, terminal). Reports apply on the UI thread in the next phase U;
reports from a SUPERSEDED attempt (a zombie worker racing the retry
that replaced it) or after close are inert and counted
(`stale_reports`, the `dead_sends` convention). Workers poll
`is_closed()`/`is_current()` as their stop conditions.

`conn.state()` is a `Signal<ConnState>` the UI renders like any other:
`Connecting`, `Connected`, `Degraded(reason)`, `Reconnecting {
attempt, next_in }` (render "retry #2 in 1.4s" from the fields),
`Closed` — a closed vocabulary by design (transport semantics must not
grow into it). `conn.close()` is the UI-side terminal close;
`conn.retry_now()` skips a pending wait; scope disposal closes, cancels
the armed timer, and drops the dial fn.

`Backoff` is the pure schedule: FULL jitter — uniform in `[0,
min(cap, base × 2^attempt)]` — with defaults base 500 ms, ×2, cap
30 s, `reset()` on success (the machine calls it on connect), and
`seeded(n)` for deterministic tests. Jitter is not optional
politeness: un-jittered fleets retry in lockstep after a server
restart (the thundering herd). While reconnecting the loop stays
parked — the one armed one-shot costs zero wakeups until due, and a
`Closed` connection costs nothing forever (test-pinned). See
[live-data.md § "Connection lifecycle"](live-data.md#connection-lifecycle)
for the state diagram and a worker-thread example.

## ui — elements, views, composition

`Element` is the view-tree builder: layout style, children, focusability,
event handlers, keyboard shortcuts, an optional draw closure, and an
optional intrinsic measure (`.measure(fn(Size) -> Size)`) so a draw
widget can answer `Auto` sizing like a text leaf instead of defaulting
to zero.
Components are plain functions `fn(Scope, Props) -> View` — no trait, no
registry. They run **once**; reactivity comes from `dyn_view(style, f)`,
which re-runs `f` when the signals it reads change and re-renders only that
region. Props structs carry data fields, `Callback<T>` fields for typed
events out, and `View` fields as slots for children:

```rust
use abstracttui::prelude::*;
use abstracttui::widgets::Button;

struct CardProps {
    title: String,
    on_close: Callback<()>, // typed event out
    children: View,         // slot
}

fn card(cx: Scope, props: CardProps) -> View {
    let close = props.on_close.clone();
    Element::new()
        .style(LayoutStyle::column())
        .child(
            Element::new()
                .style(LayoutStyle::row())
                .child(text(props.title))
                .child(Button::new("x").on_click(move || close.call(())).view(cx))
                .build(),
        )
        .child(props.children) // the slot mounts where the component says
        .build()
}
```

Events route capture → target → bubble with hit testing and focus
management; `KeyChord` shortcuts attach to any element. For app-scale state,
the endorsed pattern is a store struct of signals provided as context —
`cx.provide_context(store)` at the root, `cx.use_context()` anywhere below.
Signals are `Copy` handles, so cloning the store shares state: no prop
drilling, no reducer framework.

## layout — flex and grid

The layout solver is a flexbox subset over integer cells: `Direction`
row/column, `grow`/`shrink`/`basis`, `gap`, padding, margin, min/max,
percent and absolute positioning, plus wrapping (`wrap()`, `cross_gap`).
Rounding is largest-remainder, so children tile their container exactly.
`Display::Grid` adds track grids: columns and rows are `Track::Cells(n)`,
`Track::Percent(f)`, `Track::Auto` (content-sized), or `Track::Fr(w)`
(weighted leftover); children auto-place row-major and can span via
`col_span`/`row_span`. `Overflow` (`Visible`/`Clip`/`Scroll`) is the
clipping and wheel-routing vocabulary.

```rust
use abstracttui::prelude::*;

// Sidebar + growing content in a row.
let sidebar = LayoutStyle::default().width(Dimension::Cells(24));
let content = LayoutStyle::default().grow(1.0);

// A label/field form as a track grid.
let form = LayoutStyle::default().grid(
    vec![Track::Cells(12), Track::Fr(1.0)], // columns
    vec![Track::Auto, Track::Auto],         // rows
);
```

## widgets — the built-in library

Every widget is built from the same public `ui` + `layout` + `theme` surface
user code has — widgets hold no engine privileges. They consume design
tokens only, never raw colors; the canonical build is `.view(cx)` (theme
from context), with an `element` form for explicit tokens — stateless
widgets take just `&TokenSet`, no `Scope`. The catalog:

- **Block** — the bordered panel primitive: title, fill, focus ring, `BorderKind`.
- **Button** — clickable label; hover/pressed/focused/disabled visuals; Enter/Space or mouse fires `on_click`.
- **TextInput** — single-line editor: grapheme-cluster-atomic cursoring, selection, word jumps, `on_change`/`on_submit`; `.masked(true)` for secret fields (bullets on screen AND in the accessibility export).
- **TextArea** — multiline composer: soft wrap, vertical caret with goal column, grow-to-content between `rows(min, max)`, submit-vs-newline policy, history recall, block paste, and a caret-cell anchor for completion dropdowns (`TextAreaState` is the app wire).
- **List** — virtualized selectable list; variable-height items, sticky selection by key, `scroll_to`. Vocabulary: `on_select` = selection changed (fires on movement); `on_activate` = the user committed this row (Enter/Space/click-on-selected).
- **Feed** — virtualized, append-only, keyed rich items (markdown in the full doc vocabulary — tables, lazy in-flow images, task lists — plus plain text, code fences, custom draws): the chat/log/transcript surface. Appends are O(1); a streaming tail item re-typesets only its open region (a streamed table renders as a table live); 10k items draw one screenful.
- **Table** — fixed/percent/flex columns, styled header, virtualized rows, selection, sort-indicator hook (the app sorts).
- **Tabs** — tab bar over lazily mounted panels; only the active panel is mounted.
- **Scroll** — clipped viewport over oversized content, mounted once so state, focus, and hit testing survive scrolling. The content extent is measured by the layout solver (`content_size` is an optional override), and `follow_tail` binds the pinned-to-bottom idiom.
- **Checkbox** — `[x] label` bound to a `Signal<bool>`.
- **RadioGroup** — one-of-N bound to a `Signal<usize>`; one tab stop, Up/Down move the selection.
- **Progress** — bar with sub-cell precision; optional ok→warn→error ramp.
- **Spinner** — indeterminate activity glyph, pure over a caller-owned frame index.
- **Badge** — small tinted label for status chips, counts, tags (`Tone`).
- **Separator** — horizontal or vertical rule, optionally labeled.
- **Charts** — `Sparkline`, `LineChart`, `BarChart` on sub-cell grids, with
  optional relative time axes fed from a `TimeSeries` history ring (see the
  history-rings section below).
- **Grid** — container widget over `Display::Grid`; spans ride each child's own style.
- **Image** — bitmap display through the mosaic pipeline (`ImageFit`; `Bitmap` re-exported beside it). Measures as its native cell footprint, so it holds real space in `Auto`-sized rows/panels.
- **Viewport3D** — orbiting 3D view of a `three::Model`: `.orbit(yaw, pitch, zoom)`, `.animate(clip, t)`, `.on_orbit`/`.on_zoom` deltas; camera state lives app-side in signals.
- **MarkdownView / RichTextView / CodeView** — typeset markdown (doc vocabulary: GFM tables, lazy in-flow images, task lists, plus outline/anchor rows and find-with-highlights — see the reader-surface section below), wrapped styled spans, read-only highlighted code.
- **Meter / AudioScope** — live level rendering: dB meter with real ballistics (instant attack, timed decay, peak hold) and a rolling braille waveform — see the live-levels section below.
- **Logo** — the AbstractTUI wordmark for headers, about screens, empty states.

### Code and diffs — lexers and their theme mappings

`CodeView` tints through the pluggable `text::Highlighter` seam (byte
ranges + `TokenKind`; the built-in `CLikeLexer` is honest demo-grade),
and `widgets::code_token_color` is the ONE place token kinds become
theme inks. Diffs are line-oriented, not token-oriented, so they ride a
dedicated additive vocabulary: `text::DiffLexer` classifies each line
(`DiffKind`: added, removed, hunk header, file header, meta chrome,
context — `#[non_exhaustive]`, so downstream matches carry a `_` arm
rendering unknown kinds as body text), and `widgets::diff_token_color`
maps it onto the SEMANTIC inks — added `ok`, removed `error`, hunk
headers `info`, chrome `text_muted` — readable on the `surface_raised`
code ground in every built-in theme (measured, test-pinned).

Routing is by language label, best effort: `CodeView::lang("diff")`
(also `"patch"`/`"udiff"`; `"rust"`/`"c"` pick C-like presets; unknown
labels change nothing), and markdown/Feed code fences labeled
` ```diff ` route automatically — one shared recipe, so a fence and a
`CodeView` can never tint the same patch differently:

```rust
use abstracttui::widgets::CodeView;

fn patch_pane(patch: &str, t: &abstracttui::theme::TokenSet) -> abstracttui::ui::Element {
    CodeView::new(patch).lang("diff").element(t)
}
```

Classification is stateless per line (scroll-position-invariant by
design) and approximate by contract: a removed line whose content
begins `-- ` reads as a file header (the classic highlighter
resolution), and prose between hunks stays untinted.

### List — selection vs activation

Selection FOLLOWS MOVEMENT: arrows/Home/End/Page keys and clicks move
the highlight, and `on_select` is the selection-changed notification —
never wire commitment, navigation, or destruction to it. Activation is
the EXPLICIT "user chose this row" event: `on_activate` fires on Enter
(always), on Space (a List has no toggle meaning), and on a click on
the already-selected row; a click on an unselected row only selects,
and there is no double-click synthesis. Both callbacks run after the
List's own bookkeeping (selection write, ensure-visible), so an
`on_activate` may close the surrounding modal — disposing the List's
scope synchronously is safe. When `on_activate` is unbound, Enter and
Space pass through to your shortcuts unchanged:

```rust
use abstracttui::prelude::*;

fn theme_picker(cx: Scope, apply_and_close: impl FnMut(usize) + 'static) -> View {
    List::of(["dark", "light", "solarized"])
        .on_activate(apply_and_close) // Enter / Space / click-on-selected
        .view(cx) // browsing with arrows only moves the highlight
}
```

### TextInput — masked (secret) fields

`.masked(true)` renders one `•` per grapheme cluster (a ZWJ emoji
family is one bullet; each bullet occupies its cluster's width, so
scroll and cursor geometry match the unmasked field) and exports the
same bullets through `access_value` — the accessibility snapshot is
shipped off-process by automation consumers, so a masked field never
leaks plaintext through the semantic tree either. Editing, selection,
cursor math, and paste are untouched; the bound value signal holds the
real text. One deliberate exception: Alt+arrow word jumps treat the
whole masked value as a single word (start/end, like Home/End,
Shift-extension included) — true word boundaries would reveal the
secret's word count and word lengths through caret motion. For a
reveal toggle, rebuild the field with `masked(false)`
inside a `dyn_view_scoped` over your reveal signal.

### Feed — streaming transcripts

An app owns a cloneable `FeedState` handle and mutates it; the `Feed`
widget windows over it. Items are keyed identities (`push` with a known
key replaces); a streaming item rides `md::DocStreamSession`, so a
token append costs one open region, never the document. Markdown items
speak the full DOC vocabulary: an agent answer streaming a GFM table
renders as a TABLE live (the whole in-flight table is the open region
until its first non-pipe line seals it), task lists wear checkboxes,
`~~strikethrough~~` strikes, and `![alt](path)` images typeset from a
header-only probe (decode happens lazily when an image row first
draws — items measure and window without decoding). `total_rows()` is
the reactive content extent, and `clear()` rebuilds bounded windows:

```rust
use abstracttui::prelude::*;
use abstracttui::widgets::{Feed, FeedItem, FeedState};

fn transcript(cx: Scope) -> View {
    let feed = FeedState::new(cx);
    feed.push("q1", FeedItem::markdown("**you** — hello"));
    feed.push_stream("a1"); // a live answer…
    feed.stream_append("a1", "# Str"); // …fed token by token
    feed.stream_append("a1", "eaming");

    let follow = cx.signal(true); // render it: "following / scrolled"
    Scroll::new(Feed::new(&feed).view(cx))
        .follow_tail(follow)
        .view(cx)
}
```

### Feed — rich lines (multi-ink without a custom block)

`FeedItem::rich` / `rich_lines` / `.rich_block` carry the engine's
span model (`render::RichText`) into feed items: a severity-tinted log
line or a chat header is spans, not a `FeedBlock::Custom` draw closure
with hand-rolled wrapping. Rich blocks typeset through the same
span-preserving wrap and row walk as every other block (cell-exact
parity with `RichTextView`, test-pinned), so wrapping, windowing and
damage behave exactly like `Text`. Span styles are patches: `fg: None`
spans inherit the item's theme ink; explicit inks are resolved `Rgba`
and render verbatim (rebuild items to retint on theme switch). Rich
items are replace-on-update; token streaming stays `push_stream`:

```rust
use abstracttui::render::rich::{RichLine, Span};
use abstracttui::render::Style;
use abstracttui::widgets::FeedItem;

fn log_line(t: &abstracttui::theme::TokenSet, ts: &str, body: &str) -> FeedItem {
    FeedItem::rich_lines(vec![RichLine::from_spans(vec![
        Span::new("ERROR ", Style::new().fg(t.error)),
        Span::new(format!("{ts} "), Style::new().fg(t.text_muted)),
        Span::plain(body), // fg-less: wears the item ink per theme
    ])])
}
```

(The public `FeedBlock` enum stays exhaustive through 0.2.x, so the
rich kind rides `FeedItem` constructors; `FeedBlock::Rich` proper is
budgeted for 0.3.)

### Feed — syncing from a `Signal<Vec<T>>`

When the transcript's source of truth is a FOLD (a vector recomputed
by events) rather than an append-only stream, `FeedState::sync` owns
the diff: keys are identities, fingerprints detect change, and the
optional visibility closure is the one truth for filtering. Appends at
the tail take the O(1) push path, changed fingerprints update in
place, and anything violating push order — shrink, reorder, mid-list
insert or visibility flip — takes the rebuild path inside the engine.
A rebuild re-renders every visible item, so a source that reorders on
every drain rebuilds on every drain — for feeds ordered by mutable
rank, sync a stable order and sort at render time, or accept
O(visible) per change. Float fingerprints must compare by bits
(`f32::to_bits`) — NaN never equals itself and re-renders the item
every drain.
A synced feed has ONE writer (the bridge); foreign writes are not
silent, though: the bridge detects them (a mutation counter) and
self-heals at the next drain with a full rebuild — stray items
evicted, order restored to source order. Render/key closures run on
change, never per frame:

```rust
use abstracttui::widgets::{FeedItem, FeedState, SyncSpec};

struct Msg { id: String, rev: u64, hidden: bool, text: String }

fn wire(cx: abstracttui::reactive::Scope, feed: &FeedState,
        items: abstracttui::reactive::Signal<Vec<Msg>>) {
    feed.sync(cx, items, SyncSpec::new(
        |m: &Msg| m.id.clone(),           // identity
        |m| m.rev,                         // cheap change fingerprint
        |m| FeedItem::markdown(&m.text),   // pixels, built on change
    ).visible(|m| !m.hidden));
}
```

### Feed — selection by key

`Feed::selected_key(sig)` binds a `Signal<Option<String>>`: the
selected item's row band grounds in the theme's `selection_bg` while
item inks stay (a transcript keeps its severity/syntax colors).
Selection is app-driven state — the app writes the signal and can pair
it with `FeedState::row_of(key)` (the item's first content row) to
drive a wrapping `Scroll`'s offset to the selected item. Unknown keys
highlight nothing.

### Charts — history rings and time axes

Monitors stop hand-rolling sample rings: `TimeSeriesState` (reactive)
or `TimeSeries` (plain) take `push(t, value)` — `t` is a `Duration` on
the app's clock, never a wall-clock read — quantize time into cadence
slots, and retain a bounded window (drop-by-age `new(cadence, window)`
or drop-by-count `with_slots`). Missed slots pad with `NAN`, so a
sampling pause draws as a HOLE through the charts' existing gap
contract instead of compressing the x-axis. `LineChart::time_axis(span)`
embeds relative labels in the axis rule row — "now" anchored at the
plot's right edge, nice ticks leftward, density adapting to width —
and `Sparkline::time_axis(span)` adds an optional label row. Feed the
span from the ring so warmup labels the REAL covered time:

```rust
use std::time::Duration;
use abstracttui::prelude::*;
use abstracttui::widgets::{LineChart, TimeSeriesState};

const TICK: Duration = Duration::from_millis(250);

fn traffic(cx: Scope, t: &TokenSet, sample: impl Fn() -> f32 + 'static) -> View {
    let rx = TimeSeriesState::new(cx, TICK, TICK * 72); // 18s window
    {
        let rx = rx.clone();
        let mut n = 0u32;
        interval(cx, TICK, move || {
            n += 1;
            rx.push(TICK * n, sample());
        });
    }
    let tokens = *t;
    dyn_view(LayoutStyle::default().grow(1.0), move || {
        LineChart::new(vec![rx.samples()]) // tracked: re-renders per push
            .range(0.0, 100.0)
            .time_axis(rx.span())
            .element(&tokens)
            .build()
    })
}
```

### Meter and AudioScope — live levels

`Meter` renders level data with real ballistics: instant attack, timed
decay (default 20 dB/s over the meter span, frame-clocked and
frame-rate-independent — a stalled stream shows a falling bar, not a
frozen one), and a peak-hold marker (~1.5 s, then it falls to the
level). One channel or N bands, eighth-block sub-cell fill, zone colors
from the `ok`/`warn`/`error` theme tokens:

```rust,ignore
let level = cx.signal(0.0f32);              // fed by the recorder lane
Meter::new(level).db_floor(-60.0).view(cx); // horizontal dB channel
Meter::bands(band_frames).bar(3, 1).view(cx); // vertical spectrum bars
```

**The idle law (pinned by tests):** a silent meter decays to its
fixpoint and STOPS requesting frames — unchanged input over any number
of turns costs zero frames and zero allocations. Only real motion bills
the frame loop.

`AudioScope` draws a rolling waveform from a `Signal<Vec<f32>>` window
on the braille chart substrate. Pair it with `bounded_source` and
`OverflowPolicy::DropOldest`: the source's retained window IS the
scope's ring (with honest drop accounting riding along). The scope owns
no clock — when the data stops, the last frame stays and nothing
re-renders.

`examples/voice_mock.rs` composes all of it — push-to-talk, meters,
scope, a fake transcription feed — with no audio and no network (the
capture gesture itself is `app::PushToTalk`, described with the app
runtime below).

### Scroll follow-tail

`follow_tail(Signal<bool>)` packages the log/transcript idiom: while
true the offset tracks the content bottom across appends and resizes;
any user scroll above the bottom sets it false; reaching the bottom
edge re-arms it. The signal is app-visible both ways — set it true for
a "jump to latest" key. Without `content_size` the extent comes from
the layout solver's measurement of the mounted content:

```rust
use abstracttui::prelude::*;

fn log_pane(cx: Scope, content: View) -> View {
    let pinned = cx.signal(true);
    Scroll::new(content) // extent measured — no height bookkeeping
        .follow_tail(pinned) // pinned until the user scrolls up
        .view(cx)
}
```

### Modal content that can overflow

Put the overflow inside a `Scroll` and keep the fixed rows fixed — the
defaults now do the bookkeeping: `Scroll`'s default layout is
`grow(1.0).basis(Cells(0))` (it absorbs overflow instead of demanding
its content size), one-row controls default `shrink(0.0)` (an
overflowing sibling can never crush them to zero rows), and
`Modal::open` floors declared fixed sizes. Opt out per row with an
explicit `min_h(0)`; debug builds log any fixed-size child that still
collapses:

```rust
use abstracttui::prelude::*;
use abstracttui::widgets::Button;

fn approval(cx: Scope, details: View) -> View {
    Element::new()
        .style(LayoutStyle::column().gap(1))
        .child(text("Approve this tool call?")) // fixed row: stays
        .child(Scroll::new(details).view(cx))   // absorbs the overflow
        .child(Button::new("Approve").view(cx)) // never crushed to 0
        .build()
}
```

### TextArea — the multiline composer

The chat/console input surface. `TextAreaState` (the FeedState pattern)
owns the durable wire: the value signal, the caret byte, focus, the
history store, programmatic edits, and `caret_cell()` — the caret's
solved screen cell, which anchors completion dropdowns. The widget soft
wraps at its width, grows with content inside `rows(min, max)` and then
scrolls internally; Enter submits while Alt+Enter, Ctrl+J (the universal
chord — `0x0a` IS Ctrl+J on the legacy wire, so it works on every
terminal) and Shift+Enter where the kitty protocol reports it insert a
newline — flip it with `SubmitPolicy::EnterInserts`. Up/Down navigate the buffer first and
reach for history only at the edges; the in-progress draft survives a
recall round trip. Pastes insert whole, newlines included — never a
submit:

```rust
use abstracttui::prelude::*;

fn composer(cx: Scope) -> View {
    let state = TextAreaState::new(cx);
    let st = state.clone();
    TextArea::new()
        .state(&state)
        .placeholder("Message — Enter sends, Alt+Enter newline")
        .rows(1, 4)
        .on_submit(move |msg| {
            st.push_history(msg); // Up recalls it later
            st.clear();
        })
        .view(cx)
}
```

**Placeholder while focused** (first-app/0291): by default the
placeholder paints only while the field is empty AND unfocused — the
classic yield-to-the-caret rule — which means an `.autofocus()`ed
composer (focused from boot) never shows its hint at all. Opt in with
`.placeholder_while_focused(true)`: the hint then also paints while
focused-and-empty, one cell past the caret in the same `text_faint`
ink, so the caret block stays visible beside it — the convention
modern editors ship. The default stays off so existing apps render
byte-identically; `TextInput` has the same option.

### Completion dropdown (anchored panel)

`app::anchored` ships the passive half of the anchored-popup substrate
(backlog 0500) and the completion controller riding it (backlog 0120):
`place_panel` places below-preferred, flips above when cramped, and
clamps into the viewport; `AnchoredPanel` mounts the result as a
NON-modal overlay above everything live (`Overlays::top_z() + 1`) that
never takes focus — keys stay with the composer — and closes with its
opener's scope. `Completion` registers trigger-character providers and
wraps the composer view; while the dropdown is open, Down/Up move the
highlight, Enter/Tab accept (the candidate's `insert` replaces the
whole token), Esc dismisses, further typing refilters, and clicking a
row accepts it:

```rust
use abstracttui::app::anchored::{Completion, CompletionCandidate};
use abstracttui::prelude::*;

fn composer_with_commands(cx: Scope, app: &App) -> View {
    let state = TextAreaState::new(cx);
    let composer = TextArea::new().state(&state).rows(1, 4).view(cx);
    Completion::new()
        .trigger('/', |query| {
            ["help", "quit"]
                .iter()
                .filter(|c| c.starts_with(query))
                .map(|c| CompletionCandidate::new(format!("/{c}"), format!("/{c} ")))
                .collect()
        })
        .attach(cx, &app.overlays(), &state, composer)
}
```

Providers run synchronously with the query typed after the trigger;
an empty Vec closes the dropdown. The OWNED mode (`Popup`, a modal
tree above the whole live stack with `DismissReason`-labeled endings:
commit, Escape, outside press, anchor scope death, and viewport
resize — a resize stales both the solved placement and the captured
anchor, so an open popup closes rather than float at stale
coordinates) and the TOOLTIP mode (`Tooltip::attach`, a hover-timed
passive label) ship beside it on the same placement engine — the
select family below rides the owned mode.

### Select / Combobox / MultiSelect — the choice controls

One family over one popup substrate, three faces (`app::select`,
re-exported in the prelude). All three render as a one-row focusable
trigger (side strokes carry focus, `▾` affordance, `text_faint`
placeholder); Enter/Space or a click opens an anchored popup that
layers above EVERYTHING live — a select inside stacked modals works —
and is placed below the trigger, flipped above when cramped. Inside,
Up/Down/PageUp/PageDown move a HIGHLIGHT (never the bound value),
Enter commits, Esc abandons, and an outside press dismisses without
acting on what is below. `on_change` fires on COMMIT only, and only
when the value actually changed; `Select::commit_on_move(true)` is the
opt-in live-preview exception (Escape then restores the pre-open
value). Options carry a stable `key`, a `label`, an optional muted
right-aligned `hint`, and `disabled` (skipped by movement, out of the
focus order). The closed control reports `Role::Button` (a select
trigger is a button that opens a menu; a dedicated `Select` role is
parked in the 0.3 breaking budget) with the current choice as its
access value; popups report `Menu`/`MenuItem`.

- **`Select`** — closed one-of-N bound to a `Signal<usize>`;
  type-ahead inside the popup jumps by label prefix, a repeated char
  cycles.
- **`Combobox`** — the popup includes the trigger row and mounts a
  real `TextInput` there (zero visual jump); typing filters
  (case-insensitive substring), the filter text is never the value, a
  non-matching buffer commits nothing, and a count/"no matches" line
  is part of the popup.
- **`MultiSelect`** — checkbox-marked rows; Space (or click) toggles
  a working copy without closing, Enter commits the whole set into a
  `Signal<Vec<String>>` of keys (canonical option order), Esc abandons
  it. The collapsed row joins the chosen labels and degrades to
  "N selected" when they overflow.

```rust
use abstracttui::prelude::*;
use abstracttui::theme::themes;

fn theme_picker(cx: Scope) -> View {
    let picked = cx.signal(usize::MAX); // nothing chosen yet
    Combobox::new(
        themes().iter().map(|t| SelectOption::new(t.label)).collect(),
    )
    .value(picked)
    .placeholder("type to search themes…")
    .on_change(|i| {
        set_theme_by_id(themes()[i].id);
    })
    .view(cx)
}
```

Inside an `App` the popup finds the overlay store through reactive
context automatically; outside one (bare-tree tests), pass
`.overlays(&overlays)` explicitly. The faces live app-side (they need
the overlay store; `widgets` sits below `app` in the layer map), but
they are plain token-consuming components with the standard
`.view(cx)` / `.element(cx, &tokens)` builds.

**Programmatic open — `SelectHandle`.** Command-summoned pickers
(`/theme`, `/model` typed into a composer) open a face without a
trigger gesture: build a cloneable `SelectHandle`, attach it with
`.handle(&h)` on any of the three faces, and call `h.open()` from a
command handler or shortcut — it returns `true` when the popup is open
after the call. The popup anchors at the trigger's LAST-PAINTED rect,
so a face that has never rendered refuses (`false`) — open on the
frame after mounting (the documented one-frame caveat). Disabled
faces, empty option lists, and unmounted faces (the wire dies with the
face's scope; dyn_view regenerations rewire automatically) also return
`false`, never panic:

```rust
use abstracttui::prelude::*;

fn command_picker(cx: Scope) -> (View, SelectHandle) {
    let picker = SelectHandle::new();
    let view = Combobox::new(vec![
        SelectOption::new("nord"),
        SelectOption::new("aurora"),
    ])
    .handle(&picker)
    .placeholder("theme…")
    .view(cx);
    (view, picker) // `/theme` handler calls picker.open()
}
```

## The widget disposal-safety law

**Every widget completes its own bookkeeping — every write to its
scope-owned signals — BEFORE user callbacks run, so a callback may
dispose the widget's scope synchronously.** Closing a modal from the
button that confirmed it, from the list row that picked, from the
composer that submitted, is the NORMAL shape, not a hazard; no
one-tick "retire" deferral is needed anywhere. `EventCtx` calls
(`stop_propagation`, focus/capture requests) are dispatch-owned flags
and exempt — they are safe on either side of a callback.

Covered and pinned by a disposal test per site: `Button::on_click`
(both mouse and keyboard arms), `Checkbox`/`RadioGroup`/`Tabs`
`on_change`, `TextInput` and `TextArea` `on_change`/`on_submit`,
`List::on_select`/`on_activate`, `Table::on_select`/
`on_sort_requested`, and the select faces' commit `on_change` (the
popup follows its owner's scope down — the anchor-unmount cascade).
`Popup::on_dismiss` fires after the popup's own teardown for the same
reason. One knowable consequence: bookkeeping uses the state as the
widget left it — a callback that mutates the widget's state (a
submit-and-clear composer) sees that mutation rendered by the NEXT
event, not retroactively applied to the one that fired it.

## app — the runtime

`App::simple` is the whole happy path: mount a component, enter the
terminal, run until quit. This compiled example is the canonical first app —
Tab focuses, Enter/Space clicks, Ctrl+C quits, all by default:

```rust
use abstracttui::prelude::*;
use abstracttui::widgets::Button;

fn main() -> abstracttui::base::Result<()> {
    App::simple(|cx| {
        let count = cx.signal(0);
        Element::new()
            .style(LayoutStyle::column())
            .child(dyn_view(LayoutStyle::line(1), move || {
                text(format!("count: {}", count.get()))
            }))
            .child(Button::new("+1").on_click(move || count.update(|c| *c += 1)).view(cx))
            .child(text("Tab focuses · Enter clicks · Ctrl+C quits"))
            .build()
    })
}
```

For more control, `App::new(size)` + `mount` + `run` splits the steps, and
`App::quitter()` hands out a cloneable programmatic-quit handle. Ctrl+C
arrives as an ordinary key (raw mode); the quit-by-default policy is
overridden by any handler that consumes the event.

Around the core loop the module provides:

- **Overlays** — z-ordered layers above the main tree (`LayerHandle`,
  `ImageHandle`) for popups, menus, and pixel images.
- **Modal** — a centered, focus-trapped overlay panel: input is fully owned
  while open, Tab cycles inside, state created in the modal's scope dies on
  close. **Toast** — top-right chips that slide in, park for their duration
  at zero frame cost, then slide out and remove their layer.
- **AnchoredPanel / Popup / Tooltip** (`app::anchored`) — the three
  routing modes of the anchored-popup substrate, one placement engine
  (below-preferred, flip-above, viewport clamp): `AnchoredPanel` is the
  PASSIVE layer (never focused — keys stay with the anchor's owner;
  `Completion` builds the caret-anchored dropdown on it), `Popup` is the
  OWNED modal tree above the whole live stack with `DismissReason`-named
  endings (the Select family rides it), and `Tooltip` is the hover-timed
  passive label. All three close with their opener's scope (see the
  widgets section for the completion and select details).
- **Hooks** — `use_theme(cx)` (the app-level theme signal), `use_viewport(cx)`
  (terminal size as a signal), `use_startup_notices(cx)` (labeled startup
  degradations as a reactive list), and `use_caps(cx)` — the driver's LIVE
  `Capabilities` (env pass at enter, upgraded as active-probe replies fold
  in). Read it in a `dyn_view` for capability-honest UI: key hints that say
  "Shift+Enter newline" only where the kitty protocol is actually live,
  graphics-channel labels that flip when the probe proves a better channel.
  Read-only by contract (writing capabilities stays the driver's job);
  `current_caps()` is the untracked snapshot for plumbing.
- **KeymapHelp** — a ready-made `?` help modal listing the shortcuts
  reachable from the current focus plus every registered global action.

## app::keys — key press/release state (held keys)

Real-time surfaces (games' move-while-held, voice push-to-talk) need key
STATE over time, not key events. `use_key_state(cx)` arms a driver-fed
service that taps the input stream BEFORE the routing seam drops
releases:

```rust,ignore
use abstracttui::prelude::*; // use_key_state, KeyFidelity

let keys = use_key_state(cx);
// per frame / per tick:
let diagonal = keys.is_down(Key::Up) && keys.is_down(Key::Right);
// per turn edges (sealed by the driver's phase U):
let fired = keys.pressed_chord(KeyChord::plain(Key::Char(' ')));
```

**Fidelity is the contract.** `keys.fidelity()` answers what this
session can honestly report:

- `KeyFidelity::Full` — kitty release events are live (the terminal
  speaks the protocol AND the event-type flags are pushed; on
  probe-proven terminals this flips on within the first frames when the
  driver pushes the flags mid-session). `is_down`/`keys_down`/`released`
  carry true key state.
- `KeyFidelity::Degraded` — a legacy wire only ever reports presses.
  Press edges (`pressed`, `pressed_chord`) stay honest; the down-set
  stays EMPTY and releases never fire. There is deliberately no
  repeat-timeout approximation: auto-repeat cadence cannot distinguish
  "held" from "tapping fast", and a dropped repeat would fabricate a
  release mid-hold. Apps fall back to latch/tap semantics and label the
  gesture truthfully — `hold_gesture_label(fidelity, chord)` gives the
  wording ("hold Space" vs "press Space to start/stop").

Hygiene: the terminal losing focus clears the down-set and synthesizes
release edges for held keys (`keys.focus_cleared()` tells them apart
from wire releases) — a key released while unfocused never sticks down.
Reads are tracked signals: `dyn_view`s and effects re-run on edges.
Zero cost until the first `use_key_state` call arms the service, and
zero per-turn cost while no keys move.

## app::PushToTalk — the capture gesture

The voice capture contract over the key-state service (one binding,
three decisions owned):

```rust,ignore
let ptt = PushToTalk::bind(cx, KeyChord::plain(Key::Char(' ')))
    .on_start(|| recorder.start())
    .on_stop(|reason| recorder.stop(reason)); // Released | FocusLost | Cancelled

let state = ptt.state();        // Signal<CaptureState>: Idle | Held | Latched
let hint  = ptt.gesture_label(); // truthful per fidelity, updates live
```

On `Full` fidelity the chord is hold-to-talk (press starts, release
stops; a same-turn tap fires start then stop, in order). On `Degraded`
wires the same chord becomes toggle-to-talk (`PttMode::Latch`) — never
a fake hold. The terminal losing focus stops capture in EVERY mode
(`StopReason::FocusLost`), and capture never auto-restarts when focus
returns mid-hold: a fresh press is required. `ptt.cancel()` is the
programmatic stop. Terminals cannot see unfocused keys — there are no
global hotkeys here; audio capture itself is app-side.

## app::selection — screen-text selection and clipboard copy

Terminals in mouse-capture mode route drags to the application, so native
text selection stops working in every mouse-enabled TUI. The engine ships
the whole answer stack (see the
[troubleshooting matrix](troubleshooting.md#i-cant-select-text-with-the-mouse)
for the zero-code terminal bypasses). Three cloneable, thread-local
handles, all in `app::selection` (functions re-exported in the prelude):

```rust
use abstracttui::prelude::*; // selection(), mouse_capture(), copy_to_clipboard()

// Tier 3 — engine drag-select. Opt in once (or bind a key to toggle):
selection().set_enabled(true);   // left-drag now paints a selection
selection().is_active();         // a region is visible
selection().clear();             // Esc and click do this too

// Tier 2 — native selection mode: hand the pointer back to the terminal.
mouse_capture().suspend();       // native drag-select works; no mouse events arrive
mouse_capture().resume();        // re-arm the entered mouse mode (e.g. on next key)

// The app-reachable clipboard verb (OSC 52 through presenter custody):
copy_to_clipboard("exact source text");
```

While selection is enabled, the engine claims **left Down/Drag/Up only**:
dragging paints the theme's `selection_fg`/`selection_bg` inks over the
composed frame (damage-contract honest — only changed cells repaint), and
releasing copies. **Every copy ends the gesture** (0290): the region
clears with the copy, so the app's next keystrokes — including Enter and
`c` — route normally at once (a retained region used to silently eat
them in composer-shaped apps). The key table while a region is visible
(i.e. mid-drag):

| Key            | Effect                                   |
|----------------|------------------------------------------|
| Enter          | copy the region, then clear (one-shot)   |
| `c` / Ctrl+C   | copy the region, then clear (one-shot)   |
| Esc            | cancel — clear without copying           |
| anything else  | routes to the app normally               |

A fresh left click re-anchors; Ctrl+C only quits when no region is
visible. Wheel scrolling, hover, and every other key route normally the
whole time. Copies travel as OSC 52 through the presenter's byte custody;
terminals that did not advertise the capability still get the bytes
(harmless) plus a one-time labeled startup notice, and under tmux the
sequence is deliberately not passthrough-wrapped (tmux consumes OSC 52
natively — `set -g set-clipboard on`).

Selection semantics, stated plainly:

- **Screen text, not widget content.** What you copy is what the flattened
  frame shows: wide glyphs (CJK, emoji) are never split, blank cells read
  as spaces, trailing whitespace trims per row, rows join with `\n`.
  Soft-wrapped lines copy as separate rows; scrolled-away content cannot
  be selected. The logical text↔cells mapping is future work (backlog
  0160), not this feature.
- **Linear row flow, clamped to a pane.** The selection flows like a
  terminal's own: anchor to right edge, full middle rows, left edge to
  head. Both ends clamp to the pane under the drag *anchor* — the content
  box of the nearest clipping or padded ancestor (a `Scroll` viewport, a
  bordered `Block`), else the whole tree — so sibling panes and border
  glyphs never leak into a copy.
- **Zero idle cost.** With no active selection the render hook is two
  empty checks; a parked selection renders no frames until something
  changes.

`Terminal::set_mouse_reporting(bool)` is the tier-2 verb underneath
(implemented by both platform backends and `testing::CaptureTerm`;
`Driver::set_mouse_reporting` is the immediate form for embedders). One
platform note: job-control suspend (`Ctrl+Z`) re-enters with the original
options, re-arming reporting — suspend again after resume if you keep it
off.

## app — the full-redraw verb (Ctrl+L class)

The damage contract trusts the terminal to keep every cell the engine
painted. When that breaks EXTERNALLY — Cmd+K in Terminal.app,
`printf '\033c'` from a stray process, an emulator glitch — model-side
repaints cannot heal it: cells whose bytes did not change emit
nothing, so the loss is permanent. Two verbs (first-app/0299, exported
at `app::` and re-exported in the prelude) reach the driver's "screen
is unknown" resync — the same pair resize and suspend-resume run
(previous-frame model poisoned, presenter re-anchored, every layer
damaged, protocol images re-placed):

```rust
use abstracttui::prelude::*;

// The Ctrl+L binding every terminal app owes its users:
Element::new().shortcut(KeyChord::new(Mods::CTRL, Key::Char('l')), |_| {
    request_full_redraw() // next frame re-emits EVERY cell + re-places images
});

// Opt-in auto-heal: full redraw whenever the terminal reports
// focus-in (an external clear is nearly always followed by a focus
// round-trip, so the damage fixes itself before anyone looks):
set_redraw_on_focus_gained(true);
```

`request_full_redraw()` is callable from any component handler or
posted job on the app thread; the driver drains it at its next turn
(a call from a key handler is honored within the same turn). Cost is
bounded and honest: one full-frame emission, then idle returns to
zero bytes. The focus-regain opt-in defaults OFF — a full frame per
focus-in is real byte cost under tmux pane-switching cadence, so
existing sessions stay byte-identical unless the app asks
(`app::redraw_on_focus_gained()` reads the policy back). Use these for
terminal-side damage only; for ordinary content changes, signals and
tree damage already repaint exactly what changed.

## theme — design tokens

Widgets consume `TokenId`s resolved against the active theme's `TokenSet`;
they never hold raw colors. Twenty-six built-in themes ship in the registry:
the abstract family (`abstract-dark` — the default — plus light, aurora,
paper, ember, midnight, dawn), `observer-night`, catppuccin (mocha,
macchiato, frappe, latte), rose-pine (plus moon, dawn), `tokyo-night`,
`nord`, `one-dark`/`one-light`, `dracula`, `monokai`, `gruvbox`,
`solarized-dark`/`-light`, and `everforest-dark`/`-light`.

Switching is one signal write: widgets that read the theme signal re-render
fine-grained, and the app damages the whole tree so even static text
repaints in the new palette:

```rust
use abstracttui::prelude::*;

set_theme_by_id("catppuccin-mocha"); // false for unknown ids, nothing changes
```

`theme::list()` enumerates `(id, label, dark)` for a picker. Applications
can add their own themes at runtime with `theme::register(candidate, mode)`:
every registration runs the full contrast audit, and the mode decides
whether violations refuse the theme or register it with labeled findings.

## render — surfaces and paint (advanced)

Most applications never touch `render` directly — widgets and draw closures
do. The two concepts worth knowing:

**`Surface`** is the cell buffer draw closures write into. Damage is
recorded automatically by every write; the diff re-checks equality, so
over-approximate damage costs microseconds, never wrong pixels.

**`render::Style` is a patch, not an appearance.** `fg`/`bg` at `None` keep
what the target cell already has — text drawn over a filled panel keeps the
panel's background. Attributes are add/remove sets, so bold layers onto
existing content. `Style::absolute()` opts out (remove everything first),
and `merge` is sequential application — the later opinion wins:

```rust
use abstracttui::base::Rgba;
use abstracttui::render::{Attrs, Style};

// The common one-liner: ink + emphasis.
let err = Style::new().fg(Rgba::rgb(255, 80, 80)).bold();
assert_eq!(err.add, Attrs::BOLD);
assert_eq!(err.bg, None); // bg unset: keeps the panel underneath

// Patches compose; the later opinion wins where both have one.
let quoted = err.merge(Style::new().dim().fg(Rgba::rgb(150, 150, 150)));
assert_eq!(quoted.fg, Some(Rgba::rgb(150, 150, 150)));
assert_eq!(quoted.add, Attrs::BOLD | Attrs::DIM);
```

The one non-patch field is the hyperlink id: it always overwrites, because
inheriting a stale link under a fresh label would be a correctness hazard.

For effects, layers accept per-cell shaders (`CellShader`; built-ins in
`anim::shaders`). Shaders are billed by damage: static shaders cost nothing
after installation; animated shaders damage only what their `changed_region`
hint declares. For debugging: `render::snapshot(&surface)` prints a bordered
character grid, `snapshot_styles` adds per-row style annotations, and
`Compositor::set_debug_damage(true)` outlines every repaint region live.

**`md::StreamSession`** is the incremental entry into the markdown
pipeline (text arriving over time: model output, a growing log). Closed
blocks freeze — parsed once, never revisited — and only the open tail
re-parses per append, with any chunking of the same bytes yielding
blocks identical to `md::parse` of the whole source. An unclosed fence
reports as code from the moment its opening line arrives. It is
widget-agnostic; `Feed`'s streaming items ride its doc-vocabulary twin,
`md::DocStreamSession` (next section):

```rust
use abstracttui::render::md::{self, MdStyles, StreamSession};

let styles = MdStyles::default();
let mut s = StreamSession::new(styles.clone());
s.append("# Title\n\nStreaming **bo");
s.append("ld** text.");
assert_eq!(s.closed_blocks().len(), 1); // the heading sealed and froze
assert_eq!(
    s.finish(),
    md::parse("# Title\n\nStreaming **bold** text.", &styles)
);
```

## render::md — the doc vocabulary and the markdown reader surface

The core `md::Block` enum shipped exhaustive, so the extended block
kinds live in `md::DocBlock` (`#[non_exhaustive]`, wrapping the core
set verbatim in `DocBlock::Core`): `Table(TableBlock)` — GFM header +
alignment delimiter + body rows, inline styles inside cells, `\|`
escapes; `Image(ImageBlock)` — a whole-line `![alt](src)`; and
`Task(TaskBlock)` — `- [ ]` / `- [x]` items. `md::parse_doc` is the
entry; for sources containing none of the extended constructs it is
exactly `md::parse` wrapped in `Core` (test-pinned). Inline
`~~strikethrough~~` joined the core span vocabulary (attribute-only:
`Attrs::STRIKE`). `md::DocStreamSession` is the streaming twin of
`StreamSession` for the doc vocabulary — same freeze/equivalence
contract; a table OPENS once its header + delimiter lines are complete,
grows a row per pipe line, and CLOSES (seals) at the first non-pipe
line.

`md::outline(source)` extracts headings as `Heading { level, text,
anchor_id }` with GitHub-compatible, deduplicated slugs
(`md::slugify`). Width-resolved positions live on the widget:
`MarkdownView::outline_rows(source, &tokens, width)` pairs each heading
with the typeset ROW its text starts at (the TOC jump target), and
`MarkdownView::resolve_anchor(...)` answers `[text](#anchor)` links.

`MarkdownView` AND `Feed` markdown items render the full doc
vocabulary (one shared typeset recipe — a feed item and a reader pane
can never typeset the same source differently): tables typeset
through the Table widget's own column solver (one width policy —
natural widths when they fit, proportional flex + per-cell ellipsis
when they don't); images render as MOSAIC rows in the flow — sized from
a header-only probe (`gfx::probe_dimensions`) at typeset, DECODED
LAZILY on first draw and cached by (path, size), with alt-text captions
and labeled decode-failure states (pixel-protocol images in scrollable
flow are deliberately out of scope; mosaic cells are cell-safe in any
scroll context). Streaming feed items ride `md::DocStreamSession`
(see "Feed — streaming transcripts" above).

Find-in-document: `MarkdownView::find(source, &tokens, width, query,
case_insensitive)` returns `MdSearchMatch { row, bytes, cells }` over
the TYPESET text (matches live in what the eye sees; offsets snap to
grapheme clusters), and `.highlights(matches, current)` paints them
non-destructively at draw in selection tones, the current match
distinguished with BOLD+UNDERLINE. An empty query costs nothing. The
underlying text↔cells mapping (byte offset ↔ column, both directions)
is the shared substrate content selection (backlog 0160) will consume.
`examples/reader.rs` composes all of it into an mdpad-class reader.

## gfx — images

`gfx::decode_image(bytes)` sniffs the magic bytes (containers lie, bytes do
not) and decodes PNG or baseline JPEG into a `Bitmap` — owned RGBA8 with
get/set, nearest and bilinear resize, cropping, and a box-filter mip chain.
Unknown formats are rejected by name, telling the caller what does decode;
truncated or hostile bytes are named errors, never panics.

Three presentation entry points, smallest first:

```rust
use abstracttui::base::{Rect, Rgba};
use abstracttui::gfx::{render_to_cells, Bitmap};
use abstracttui::term::Capabilities;

let img = Bitmap::new(16, 8, Rgba::rgb(180, 90, 30));
let cells = render_to_cells(&img, Rect::new(2, 1, 8, 4), &Capabilities::default());
assert_eq!(cells.len(), 8 * 4);
```

- `render_to_cells` picks the best mosaic mode for the probed terminal and
  returns ready-to-blit cell patches; `MosaicMode::auto(&caps)` returns both
  the mode and the reason it was chosen (half-block, quadrant, sextant, or
  braille; optional Floyd–Steinberg dithering).
- `widgets::Image` is the widget form — always mosaic, because a draw
  closure owns cells, not escape bytes.
- `gfx::ImageSession` manages the pixel protocols (kitty, iTerm2, sixel):
  slots keyed by the caller, content versions, minimal traffic per channel —
  kitty transmits once and re-places on move; iTerm2 and sixel honestly
  re-emit. Bytes reach the terminal through the presenter, and tmux
  passthrough wrapping applies automatically when capabilities prove it.

## three — 3D models

`three::quick_view(path)` is the five-line hello: load a GLB, get a camera
framed on the model's bounds and a default light, render:

```rust
use abstracttui::three::{self, Framebuffer, SceneRenderer};

let view = three::quick_view("model.glb")?;
let mut fb = Framebuffer::new(160, 96);
SceneRenderer::new().render(&view.scene(), &mut fb);
// fb -> mosaic cells via gfx, or hand the model to widgets::Viewport3D.
```

Underneath: `Model::load(bytes)` / `load_glb(path)` parse and validate the
GLB (unsupported features reject by name; recoverable gaps degrade with
labels into `model.warnings`), `Scene`/`Camera`/`Light` describe the view,
and `SceneRenderer` rasterizes with z-buffer, texturing, and mips.
`model.animations()` lists clips; `sample_pose_full(clip, t, &mut pose)`
produces node worlds and skin joint matrices, pure in `t` and allocation-free
at steady state — loop with `t % clip.duration()`. One culling note: bare
`Scene::new` culls back faces (procedural meshes are consistently wound);
`QuickView::scene()` and `Viewport3D` render double-sided, because
real-world exports are not.

## term and input — the terminal, when you need it

Applications under `App` rarely touch these; embedders and diagnostics do.
`Capabilities::detect_env()` is the free, instant, conservative environment
pass; the active probe refines it concurrently at startup. `caps.summary()`
is the multi-line human report (`summary_line()` the one-liner); scripts
should read fields, not parse prose. `EnterOptions` declares the session
posture — the default is the full-screen stance (alternate screen, hidden
cursor, button-drag mouse, bracketed paste, focus events), with kitty
keyboard flags as an explicit opt-in:

```rust
use abstracttui::term::{Capabilities, EnterOptions, TermRead, Terminal, UnixTerminal};
use std::time::{Duration, Instant};

let caps = Capabilities::detect_env(); // free, instant, conservative
let mut term = UnixTerminal::new()?;   // real device fd acquisition
term.enter(&EnterOptions::default())?; // raw mode + altscreen + modes

match term.read(Some(Instant::now() + Duration::from_secs(5)))? {
    TermRead::Input(bytes) => { /* feed input::Parser */ }
    TermRead::Resize(size) => { /* re-layout */ }
    TermRead::Wake => { /* another thread wants the loop */ }
    TermRead::Idle => { /* deadline expired */ }
}

term.leave()?; // also runs on Drop — the terminal always restores
```

`input::Parser` turns raw bytes into structured events — resumable across
arbitrary chunk splits (mid-UTF-8, mid-escape), never panicking on any
input. `input::EventReader` glues a terminal to the parser and owns the
ESC-disambiguation deadlines.

Kitty keyboard flags follow the PROBE, not just the environment: the env
pass claims the protocol only for terminals that speak it out of the box
(kitty, ghostty, foot — WezTerm ships it config-off, so its claim waits
for probe evidence), and when the active probe proves the protocol on a
terminal env could not claim (iTerm2 ≥ 3.5, VS Code/Cursor, Warp), the
driver pushes the standard flags mid-session via
`Terminal::set_kitty_keyboard` — Shift+Enter-class chords start working
without a restart. The verb updates the terminal's session accounting,
so `leave` pops exactly what was pushed and job-control suspend/resume
stays symmetric (pop on suspend, re-push on resume). Embedders that
enter with explicit `RunConfig::enter` options own their posture: the
driver never upgrades it.

## testing — the headless harness

The `testing` module ships in the library so applications can test against
the same machinery the engine tests itself with: `CaptureTerm` is an
in-memory terminal that records emitted bytes and models the screen,
`VtScreen` is the VT100/xterm interpreter that serves as ground truth
("the bytes we emitted produce the frame we intended"), and `app::Driver`
pumps real frames — the same pipeline production uses — without a tty:

```rust
use abstracttui::prelude::*;
use abstracttui::app::Driver;
use abstracttui::testing::CaptureTerm;

let size = Size::new(20, 4);
let mut app = App::new(size);
app.mount(|cx| {
    let n = cx.signal(0);
    Element::new()
        .shortcut(KeyChord::plain(Key::Char('+')), move |_| n.update(|v| *v += 1))
        .child(dyn_view(LayoutStyle::line(1), move || text(format!("n = {}", n.get()))))
        .build()
}).unwrap();

let mut term = CaptureTerm::new(size);
let cfg = RunConfig { probe: false, ..RunConfig::default() };
let mut driver = Driver::new(&mut app, &mut term, cfg).unwrap();
driver.turn(&mut app, &mut term).unwrap();          // first frame
assert!(term.screen().to_text().contains("n = 0"));

term.push_input(b"+");                              // a keypress
driver.turn(&mut app, &mut term).unwrap();          // dispatch + repaint
assert!(term.screen().to_text().contains("n = 1"));
```

Input is fed as the terminal would send it, so every dispatch, focus, and
damage path is the real one. For pure component tests, skip the driver: mount
into a `ui::UiTree`, dispatch events, draw into a `ui::BufferCanvas`.
Golden-snapshot assertions and deterministic fuzz helpers round out the
module.

## Stability and limits

Plain statements of current behavior:

- **JPEG** decoding is baseline sequential only; progressive and arithmetic
  variants reject by name. **PNG** supports 8-bit depths without interlacing
  (Adam7 rejects by name).
- **Sixel** uses one palette per emission: multiple live sixel images
  recolor each other — prefer one per screen. iTerm2 and sixel have no
  placement model (moves re-emit the payload); only kitty gets placement
  escapes and true deletes.
- **Pixel protocols** are verified byte-for-byte against protocol models,
  not live terminals; unicode mosaic is the universal, always-safe path.
- **3D animation** supports LINEAR and STEP interpolation; CUBICSPLINE and
  morph weights skip with labels; rotations nlerp (shortest path), not
  slerp. Skinning reads `JOINTS_0`/`WEIGHTS_0` (four joints per vertex,
  linear blend). Textures: base color only, REPEAT wrap, per-triangle mips.
- **Mosaic** color resolution is two colors per cell (the glyph split
  carries the rest); braille conveys structure, not color; sextant glyphs
  need a recent font and are an explicit opt-in.
- **Ambiguous-width characters** follow `unicode-width` narrow semantics. A
  terminal configured ambiguous-wide breaks cell layout for every terminal
  application; the presenter's cursor discipline bounds the drift but
  cannot erase it.
- **Capacity ceilings** degrade with labels, never unbounded growth: 4096
  distinct long grapheme clusters per surface (then U+FFFD), 65535
  hyperlinks per surface (then plain text), with counters exposed.
- **Scroll optimization** requires DECSTBM/SU/SD compliance — present in
  every VT100 descendant — and can be forced off via `PresenterOpts`.
- **Windows** compiles clean and its extracted logic is unit-tested on every
  host, but it has not yet run on a live Windows machine; treat a first
  Windows deployment as a beta event. macOS and Linux are the live-verified
  platforms.
