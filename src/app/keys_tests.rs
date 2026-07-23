//! Key-state service tests — split from `keys.rs` for the file-size
//! discipline (the feed/driver sibling-file pattern). Same module
//! semantics as the old inline block: `#[path]`-included as
//! `keys::tests`, so private items stay reachable.

use super::*;
use crate::input::{KeyCode, Mods as InMods};
use crate::reactive::flush_effects;

fn press(code: KeyCode) -> InKeyEvent {
    InKeyEvent::plain(code)
}

fn release(code: KeyCode) -> InKeyEvent {
    InKeyEvent::plain(code).with_kind(KeyEventKind::Release)
}

fn repeat(code: KeyCode) -> InKeyEvent {
    InKeyEvent::plain(code).with_kind(KeyEventKind::Repeat)
}

/// Reset this thread's store between assertions (tests share a
/// thread-immortal store the way caps tests do).
fn reset() {
    let s = store();
    s.enabled.set(false);
    *s.frame.borrow_mut() = Frame::default();
    publish_fidelity(false);
}

#[test]
fn disabled_store_ignores_everything() {
    reset();
    publish_fidelity(true);
    on_key_event(&press(KeyCode::Char('w')));
    begin_turn();
    on_focus_lost();
    let s = store();
    assert!(s.frame.borrow().pressed.is_empty());
    assert!(s.frame.borrow().down.is_empty());
    assert!(!s.frame.borrow().focus_cleared);
    reset();
}

#[test]
fn full_fidelity_tracks_chords_and_release() {
    reset();
    let ks = key_state();
    publish_fidelity(true);
    begin_turn();
    on_key_event(&press(KeyCode::Up));
    on_key_event(&press(KeyCode::Right));
    assert!(ks.is_down(Key::Up) && ks.is_down(Key::Right), "chord");
    assert!(ks.pressed(Key::Up) && ks.pressed_chord(KeyChord::plain(Key::Right)));
    assert_eq!(ks.keys_down().len(), 2);

    begin_turn(); // edges seal away, state persists
    assert!(!ks.pressed(Key::Up), "press edge sealed by next turn");
    assert!(ks.is_down(Key::Up), "hold persists across turns");

    on_key_event(&release(KeyCode::Up));
    assert!(!ks.is_down(Key::Up) && ks.is_down(Key::Right));
    assert!(ks.released(Key::Up));
    // Unmatched release: no-op, never a panic.
    on_key_event(&release(KeyCode::Delete));
    assert!(!ks.is_down(Key::Delete));
    reset();
}

#[test]
fn repeat_is_proof_of_down_but_not_a_press_edge() {
    reset();
    let ks = key_state();
    publish_fidelity(true);
    begin_turn();
    on_key_event(&repeat(KeyCode::Char('w')));
    assert!(ks.is_down(Key::Char('w')), "repeat proves the key is down");
    assert!(!ks.pressed(Key::Char('w')), "repeat is not a press edge");
    reset();
}

#[test]
fn degraded_never_claims_held_but_press_edges_stay_honest() {
    reset();
    let ks = key_state();
    publish_fidelity(false);
    begin_turn();
    on_key_event(&press(KeyCode::Char(' ')));
    assert!(
        ks.pressed(Key::Char(' ')),
        "press edges are real everywhere"
    );
    assert!(!ks.is_down(Key::Char(' ')), "Degraded never fakes a hold");
    // Even a stray release (a terminal speaking kitty unasked) adds
    // no state on a Degraded contract.
    on_key_event(&release(KeyCode::Char(' ')));
    assert!(!ks.released(Key::Char(' ')));
    assert_eq!(ks.fidelity_untracked(), KeyFidelity::Degraded);
    reset();
}

#[test]
fn focus_lost_clears_and_synthesizes_releases() {
    reset();
    let ks = key_state();
    publish_fidelity(true);
    begin_turn();
    on_key_event(&press(KeyCode::Char('w')));
    on_key_event(&press(KeyCode::Char('d')));
    begin_turn();
    on_focus_lost();
    assert!(!ks.any_down(), "focus loss empties the down-set");
    assert!(ks.released(Key::Char('w')) && ks.released(Key::Char('d')));
    assert!(ks.focus_cleared(), "synthesized releases are labeled");
    begin_turn();
    assert!(!ks.focus_cleared(), "the flag seals like any edge");
    reset();
}

#[test]
fn lock_mods_strip_for_chord_matching() {
    reset();
    let ks = key_state();
    publish_fidelity(true);
    begin_turn();
    on_key_event(&InKeyEvent::new(
        KeyCode::Char(' '),
        InMods::CTRL | InMods::CAPS_LOCK,
    ));
    assert!(ks.pressed_chord(KeyChord::ctrl(Key::Char(' '))));
    assert!(!ks.pressed_chord(KeyChord::plain(Key::Char(' '))));
    reset();
}

#[test]
fn generation_bumps_wake_subscribers_once_per_change() {
    reset();
    let ks = key_state();
    publish_fidelity(true);
    let (root, runs) = create_root(|cx| {
        let runs = cx.signal(0u32);
        cx.effect(move || {
            let _ = ks.any_down(); // subscribe through gen
            runs.update(|r| *r += 1);
        });
        runs
    });
    flush_effects();
    let base = runs.get_untracked();
    begin_turn(); // no edges pending: no bump
    flush_effects();
    assert_eq!(runs.get_untracked(), base, "quiet turn wakes nobody");
    on_key_event(&press(KeyCode::Char('x')));
    flush_effects();
    assert_eq!(runs.get_untracked(), base + 1, "press edge: one wake");
    begin_turn(); // falling edge of the pulse
    flush_effects();
    assert_eq!(runs.get_untracked(), base + 2, "edge clear: one wake");
    begin_turn();
    flush_effects();
    assert_eq!(runs.get_untracked(), base + 2, "then quiet again");
    root.dispose();
    reset();
}

#[test]
fn suspend_drains_the_down_set_and_labels_the_frame() {
    // Cycle-2 review I-2: a job-control suspend stops the process —
    // releases during the stop are unobservable, so the seam drains
    // held keys into synthesized releases BEFORE the stop (fail toward
    // not-held) and labels the frame so capture surfaces can stop
    // with a truthful reason.
    reset();
    let ks = key_state();
    publish_fidelity(true);
    begin_turn();
    on_key_event(&press(KeyCode::Char('w')));
    assert!(ks.is_down(Key::Char('w')), "precondition: held on Full");
    begin_turn();
    on_suspend();
    assert!(
        !ks.is_down(Key::Char('w')),
        "resumed state must read not-held"
    );
    assert!(
        ks.released(Key::Char('w')),
        "the drain synthesizes release edges (the focus-loss rule)"
    );
    assert!(ks.suspend_cleared(), "synthesized releases are labeled");
    assert!(!ks.focus_cleared(), "a suspend is not a focus event");
    begin_turn();
    assert!(!ks.suspend_cleared(), "the flag seals like any edge");
    // Empty down-set (a Degraded wire holds nothing): the flag still
    // fires — a latched capture must stop on suspend too.
    publish_fidelity(false);
    begin_turn();
    on_suspend();
    assert!(ks.suspend_cleared(), "flag fires with an empty down-set");
    reset();
}

#[test]
fn downgrade_drains_the_down_set_toward_not_held() {
    // Unreachable from today's driver (probes only upgrade) —
    // pinned so a future suspend/probe change cannot re-open the
    // stuck-hold class: on Degraded, releases never arrive, so a
    // key left in the down-set would read held FOREVER.
    reset();
    let ks = key_state();
    publish_fidelity(true);
    begin_turn();
    on_key_event(&press(KeyCode::Char('w')));
    assert!(ks.is_down(Key::Char('w')), "precondition: held on Full");
    publish_fidelity(false); // the hypothetical downgrade
    assert!(
        !ks.is_down(Key::Char('w')),
        "Degraded must never claim a hold it can no longer end"
    );
    assert!(
        ks.released(Key::Char('w')),
        "the drain synthesizes release edges (the focus-loss rule)"
    );
    assert!(!ks.focus_cleared(), "a downgrade is not a focus event");
    reset();
}

#[test]
fn gesture_label_is_truthful_per_fidelity() {
    let chord = KeyChord::plain(Key::Char(' '));
    assert_eq!(hold_gesture_label(KeyFidelity::Full, chord), "hold Space");
    assert_eq!(
        hold_gesture_label(KeyFidelity::Degraded, chord),
        "press Space to start/stop"
    );
}

#[test]
fn release_liveness_rule() {
    let caps_on = Capabilities::with(|c| c.kitty_keyboard = true);
    assert!(release_events_live(&caps_on, KittyFlags::standard()));
    assert!(
        !release_events_live(&caps_on, KittyFlags(0)),
        "protocol spoken but flags never pushed (the 0293 gap) = Degraded"
    );
    assert!(
        !release_events_live(&Capabilities::default(), KittyFlags::standard()),
        "flags pushed at a non-kitty terminal = Degraded"
    );
}
