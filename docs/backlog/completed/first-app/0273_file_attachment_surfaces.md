# 0273 — File-attachment surfaces: paste intercept, drop classifier, FilePicker

Status: completed 2026-07-25 (same-wave delivery — filed directly in
completed/, the app-kits/0535 precedent)
Owner: engine (widgets/input, widgets/textarea, input/paste,
widgets/file_picker)
Effort: L

## The field request

Operator tasking (2026-07-25, commons#5407, verbatim intent): "i am
unsure that we can easily attach files to code-tui. discuss with @tui
how you could create such a capability (a) to select files and (b) to
accept drag & drop of files into code-tui to serve as attachments."
The code-tui seat owns the client half (upload transport, chips,
fs-existence checks); the engine ruling (commons#5408) took three
surfaces, and code-tui bound to them (#5409). abstractcode (the Python
sibling) declared itself a third consumer for the classifier (#5410).

The engine facts behind the split: terminals have NO drop protocol —
dropping a file PASTES its path (bracketed, so it arrives as one
`UiEvent::Paste` on the focused widget), and the spelling varies per
terminal. Before this item the two editors inserted every paste
directly with no intercept, the cross-terminal drop knowledge lived
nowhere, and file selection meant hand-rolling List-in-Modal per app.

## The ask (as ruled)

1. `TextInput::on_paste` / `TextArea::on_paste(FnMut(&str) ->
   PasteAction)` — intercept BEFORE insertion; `Insert` = today's
   behavior byte-identical, `Consume` = the widget inserts nothing.
2. `input::paste::classify(&str) -> Option<Vec<String>>` —
   engine-owned pure classifier over the researched drop spellings of
   the real terminals (iTerm2, kitty, WezTerm, Terminal.app, Ghostty,
   Windows Terminal, VTE class).
3. `FilePicker` — modal-friendly picker widget over a pure
   `FileSource` seam with a `std::fs` source beside it.
4. Example + docs (api.md section, the terminal-drop reality table).

## Completion report (2026-07-25)

Shipped exactly the ruled shapes, additive over 0.2.19:

- **`on_paste`, both editors, uniform** (`widgets/paste_hook.rs`
  `#[path]` sibling shared by input.rs/textarea.rs):
  `on_paste(impl FnMut(&str) -> PasteAction + 'static)`;
  `PasteAction::{Insert, Consume}` is `#[non_exhaustive]` (ADR-0003
  §3 — the engine may grow it; in-crate matches stay exhaustive so new
  variants walk every consumption site). The hook sees the RAW paste
  (line endings intact; TextInput's fold-to-spaces and TextArea's
  newline normalization are Insert-path only) and fires in `masked`
  fields (documented password-blocking use). Disposal-safety law
  refined for the interceptor shape, documented in api.md: the hook
  runs FIRST by contract; `Consume` is followed by zero widget writes,
  and an `Insert` from a hook that disposed the scope is treated as
  consumed via a `Signal::is_alive` re-check — never a dead-signal
  panic. Unbound hook = the pre-0273 code path, byte-identical.
- **`input::paste::classify`** (src/input/paste.rs, pure, zero I/O):
  shell-style tokenization (backslash escapes, single/double quotes
  with POSIX escape subset + segment concatenation, Windows
  drive/UNC tokens consumed backslash-LITERAL — POSIX decoding would
  eat `C:\Users`), per-token shape acceptance (POSIX absolute, `~/`
  returned as-is, drive/UNC, `file://` with percent-decoding and
  empty/`localhost` hosts), newline-joined multi-line drops (the
  Ghostty GTK finding — see below), 64 KiB/4 KiB/512-path guards.
  Ambiguity policy documented in the module docs: when unsure, `None`
  (a false positive eats user text; a false negative just pastes) —
  raw unescaped spaces, unterminated quotes, control chars, relative
  paths, non-file URLs, prose all refuse. Existence checks stay
  app-side (the ruled split). The corpus provenance table (terminal →
  spelling → source) lives in the module docs; researched 2026-07-25:
  Terminal.app + iTerm2 backslash-escape (iTerm2 pref single-quotes),
  Ghostty backslash-escapes on BOTH platforms and its GTK apprt joins
  multi-drops with NEWLINES (PR #4211 — a space-joined-only rule
  would refuse a mainstream terminal; this AMENDS the preliminary
  ruling's "multi-file space-joined" line), WezTerm's five
  `quote_dropped_files` modes (default SpacesOnly), kitty raw-by-
  policy, Windows Terminal double-quotes-when-spaces + WSL
  single-quoting (embedded-quote bug #18006 refuses honestly),
  GNOME/VTE `g_shell_quote`d conversion, MATE's raw uri-list bug.
- **`FilePicker`** (widgets/file_picker.rs + `#[path]` siblings
  file_picker_source.rs / file_picker_view.rs): breadcrumb
  (LEFT-truncated — the tail is the informative end), live filter
  input (the single focus stop; autofocus default ON per the 0230
  finding, builder opt-out), rows with themed kind glyphs (`▸`
  accent dirs / `·` muted files), optional size column, mark badges,
  shared scrollbar. Keys at CAPTURE phase on the picker root (the
  anchored-completion precedent): Enter descend/pick,
  Backspace/Left = parent ONLY when the filter is empty (otherwise
  they edit the filter — a refinement over the ruling's bare
  "Backspace = up-dir": unconditional-parent would make the filter
  uneditable), Space toggles FILE marks in multi-select (marks are
  full paths and persist across directories; commit order = mark
  order), Up/Down/Page selection, Esc NEVER consumed (host modal owns
  dismissal). Mouse: click selects, click-on-selected activates (the
  List convention). PURE widget: `FileSource::read_dir(path) ->
  Result<Vec<FileEntry>, String>` called once per navigation, never
  per frame; errors render honestly in the list area.
  `StdFileSource`: std::fs, dirs-first case-insensitive, hidden
  toggle, symlinks resolved through `fs::metadata` (a link to a dir
  descends), broken links degrade to sizeless file rows.
  DELIBERATE NON-REUSE of the List widget (recorded here as the
  ruling allowed): List's Space-aliases-Enter activation conflicts
  with Space-toggles-marks, its `scroll_to` command scrolls the item
  to the TOP (a yank per keystroke — not ensure-visible), and its
  single-style rows cannot carry per-kind glyph inks or an aligned
  size column; the picker reuses the shared `draw_scrollbar`,
  `truncate_ellipsis`, and List's selection/ensure-visible PATTERNS
  instead.
- **Example**: `examples/attachments.rs` — composer with
  on_paste+classify (drops become chips), Ctrl+O opens FilePicker in
  a Modal (Esc closes via the host's shortcut — the closer pattern),
  headless exit-0, teaching header; README learning-path row; live
  PTY smoke `live_smoke::live_attachments` (drop → picker → Esc →
  Ctrl+C through a real pseudo-terminal) — run once, green (82s incl.
  build).
- **Docs**: api.md "File attachments" section (three surfaces, the
  terminal table, the asymmetry policy, the client recipe);
  CHANGELOG under Unreleased; prelude re-exports (`PasteAction`,
  `FilePicker`, `FileSource`, `FileEntry`, `StdFileSource`;
  `classify` stays behind `input::paste` by prelude curation).
- **Tests** (44 new):
  - classifier corpus (24, `input::paste::tests`): one test per
    terminal spelling incl. all five WezTerm modes, Ghostty
    newline-joined multi-drop, WSL malformed-quote refusal, uri-list
    CRLF, `file:///C:/` drive form, `~`, root, compiler-diagnostic
    single-token acceptance; refusals: prose/commands/code, non-file
    URLs + non-local hosts, relative/bare words, malformed
    quoting/escapes, control chars + bad percent-encoding, size/count
    guards, interior blank lines, windows edge shapes;
  - hook, TextInput (4, `input::paste_tests`): consume-inserts-
    nothing + raw text to the hook + no on_change, Insert
    byte-identical vs an unhooked twin, masked-still-fires (block +
    allow), disposing-hook safe on both arms;
  - hook, TextArea (4, `textarea::paste_tests`): raw-text pin,
    consume touches nothing (buffer/caret/history/on_change), Insert
    byte-identical twin, disposal both arms;
  - FilePicker (12 + StdFileSource 1): breadcrumb/glyphs/sizes
    render, descend + parent (Backspace AND Left), filter narrows
    live + Backspace edits-before-navigates + no-matches, single
    pick payload, multi-select badge + mark-order commit +
    cross-directory persistence + files-only toggling, empty dir,
    unreadable dir rendered + recoverable, arrow clamping,
    click-then-click activation, on_pick-disposes-scope, deep
    breadcrumb left-truncation; pure helpers (filter/left-truncate/
    format_size); StdFileSource tempdir integration (sort, sizes,
    hidden toggle, missing-dir error);
  - driver-level (4, `tests/wave_attachments.rs`): bracketed-paste
    drop → chip through REAL wire bytes (`ESC[200~…ESC[201~`) with
    prose still inserting, FilePicker in a Modal picking through wire
    keys + closing synchronously, keyboard round-trip
    (descend/parent/marks/Esc-stays-the-host's), zero-idle pin
    (parked composer + open picker = 0 bytes, 8 turns);
  - `unknown_seq_count == 0` on every driver test (all bytes
    modeled).
- **Gates**: whole-workspace battery green — 2,184 tests passed, 0
  failed (1,470 lib + 675 across the integration suites + 39 sibling
  crates' tests and doctests; 100 ignored = perf/soak/live-pty/fuzz
  gates, of which live_attachments was additionally run once, green),
  clippy `--workspace --all-targets -- -D warnings` zero, fmt clean,
  `cargo semver-checks --baseline-version 0.2.19` 196 checks passed
  (additive-clean), `examples/attachments` headless exit-0.
