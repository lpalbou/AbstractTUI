//! ReasoningSelect tests (split file, `#[path]`-included as
//! `reasoning::tests`): the label-grammar goldens + the lock-glyph
//! width research pin as units; the three coupling states, the
//! unknown-state override flow, commit semantics, the modal
//! screen-space anchor pin (the P1 class), a11y and zero-idle through
//! the REAL `Driver` + `CaptureTerm` (the theme_switcher rig).

use std::cell::RefCell;
use std::rc::Rc;

use super::*;
use crate::app::driver::{Driver, RunConfig};
use crate::app::overlays::OverlayContent;
use crate::app::popups::Modal;
use crate::app::App;
use crate::base::Size;
use crate::term::Capabilities;
use crate::testing::CaptureTerm;
use crate::ui::{dyn_view_scoped, text};

// ------------------------------------------------------------- units

#[test]
fn ladder_is_the_contract_spelling() {
    assert_eq!(
        REASONING_LADDER,
        ["none", "minimal", "low", "medium", "high", "xhigh"],
        "the effort ladder is shared contract vocabulary — never respell"
    );
    assert_eq!(REASONING_AUTO, "auto");
}

#[test]
fn grammar_goldens_all_values_by_lock_state() {
    // The parity grammar: ONE source for every footer. Golden across
    // the whole ladder + auto, both states, both forms.
    for v in REASONING_LADDER.iter().chain([REASONING_AUTO].iter()) {
        assert_eq!(reasoning_label(v, LockState::Unlocked), format!("r: {v}"));
        assert_eq!(
            reasoning_label(v, LockState::Locked),
            format!("r: {v} (locked)")
        );
        assert_eq!(
            reasoning_label_glyph(v, LockState::Unlocked),
            format!("r: {v}"),
            "unlocked glyph form == plain form (no marker to carry)"
        );
        assert_eq!(
            reasoning_label_glyph(v, LockState::Locked),
            format!("r: {v} \u{2298}")
        );
    }
}

#[test]
fn lock_glyph_is_narrow_in_both_width_conventions() {
    use unicode_width::UnicodeWidthChar;
    // The research pin (module docs): the chosen glyph must measure 1
    // under BOTH unicode-width opinions — `width != width_cjk` is the
    // crate's own East-Asian-Ambiguous oracle (text::is_risky_ambiguous).
    let chosen = LOCK_GLYPH.chars().next().expect("glyph");
    assert_eq!(chosen, '\u{2298}');
    assert_eq!(chosen.width(), Some(1));
    assert_eq!(chosen.width_cjk(), Some(1), "never double-width");
    assert_eq!(crate::text::width(LOCK_GLYPH), 1);
    // The rejected candidates, pinned WITH their rejection reasons so
    // the research cannot silently rot: the emoji lock is double-width
    // (and emoji-promotable); the squared key, the multiplication sign
    // and the shipped ● are Ambiguous (double under ambiguous-wide
    // terminals — the ◐ risk the ThemeSwitcher note names).
    assert_eq!('\u{1F512}'.width(), Some(2), "🔒 emoji lock: rejected");
    for (c, name) in [
        ('\u{26BF}', "⚿ squared key"),
        ('\u{00D7}', "× multiplication sign"),
        ('\u{2262}', "≢ not identical to"),
    ] {
        assert_ne!(c.width(), c.width_cjk(), "{name}: ambiguous — rejected");
    }
}

#[test]
fn facts_constructors_encode_the_three_states() {
    let capable = ReasoningFacts::capable(["low", "high"]);
    assert_eq!(capable.support, Some(true));
    assert_eq!(capable.levels, vec!["low".to_string(), "high".to_string()]);
    let non = ReasoningFacts::non_reasoning();
    assert_eq!(non.support, Some(false));
    assert!(non.levels.is_empty());
    let unknown = ReasoningFacts::unknown();
    assert_eq!(unknown.support, None);
    assert_eq!(unknown, ReasoningFacts::default(), "absent block = default");
}

// ------------------------------------------------------------ the rig

const VP: Size = Size::new(110, 34);

struct Rig {
    app: App,
    term: CaptureTerm,
    driver: Driver,
    overlays: super::super::overlays::Overlays,
    scope: Scope,
}

/// Real App + Driver + CaptureTerm; the control under test renders at
/// row 1 (row 0 is a header line, plain text below).
fn rig(vp: Size, control: impl FnOnce(Scope) -> View + 'static) -> Rig {
    let mut term = CaptureTerm::new(vp);
    let mut app = App::new(vp);
    let overlays = app.overlays();
    let holder: Rc<RefCell<Option<Scope>>> = Default::default();
    let h = holder.clone();
    app.mount(move |cx| {
        *h.borrow_mut() = Some(cx);
        Element::new()
            .style(LayoutStyle::column())
            .child(text("reasoning rig"))
            .child(
                Element::new()
                    .style(LayoutStyle::line(1))
                    .child(control(cx))
                    .build(),
            )
            .child(text("below content"))
            .build()
    })
    .expect("mount");
    let cfg = RunConfig {
        caps: Some(Capabilities::with(|c| {
            c.truecolor = true;
            c.colors_256 = true;
        })),
        enter: None,
        probe: false,
        ..RunConfig::default()
    };
    let driver = Driver::new(&mut app, &mut term, cfg).expect("driver");
    let scope = holder.borrow().expect("mount scope");
    Rig {
        app,
        term,
        driver,
        overlays,
        scope,
    }
}

impl Rig {
    fn settle(&mut self) {
        for _ in 0..64 {
            if self
                .driver
                .turn(&mut self.app, &mut self.term)
                .expect("turn")
                .idle
            {
                break;
            }
        }
    }

    fn input(&mut self, bytes: &[u8]) {
        self.term.push_input(bytes);
        self.settle();
    }

    fn click(&mut self, x: i32, y: i32) {
        self.input(format!("\x1b[<0;{};{}M", x + 1, y + 1).as_bytes());
        self.input(format!("\x1b[<0;{};{}m", x + 1, y + 1).as_bytes());
    }

    fn screen(&self) -> String {
        self.term.screen().to_text()
    }

    /// The owned popup's layer bounds (highest-z modal tree).
    fn popup_bounds(&self) -> Option<Rect> {
        let store = self.overlays.store().borrow();
        store
            .meta
            .iter()
            .zip(&store.layers)
            .filter(|(m, _)| matches!(m.content, OverlayContent::Tree { modal: true, .. }))
            .max_by_key(|(_, l)| l.z())
            .map(|(_, l)| l.bounds())
    }

    /// Accessibility text of the topmost modal overlay tree.
    fn popup_access(&self) -> Option<String> {
        let tree = {
            let store = self.overlays.store().borrow();
            store.meta.iter().rev().find_map(|m| match &m.content {
                OverlayContent::Tree {
                    tree, modal: true, ..
                } => Some(tree.handle()),
                _ => None,
            })?
        };
        let mut tree = tree.handle();
        tree.layout();
        Some(tree.accessibility_tree_text())
    }
}

/// Shared change-log capture for `on_change` assertions.
fn seen_log() -> (Rc<RefCell<Vec<String>>>, impl FnMut(&str) + 'static) {
    let seen: Rc<RefCell<Vec<String>>> = Default::default();
    let s = seen.clone();
    (seen, move |v: &str| s.borrow_mut().push(v.to_string()))
}

// ------------------------------------------------- the capable state

#[test]
fn capable_offers_declared_levels_only() {
    let (seen, log) = seen_log();
    let mut rig = rig(VP, move |cx| {
        ReasoningSelect::new(ReasoningFacts::capable(["low", "medium", "high"]))
            .on_change(log)
            .view(cx)
    });
    rig.settle();
    assert!(
        rig.screen().contains("r: auto"),
        "uncontrolled default value is auto:\n{}",
        rig.screen()
    );
    rig.input(b"\t"); // focus
    rig.input(b"\r"); // open
    let popup = rig.popup_bounds().expect("popup open");
    assert_eq!(popup.h, 5, "auto + none + the three declared levels");
    let access = rig.popup_access().expect("popup access");
    for row in ["auto", "none", "low", "medium", "high"] {
        assert!(
            access.contains(&format!("menuitem \"{row}\"")),
            "offers {row}:\n{access}"
        );
    }
    for absent in ["minimal", "xhigh"] {
        assert!(
            !access.contains(absent),
            "NEVER offers undeclared ladder steps ({absent}):\n{access}"
        );
    }
    // Down to "low" (auto -> none -> low), Enter commits.
    rig.input(b"\x1b[B\x1b[B");
    rig.input(b"\r");
    assert!(rig.popup_bounds().is_none(), "commit closes");
    assert_eq!(*seen.borrow(), vec!["low"], "on_change fired exactly once");
    assert!(
        rig.screen().contains("r: low"),
        "trigger renders the committed value:\n{}",
        rig.screen()
    );
}

#[test]
fn capable_empty_levels_offers_auto_and_none_only() {
    let mut rig = rig(VP, |cx| {
        ReasoningSelect::new(ReasoningFacts::capable(Vec::<String>::new())).view(cx)
    });
    rig.settle();
    rig.input(b"\t");
    rig.input(b"\r");
    let popup = rig.popup_bounds().expect("popup open");
    assert_eq!(
        popup.h, 2,
        "a capable model with no declared levels still offers auto/none"
    );
}

#[test]
fn capable_unknown_level_strings_render_verbatim_and_dedup() {
    // The thin-client honesty decision: the gateway is the authority —
    // an undeclared-in-our-ladder string ("ultrathink") is offered
    // verbatim, duplicates collapse, and a declared "auto"/"none"
    // never doubles the structural rows.
    let mut rig = rig(VP, |cx| {
        ReasoningSelect::new(ReasoningFacts::capable([
            "ultrathink",
            "high",
            "high",
            "auto",
            "",
        ]))
        .view(cx)
    });
    rig.settle();
    rig.input(b"\t");
    rig.input(b"\r");
    let popup = rig.popup_bounds().expect("popup open");
    assert_eq!(popup.h, 4, "auto, none, ultrathink, high — deduped");
    let access = rig.popup_access().expect("access");
    assert!(access.contains("menuitem \"ultrathink\""), "{access}");
    assert_eq!(access.matches("menuitem \"auto\"").count(), 1, "{access}");
    assert_eq!(access.matches("menuitem \"high\"").count(), 1, "{access}");
    // And it commits like any declared row (type-ahead included).
    rig.input(b"u");
    rig.input(b"\r");
    assert!(rig.screen().contains("r: ultrathink"), "{}", rig.screen());
}

#[test]
fn commit_fires_once_and_same_value_commit_is_silent() {
    let (seen, log) = seen_log();
    let mut rig = rig(VP, move |cx| {
        ReasoningSelect::new(ReasoningFacts::capable(["high"]))
            .on_change(log)
            .view(cx)
    });
    rig.settle();
    rig.input(b"\t");
    // Committing the highlighted current value (auto): silent close
    // (the select-family 0250 rule; field-gateway 0905 records the
    // recommit gap family-wide).
    rig.input(b"\r");
    rig.input(b"\r");
    assert!(rig.popup_bounds().is_none());
    assert!(seen.borrow().is_empty(), "same-value commit stays silent");
    // A changing commit fires exactly once.
    rig.input(b"\r");
    rig.input(b"h"); // type-ahead -> high
    rig.input(b"\r");
    assert_eq!(*seen.borrow(), vec!["high"]);
    // Escape never fires.
    rig.input(b"\r");
    rig.input(b"\x1b[B");
    rig.input(b"\x1b[27u");
    assert!(rig.popup_bounds().is_none(), "Escape closes");
    assert_eq!(seen.borrow().len(), 1, "Escape commits nothing");
    assert!(rig.screen().contains("r: high"), "{}", rig.screen());
}

#[test]
fn controlled_value_signal_is_respected_and_written() {
    let holder: Rc<RefCell<Option<Signal<String>>>> = Default::default();
    let h = holder.clone();
    let mut rig = rig(VP, move |cx| {
        let value = cx.signal(String::from("medium"));
        *h.borrow_mut() = Some(value);
        ReasoningSelect::new(ReasoningFacts::capable(["medium", "high"]))
            .value(value)
            .view(cx)
    });
    rig.settle();
    assert!(rig.screen().contains("r: medium"), "{}", rig.screen());
    rig.input(b"\t");
    rig.input(b"\r");
    rig.input(b"h");
    rig.input(b"\r");
    let value = holder.borrow().expect("signal");
    assert_eq!(value.get_untracked(), "high", "commit writes the binding");
    // External writes re-render the trigger (controlled mode).
    value.set("none".into());
    rig.settle();
    assert!(rig.screen().contains("r: none"), "{}", rig.screen());
}

// Locked/unknown-state flows, the modal anchor pin, a11y and idle —
// split sibling (file-size discipline; the select_tests_handle.rs
// pattern: the shared Rig lives in this parent module).
#[path = "reasoning_tests_states.rs"]
mod states;
