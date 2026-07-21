//! REDTEAM cycle-4 attack: image protocol lifecycle. The KittyModel
//! referees GFX3D's emitters today (transmit/place/delete accounting,
//! chunking, tmux wrapping) and ImageSession the moment it lands.

use abstracttui::base::{Point, Rect, Rgba};
use abstracttui::gfx::bitmap::Bitmap;
use abstracttui::gfx::proto::kitty;
use abstracttui::gfx::{ExternalSink, ImageSession, SyncOutcome};
use abstracttui::term::caps::GraphicsCaps;
use abstracttui::testing::{unwrap_tmux, KittyModel};

fn img(w: u32, h: u32) -> Bitmap {
    Bitmap::from_fn(w, h, |x, y| Rgba::rgb((x * 3) as u8, (y * 5) as u8, 128))
}

/// An `ExternalSink` that feeds every byte the session emits straight
/// into the id-accounting `KittyModel` — the referee sees exactly what a
/// terminal would receive.
struct ModelSink {
    model: KittyModel,
    writes: usize,
}

impl ModelSink {
    fn new() -> ModelSink {
        ModelSink {
            model: KittyModel::new(),
            writes: 0,
        }
    }
    fn tmux() -> ModelSink {
        ModelSink {
            model: KittyModel::with_tmux_unwrap(),
            writes: 0,
        }
    }
}

impl ExternalSink for ModelSink {
    fn external_write(&mut self, bytes: &[u8], _at: Point) {
        self.writes += 1;
        self.model.feed(bytes);
    }
}

fn kitty_caps() -> GraphicsCaps {
    GraphicsCaps {
        wrap: None,
        kitty_graphics: true,
        iterm2_images: false,
        sixel: false,
        sixel_max_registers: None,
        cell_pixel_size: None,
    }
}

// ---------------------------------------------------------------------------
// Emitter-level lifecycle through the model.
// ---------------------------------------------------------------------------

#[test]
fn transmit_place_move_delete_accounting_via_emitters() {
    let mut model = KittyModel::new();
    let opts = kitty::Options {
        id: 31,
        ..kitty::Options::default()
    };
    // Transmit + display once.
    model.feed(&kitty::transmit_display(&img(64, 48), &opts));
    assert_eq!(model.transmit_count(31), 1);
    assert!(!model.image(31).unwrap().data_freed);

    // "Move" = re-place by id: transmit count must NOT grow.
    model.feed(&kitty::place(31, Some(10), Some(5), 0));
    model.feed(&kitty::place(31, Some(12), Some(6), 0));
    assert_eq!(model.transmit_count(31), 1, "moves must never re-transmit");
    assert!(model.image(31).unwrap().placements >= 2);

    // Drop: delete-by-id freeing data. No leaks left.
    model.feed(&kitty::delete_by_id(31, true));
    assert!(
        model.live_data_ids().is_empty(),
        "transmitted data must be freed on drop"
    );
    assert!(model.placed_ids().is_empty());
    assert!(
        model.violations.is_empty(),
        "protocol violations: {:?}",
        model.violations
    );
}

#[test]
fn big_image_chunking_reassembles_as_one_transmit() {
    let mut model = KittyModel::new();
    let opts = kitty::Options {
        id: 77,
        ..kitty::Options::default()
    };
    // 128x128 RGBA ~ 64 KB raw -> many chunks (multi-frame APC).
    model.feed(&kitty::transmit_display(&img(128, 128), &opts));
    assert_eq!(
        model.transmit_count(77),
        1,
        "chunked transmit must count once"
    );
    assert!(model.violations.is_empty(), "{:?}", model.violations);
}

#[test]
fn interleaved_images_and_cell_traffic() {
    // Two images + presenter-style cell bytes interleaved: accounting
    // stays per-id, cell bytes pass through untouched.
    let mut model = KittyModel::new();
    let a = kitty::Options {
        id: 1,
        ..kitty::Options::default()
    };
    let b = kitty::Options {
        id: 2,
        ..kitty::Options::default()
    };
    model.feed(b"\x1b[2J\x1b[1;1Hdashboard ");
    model.feed(&kitty::transmit_display(&img(16, 16), &a));
    model.feed(b"\x1b[5;1Hmiddle text");
    model.feed(&kitty::transmit_display(&img(24, 24), &b));
    model.feed(&kitty::delete_by_id(1, true));
    assert_eq!(model.live_data_ids(), vec![2], "only image 2 remains live");
    assert!(model.violations.is_empty(), "{:?}", model.violations);
}

// ---------------------------------------------------------------------------
// tmux passthrough correctness.
// ---------------------------------------------------------------------------

/// Wrap an emission the way a tmux-aware writer must (ESC doubled inside
/// `ESC P tmux; ... ESC \`), then verify the model unwraps to the exact
/// original bytes and the lifecycle accounting is unchanged.
fn tmux_wrap(bytes: &[u8]) -> Vec<u8> {
    let mut out = b"\x1bPtmux;".to_vec();
    for &b in bytes {
        if b == 0x1b {
            out.push(0x1b);
        }
        out.push(b);
    }
    out.extend_from_slice(b"\x1b\\");
    out
}

#[test]
fn tmux_wrapped_payloads_unwrap_byte_identical() {
    let opts = kitty::Options {
        id: 9,
        ..kitty::Options::default()
    };
    let plain = kitty::transmit_display(&img(32, 32), &opts);
    let wrapped = tmux_wrap(&plain);
    assert_eq!(
        unwrap_tmux(&wrapped),
        plain,
        "unwrap(wrap(x)) must be byte-exact"
    );

    // Same accounting through the wrapped path.
    let mut direct = KittyModel::new();
    direct.feed(&plain);
    let mut via_tmux = KittyModel::with_tmux_unwrap();
    via_tmux.feed(&wrapped);
    assert_eq!(direct.transmit_count(9), via_tmux.transmit_count(9));
    assert_eq!(direct.live_data_ids(), via_tmux.live_data_ids());
    assert!(via_tmux.violations.is_empty(), "{:?}", via_tmux.violations);
}

#[test]
fn truncated_tmux_wrapper_loses_no_bytes() {
    let opts = kitty::Options {
        id: 4,
        ..kitty::Options::default()
    };
    let plain = kitty::transmit_display(&img(8, 8), &opts);
    let mut wrapped = tmux_wrap(&plain);
    wrapped.truncate(wrapped.len() - 2); // cut the ST
    let out = unwrap_tmux(&wrapped);
    // Never silently drop the tail: the raw wrapper surfaces instead.
    assert!(out.len() >= wrapped.len() - 8);
}

// ---------------------------------------------------------------------------
// RT4-1: ImageSession lifecycle through the real session API, refereed by
// the KittyModel. This is the authored version of the reserved
// placeholder (REACT flagged the `unreachable!()` body cycle 4). The
// session's own unit tests assert on byte substrings; this asserts on
// MODELED id state — transmit-once, no-retransmit-on-move,
// retransmit-frees-old-on-version-bump, delete-on-remove, no leaks.
// ---------------------------------------------------------------------------

/// The whole lifecycle in one slot: transmit → unchanged → move (no
/// retransmit) → version bump (old id freed, new id transmitted) →
/// release (all data freed, nothing left placed).
#[test]
fn image_session_lifecycle_no_leaks() {
    let mut session = ImageSession::new();
    let mut sink = ModelSink::new();
    let caps = kitty_caps();
    let slot = 42u64;
    let pic = img(48, 32);

    // Frame 1: first show → a full transmit+display of exactly one id.
    let out = session.sync(&mut sink, slot, 1, &pic, Rect::new(2, 2, 10, 5), &caps);
    assert!(
        matches!(out, SyncOutcome::Emitted(_)),
        "first show must emit"
    );
    let live = sink.model.live_data_ids();
    assert_eq!(live.len(), 1, "exactly one image transmitted; got {live:?}");
    let id_v1 = live[0];
    assert_eq!(sink.model.transmit_count(id_v1), 1);

    // Frame 2: identical version + rect → the session stays silent and
    // the terminal state is untouched.
    let out = session.sync(&mut sink, slot, 1, &pic, Rect::new(2, 2, 10, 5), &caps);
    assert!(
        matches!(out, SyncOutcome::Unchanged),
        "no-op frame must be silent"
    );
    assert_eq!(
        sink.model.transmit_count(id_v1),
        1,
        "silent frame must not transmit"
    );

    // Frame 3..5: move the image three times (same version, new rects).
    // Placement escapes only — the transmit count for the id must NEVER
    // grow, and no new id may appear.
    for (i, rect) in [(20, 3, 10, 5), (30, 8, 8, 4), (1, 1, 12, 6)]
        .into_iter()
        .map(|(x, y, w, h)| (x, Rect::new(x, y, w, h)))
        .enumerate()
    {
        let out = session.sync(&mut sink, slot, 1, &pic, rect.1, &caps);
        assert!(
            matches!(out, SyncOutcome::Emitted(_)),
            "move {i} emits a placement"
        );
        assert_eq!(
            sink.model.transmit_count(id_v1),
            1,
            "move {i} RETRANSMITTED (id {id_v1}) — the move-is-free contract is broken"
        );
        let ids: Vec<u32> = sink.model.live_data_ids();
        assert_eq!(ids, vec![id_v1], "move {i} minted a new id {ids:?}");
    }

    // Frame 6: content version bump → the old upload must be FREED and a
    // fresh id transmitted (unbounded terminal memory otherwise).
    let pic2 = img(48, 32); // "new pixels" — version is the signal
    let out = session.sync(&mut sink, slot, 2, &pic2, Rect::new(1, 1, 12, 6), &caps);
    assert!(
        matches!(out, SyncOutcome::Emitted(_)),
        "version bump must emit"
    );
    let live = sink.model.live_data_ids();
    assert_eq!(
        live.len(),
        1,
        "exactly one live id after re-version; got {live:?}"
    );
    let id_v2 = live[0];
    assert_ne!(id_v2, id_v1, "version bump must mint a NEW id");
    assert!(
        sink.model
            .image(id_v1)
            .map(|s| s.data_freed)
            .unwrap_or(true),
        "old upload {id_v1} was NOT freed on version bump — terminal-memory leak"
    );

    // Release: the live upload dies; nothing left placed or live.
    session.release(&mut sink, slot, &caps);
    assert_eq!(
        session.live_slots(),
        0,
        "session still tracks a released slot"
    );
    assert!(
        sink.model.live_data_ids().is_empty(),
        "release left live data: {:?}",
        sink.model.live_data_ids()
    );
    assert!(
        sink.model.placed_ids().is_empty(),
        "release left a placement"
    );
    assert!(
        sink.model.violations.is_empty(),
        "protocol violations: {:?}",
        sink.model.violations
    );
}

/// Two slots, then `release_all` (screen clear / shutdown): every
/// transmitted id must be freed — no orphan uploads survive teardown.
#[test]
fn image_session_release_all_frees_every_id() {
    let mut session = ImageSession::new();
    let mut sink = ModelSink::new();
    let caps = kitty_caps();

    session.sync(&mut sink, 1, 1, &img(16, 16), Rect::new(0, 0, 4, 2), &caps);
    session.sync(&mut sink, 2, 1, &img(24, 24), Rect::new(6, 0, 5, 3), &caps);
    assert_eq!(sink.model.live_data_ids().len(), 2, "two uploads expected");

    session.release_all(&mut sink, &caps);
    assert_eq!(session.live_slots(), 0);
    assert!(
        sink.model.live_data_ids().is_empty(),
        "release_all leaked: {:?}",
        sink.model.live_data_ids()
    );
    assert!(
        sink.model.violations.is_empty(),
        "{:?}",
        sink.model.violations
    );
}

/// Version bump held to account precisely: exactly N distinct ids over N
/// content versions, and at end-of-life only the last is live before
/// release. Catches an "id reuse that skips the free" regression.
#[test]
fn image_session_version_churn_frees_each_superseded_upload() {
    let mut session = ImageSession::new();
    let mut sink = ModelSink::new();
    let caps = kitty_caps();
    let slot = 5u64;
    let rect = Rect::new(0, 0, 8, 4);

    let mut seen_ids = std::collections::BTreeSet::new();
    for version in 1..=6u64 {
        session.sync(&mut sink, slot, version, &img(20, 20), rect, &caps);
        let live = sink.model.live_data_ids();
        assert_eq!(
            live.len(),
            1,
            "version {version}: exactly one live upload, got {live:?}"
        );
        seen_ids.insert(live[0]);
    }
    assert_eq!(
        seen_ids.len(),
        6,
        "six versions must produce six distinct ids: {seen_ids:?}"
    );

    session.release(&mut sink, slot, &caps);
    assert!(
        sink.model.live_data_ids().is_empty(),
        "final release must free the last upload"
    );
    assert!(
        sink.model.violations.is_empty(),
        "{:?}",
        sink.model.violations
    );
}

/// The same lifecycle under tmux passthrough: every session-authored
/// escape (transmit, placement, delete) must be wrapped, and the model
/// unwraps to identical accounting — no leak, no violation.
#[test]
fn image_session_lifecycle_under_tmux_wrap() {
    use abstracttui::term::caps::WrapKind;
    let mut session = ImageSession::new();
    let mut sink = ModelSink::tmux();
    let caps = GraphicsCaps {
        wrap: Some(WrapKind::Tmux),
        ..kitty_caps()
    };
    let slot = 8u64;

    session.sync(
        &mut sink,
        slot,
        1,
        &img(32, 32),
        Rect::new(0, 0, 6, 3),
        &caps,
    );
    session.sync(
        &mut sink,
        slot,
        1,
        &img(32, 32),
        Rect::new(4, 4, 6, 3),
        &caps,
    ); // move
    session.sync(
        &mut sink,
        slot,
        2,
        &img(32, 32),
        Rect::new(4, 4, 6, 3),
        &caps,
    ); // re-version
    session.release(&mut sink, slot, &caps);

    assert!(
        sink.model.live_data_ids().is_empty(),
        "tmux-wrapped lifecycle leaked: {:?}",
        sink.model.live_data_ids()
    );
    assert!(
        sink.model.violations.is_empty(),
        "tmux-wrapped escapes failed to unwrap cleanly: {:?}",
        sink.model.violations
    );
    // A move happened, so more than the two transmits were written.
    assert!(
        sink.writes >= 4,
        "expected transmit+move+reversion(free+transmit)+release writes"
    );
}
