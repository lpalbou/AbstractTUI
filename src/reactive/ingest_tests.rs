//! Unit tests for the bounded ingestion lane (sibling file, the
//! widgets `feed_tests.rs` pattern — keeps ingest.rs under the
//! 600-line bar). Integration coverage: tests/wave_livedata.rs.

use super::*;
use crate::reactive::{create_root, drain_posted};

#[test]
fn under_capacity_everything_delivers_in_order() {
    let (root, ()) = create_root(|cx| {
        let (tx, events, stats) = bounded_source(cx, 100, OverflowPolicy::DropOldest);
        std::thread::spawn(move || {
            for n in 0..50u32 {
                tx.send(n);
            }
        })
        .join()
        .expect("producer");
        drain_posted();
        assert_eq!(events.get_untracked(), (0..50).collect::<Vec<_>>());
        assert_eq!(
            stats.get_untracked(),
            IngestStats {
                delivered: 50,
                dropped: 0,
                coalesced: 0,
                fold_panics: 0
            }
        );
    });
    root.dispose();
}

#[test]
fn drop_oldest_keeps_the_newest_tail_and_counts_exactly() {
    let (root, ()) = create_root(|cx| {
        let (tx, events, stats) = bounded_source(cx, 8, OverflowPolicy::DropOldest);
        for n in 0..20u32 {
            tx.send(n);
        }
        drain_posted();
        assert_eq!(events.get_untracked(), (12..20).collect::<Vec<_>>());
        let s = stats.get_untracked();
        assert_eq!(s.dropped, 12, "every eviction counted");
        assert_eq!(s.delivered + s.dropped, 20, "accounting is total");
    });
    root.dispose();
}

#[test]
fn drop_newest_keeps_the_head_and_counts_refusals() {
    let (root, ()) = create_root(|cx| {
        let (tx, events, stats) = bounded_source(cx, 8, OverflowPolicy::DropNewest);
        for n in 0..20u32 {
            tx.send(n);
        }
        drain_posted();
        assert_eq!(events.get_untracked(), (0..8).collect::<Vec<_>>());
        let s = stats.get_untracked();
        assert_eq!(s.delivered, 8);
        assert_eq!(s.dropped, 12);
    });
    root.dispose();
}

#[test]
fn coalesce_merges_overflow_and_final_state_is_last_writer() {
    let (root, ()) = create_root(|cx| {
        let (tx, events, stats) = bounded_source(
            cx,
            4,
            OverflowPolicy::coalesce(|kept: &mut u32, new| {
                *kept = new; // supersede: last writer wins
            }),
        );
        for n in 0..12u32 {
            tx.send(n);
        }
        drain_posted();
        // First three admitted untouched; the fourth slot absorbed
        // every later value, ending at the last writer.
        assert_eq!(events.get_untracked(), vec![0, 1, 2, 11]);
        let s = stats.get_untracked();
        assert_eq!(s.coalesced, 8, "merged values are counted, not lost");
        assert_eq!(s.dropped, 0, "coalesce never drops");
        assert_eq!(s.delivered, 4);
    });
    root.dispose();
}

#[test]
fn burst_schedules_exactly_one_drain_closure() {
    let (root, ()) = create_root(|cx| {
        let (tx, events, _) = bounded_source(cx, 1024, OverflowPolicy::DropOldest);
        std::thread::spawn(move || {
            for n in 0..1000u32 {
                tx.send(n);
            }
        })
        .join()
        .expect("producer");
        assert_eq!(drain_posted(), 1, "one posted job for the whole burst");
        assert_eq!(events.with_untracked(|v| v.len()), 1000);
    });
    root.dispose();
}

#[test]
fn sends_after_disposal_are_inert_bounded_and_counted() {
    let mut handles = None;
    let (root, ()) = create_root(|cx| {
        let child = cx.child();
        let (tx, events, stats) = bounded_source(child, 4, OverflowPolicy::DropOldest);
        child.dispose();
        handles = Some((tx, events, stats));
    });
    let (tx, events, stats) = handles.expect("handles");
    for n in 0..100u32 {
        tx.send(n); // transit stays bounded at capacity 4
    }
    drain_posted();
    assert!(!events.is_alive());
    assert!(!stats.is_alive());
    assert_eq!(
        tx.dead_sends(),
        4,
        "the bounded batch that reached the drain"
    );
    root.dispose();
}

#[test]
#[should_panic(expected = "capacity must be >= 1")]
fn zero_capacity_panics_loudly() {
    let (_root, ()) = create_root(|cx| {
        let _ = bounded_source::<u32>(cx, 0, OverflowPolicy::DropOldest);
    });
}

/// A fold panicking on the PRODUCER side (transit overflow) must
/// not poison the queue mutex: later sends work, the lost value is
/// counted dropped, the event counted fold_panics.
#[test]
fn transit_fold_panic_degrades_labeled_never_poisons() {
    let (root, ()) = create_root(|cx| {
        let (tx, events, stats) = bounded_source(
            cx,
            2,
            OverflowPolicy::coalesce(|kept: &mut u32, new| {
                assert!(new != 3, "fold bug on 3"); // the app's bug
                *kept = new;
            }),
        );
        tx.send(1);
        tx.send(2); // transit full
        tx.send(3); // fold panics: caught, counted, value lost
        tx.send(4); // MUST still work (the poison this fix removes)
        drain_posted();
        assert_eq!(events.get_untracked(), vec![1, 4], "later folds fine");
        assert_eq!(
            stats.get_untracked(),
            IngestStats {
                delivered: 2,
                dropped: 1,     // value 3 died inside the fold
                coalesced: 1,   // value 4 merged normally
                fold_panics: 1  // the labeled event
            }
        );
    });
    root.dispose();
}

/// Same firewall on the UI side (window overflow during a drain):
/// the drain completes, siblings in the batch still apply, stats
/// stay exact.
#[test]
fn window_fold_panic_degrades_labeled_and_drain_survives() {
    let (root, ()) = create_root(|cx| {
        let (tx, events, stats) = bounded_source(
            cx,
            2,
            OverflowPolicy::coalesce(|kept: &mut u32, new| {
                assert!(new != 3, "fold bug on 3");
                *kept = new;
            }),
        );
        tx.send(1);
        tx.send(2);
        drain_posted(); // window [1, 2] — full from now on
        tx.send(3); // will hit the WINDOW-stage fold and panic
        tx.send(4); // same drain, must still merge
        drain_posted();
        assert_eq!(events.get_untracked(), vec![1, 4]);
        assert_eq!(
            stats.get_untracked(),
            IngestStats {
                delivered: 2,
                dropped: 1,
                coalesced: 1,
                fold_panics: 1
            }
        );
        // The lane stays fully alive afterwards.
        tx.send(5);
        drain_posted();
        assert_eq!(events.get_untracked(), vec![1, 5]);
    });
    root.dispose();
}
