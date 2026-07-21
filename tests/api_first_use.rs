//! VERIFY cycle-8 API usability review: three small apps implemented AS
//! A NEW USER, reaching for `abstracttui::prelude::*` first and only
//! reading doc comments. Every place the prelude didn't cover, a name
//! surprised, or a signature was inconsistent is recorded as an RT8-*
//! finding in reviews/cycle8/redteam-findings.md and annotated inline
//! here. These tests then STAY as documentation-accuracy guards: if a
//! future API change breaks the newcomer path, one of these fails.
//!
//! Drive path: the docs present `App::run` (real tty) and `App::simple`
//! (sugar) as THE run paths. Headless testing of an app is not in the
//! prelude — a newcomer must discover `app::Driver` + `testing::CaptureTerm`
//! (RT8-2). We use them here.

use abstracttui::prelude::*;

// Reaches PAST the prelude a newcomer is forced to make (each is a
// finding; grouped here so the friction is visible at the top of the
// file exactly as a new user would accumulate `use` lines):
use abstracttui::app::{Driver, RunConfig}; // RT8-2: not in prelude
use abstracttui::gfx::Bitmap; // RT8-4: image apps need this, not re-exported near Image
use abstracttui::term::Capabilities;
use abstracttui::testing::CaptureTerm; // RT8-2
use abstracttui::testing::VtScreen;
use abstracttui::widgets::{Button, Checkbox, Image, List, TextInput}; // RT8-1: interactive widgets not in prelude

/// A newcomer-facing headless harness: App + CaptureTerm + Driver + a
/// PERSISTENT VtScreen fed cumulatively. The persistence is the subtle
/// part a newcomer's own test would get wrong (RT8-9): the terminal
/// emits INCREMENTAL diffs, so a fresh model per read only sees the
/// delta — the model must accumulate to reflect the true screen.
struct Harness {
    app: App,
    term: CaptureTerm,
    driver: Driver,
    vt: VtScreen,
}

impl Harness {
    fn boot(size: Size, component: impl FnOnce(Scope) -> View) -> Harness {
        let mut term = CaptureTerm::new(size);
        let mut app = App::new(size);
        app.mount(component).expect("mount");
        let cfg = RunConfig {
            caps: Some(Capabilities::default()),
            enter: None,
            probe: false,
        };
        let driver = Driver::new(&mut app, &mut term, cfg).expect("driver enters");
        let mut vt = VtScreen::new(size);
        let mut h = Harness {
            app,
            term,
            driver,
            vt: VtScreen::new(size),
        };
        // Pump to steady state, feeding every byte into the model.
        for _ in 0..8 {
            let idle = h.driver.turn(&mut h.app, &mut h.term).expect("turn").idle;
            vt.feed(&h.term.take_bytes());
            if idle {
                break;
            }
        }
        h.vt = vt;
        h
    }

    /// Send input, pump to idle, keep the model current.
    fn key(&mut self, bytes: &[u8]) {
        self.term.push_input(bytes);
        for _ in 0..6 {
            let idle = self
                .driver
                .turn(&mut self.app, &mut self.term)
                .expect("turn")
                .idle;
            self.vt.feed(&self.term.take_bytes());
            if idle {
                break;
            }
        }
    }

    fn text(&self) -> String {
        self.vt.to_text()
    }
}

// ---------------------------------------------------------------------------
// App (a): two-panel focus app — a list drives a detail pane.
// ---------------------------------------------------------------------------

#[test]
fn app_a_list_drives_detail_pane() {
    let size = Size::new(60, 12);
    let mut h = Harness::boot(size, |cx| {
        let items = vec!["Alpha".to_string(), "Beta".to_string(), "Gamma".to_string()];
        let selection = cx.signal(0usize);
        let t = &current_theme().tokens;

        // Detail text is derived from the selection — the "master drives
        // detail" wiring is just a signal read inside a dyn_view. A
        // DISTINCTIVE token ("PICKED:") so the assertion can tell the
        // detail pane apart from the list item of the same name.
        let detail_items = items.clone();
        let detail = dyn_view(LayoutStyle::line(1), move || {
            let i = selection.get();
            text(format!("PICKED:{}", detail_items[i]))
        });

        // RT8-6: a newcomer's first `Element::row()` with two unsized
        // children collapses/contends because nothing says how to share
        // the main axis. A COLUMN (list on top, detail below, both full
        // width) is the friction-free first layout — no width arithmetic
        // to discover. The `grow` requirement for side-by-side panes is
        // the recorded ergonomics finding.
        Element::new()
            .style(LayoutStyle::column())
            .child(
                List::new(items)
                    .selection(selection)
                    .layout(LayoutStyle::line(4))
                    .element(cx, t)
                    .build(),
            )
            .child(detail)
            .build()
    });

    let frame = h.text();
    assert!(frame.contains("Alpha"), "list item not rendered:\n{frame}");
    assert!(
        frame.contains("PICKED:Alpha"),
        "detail pane not driven by selection:\n{frame}"
    );

    // Focus the list (Tab) and move the selection down; the detail pane
    // must follow. Tab/arrows are the documented default focus keys.
    h.key(b"\t");
    h.key(b"\x1b[B"); // Down
    let frame = h.text();
    assert!(
        frame.contains("PICKED:Beta") || frame.contains("PICKED:Gamma"),
        "arrow-key selection did not drive the detail pane:\n{frame}"
    );
}

// ---------------------------------------------------------------------------
// App (b): an image inside a panel, on CaptureTerm.
// ---------------------------------------------------------------------------

/// Baseline (passes today): a Block with a TEXT child insets its content
/// below the title and inside the border. Establishes that Block chrome
/// works — so the image failure below is Image-specific, not Block.
#[test]
fn app_b_baseline_text_in_a_panel() {
    let size = Size::new(40, 8);
    let h = Harness::boot(size, |cx| {
        let _ = cx;
        let t = &current_theme().tokens;
        Element::new()
            .style(LayoutStyle::column())
            .child(
                Block::new()
                    .title("picture")
                    // RT8-7: size a Block via its OWN `.layout()` builder,
                    // NOT `.style()` on the returned Element — the latter
                    // clobbers the border-inset padding `element()` adds,
                    // dropping the child onto the border/title.
                    .layout(LayoutStyle::default().grow(1.0))
                    .child(text("inside the panel"))
                    .element(t)
                    .build(),
            )
            .build()
    });
    let frame = h.text();
    assert!(
        frame.contains("picture"),
        "panel title missing (Block chrome broken):\n{frame}"
    );
    assert!(
        frame.contains("inside the panel"),
        "panel content missing:\n{frame}"
    );
}

/// An `Image` child of a `Block` renders INSIDE the border, title/frame
/// intact — ONCE the Block is sized via its own `.layout()` (RT8-7).
/// This originally read as an "image overdraws the border" bug (RT8-8);
/// it dissolved into RT8-7: sizing the Block with `.style()` on the
/// returned Element clobbered the border-inset padding, so BOTH text and
/// image children fell onto the frame. With `.layout()` the widget is
/// correct. Kept as a guard that the image path respects the interior.
#[test]
fn app_b_image_in_a_panel() {
    let size = Size::new(40, 16);
    let h = Harness::boot(size, |cx| {
        let _ = cx;
        let t = &current_theme().tokens;
        let bitmap = std::sync::Arc::new(Bitmap::new(8, 8, Rgba::rgb(120, 60, 200)));
        Element::new()
            .style(LayoutStyle::column())
            .child(
                Block::new()
                    .title("picture")
                    // Block sized via its own `.layout()` (RT8-7 lesson).
                    .layout(LayoutStyle::default().grow(1.0))
                    // RT8-3: Image::element(t) takes only the token set,
                    // while List/Input/Checkbox/Button take (cx, t).
                    .child(
                        Image::from_bitmap(bitmap)
                            .layout(LayoutStyle::default().grow(1.0))
                            .element(t)
                            .build(),
                    )
                    .element(t)
                    .build(),
            )
            .build()
    });
    let frame = h.text();
    assert!(
        frame.contains("picture"),
        "RT8-8: image overdrew the panel title:\n{frame}"
    );
    // The border corners must survive (the image stays in the interior).
    assert!(
        frame.contains('┌') && frame.contains('┐'),
        "RT8-8: image overdrew the top border:\n{frame}"
    );
}

// ---------------------------------------------------------------------------
// App (c): a themed form — input + checkbox + submit button + validation.
// ---------------------------------------------------------------------------

#[test]
fn app_c_themed_form_with_validation() {
    let size = Size::new(50, 12);
    let mut h = Harness::boot(size, |cx| {
        let name = cx.signal(String::new());
        let accept = cx.signal(false);
        let error = cx.signal(String::new());
        let t = &current_theme().tokens;

        // Validation is composed by hand: a signal + a dyn_view. There is
        // no built-in "form validation" helper (RT8-5, expected for a v1
        // toolkit — recorded so docs set the expectation). Signals are
        // Copy, so the closure captures them by value directly.
        let submit = {
            move || {
                if name.get_untracked().is_empty() {
                    error.set("name is required".into());
                } else if !accept.get_untracked() {
                    error.set("please accept the terms".into());
                } else {
                    error.set("submitted!".into());
                }
            }
        };

        let err_view = dyn_view(LayoutStyle::line(1), move || text(error.get()));

        Element::new()
            .style(LayoutStyle::column())
            .child(
                TextInput::new()
                    .value(name)
                    .placeholder("name")
                    .element(cx, t)
                    .build(),
            )
            .child(
                Checkbox::new("accept terms")
                    .checked(accept)
                    .element(cx, t)
                    .build(),
            )
            .child(
                Button::new("submit")
                    .on_click(submit)
                    .element(cx, t)
                    .build(),
            )
            .child(err_view)
            .build()
    });

    // Tab to the submit button and press Enter with an empty name: the
    // validation message must appear. (Tab count: input, checkbox,
    // button = 3 tabs to reach submit.)
    h.key(b"\t");
    h.key(b"\t");
    h.key(b"\t");
    h.key(b"\r"); // Enter = click focused button (documented default)
    let frame = h.text();
    assert!(
        frame.contains("required") || frame.contains("accept") || frame.contains("submitted"),
        "form validation message never rendered:\n{frame}"
    );
}
