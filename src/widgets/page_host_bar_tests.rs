//! `plan_bar` geometry pins (`#[path]` sibling of
//! `page_host_tests.rs`, file-size discipline): the ONE plan the draw
//! closure and the click hit-test both consume — fits, windowing
//! stickiness, oversized-tab clamping, degenerate widths.

use super::super::bar::{hit_bar, plan_bar, BarHit, BarItem, BarModel};

fn model(titles: &[&str], active: usize) -> BarModel {
    BarModel {
        items: titles
            .iter()
            .map(|t| BarItem {
                title: (*t).to_string(),
                badge: None,
            })
            .collect(),
        active,
    }
}

#[test]
fn plan_bar_lays_everything_out_when_it_fits() {
    let m = model(&["one", "two"], 0);
    let plan = plan_bar(&m, 0, 30);
    assert!(!plan.overflow);
    assert_eq!(plan.segs.len(), 2);
    assert_eq!((plan.segs[0].x, plan.segs[0].w), (0, 5));
    assert_eq!((plan.segs[1].x, plan.segs[1].w), (6, 5));
    assert_eq!(hit_bar(&plan, 30, 7), BarHit::Tab(1));
    assert_eq!(hit_bar(&plan, 30, 12), BarHit::Miss);
}

#[test]
fn plan_bar_windows_around_the_active_tab_with_a_sticky_start() {
    let m = model(&["aaaa", "bbbb", "cccc", "dddd", "eeee", "ffff"], 0);
    // Six 6-wide segments + gaps = 41; avail 24 -> budget 20: 3 fit.
    let plan = plan_bar(&m, 0, 24);
    assert!(plan.overflow && !plan.left_more && plan.right_more);
    assert_eq!(plan.segs.len(), 3);
    assert_eq!(plan.first, 0);
    // Active moves within the window: the start STAYS (sticky).
    let m = model(&["aaaa", "bbbb", "cccc", "dddd", "eeee", "ffff"], 2);
    let plan = plan_bar(&m, 0, 24);
    assert_eq!(plan.first, 0);
    // Active leaves the window: the start slides minimally.
    let m = model(&["aaaa", "bbbb", "cccc", "dddd", "eeee", "ffff"], 4);
    let plan = plan_bar(&m, 0, 24);
    assert_eq!(plan.first, 2);
    assert!(plan.left_more && plan.right_more);
    // Indicators hit-map to prev/next only when something is hidden.
    assert_eq!(hit_bar(&plan, 24, 0), BarHit::Prev);
    assert_eq!(hit_bar(&plan, 24, 23), BarHit::Next);
}

#[test]
fn plan_bar_clamps_a_single_oversized_tab_into_the_budget() {
    let m = model(&["a very long page title indeed", "ok"], 0);
    let plan = plan_bar(&m, 0, 20);
    assert!(plan.overflow);
    let seg = &plan.segs[0];
    assert!(
        seg.w <= 16,
        "seg fits the indicator-reserved budget: {}",
        seg.w
    );
    assert!(seg.title_w < 29, "title took the cut");
}

#[test]
fn plan_bar_handles_empty_and_degenerate_widths() {
    let m = model(&[], 0);
    let plan = plan_bar(&m, 0, 20);
    assert!(plan.segs.is_empty());
    let m = model(&["one"], 0);
    let plan = plan_bar(&m, 0, 0);
    assert!(plan.segs.is_empty());
}
