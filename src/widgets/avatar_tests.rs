use super::*;
use crate::{
    base::Size,
    reactive::{flush_effects, next_timer_deadline, run_due_timers},
    widgets::itest_util::{mount_widget, settle},
};
use std::{cell::RefCell, rc::Rc, time::Instant};

#[test]
fn activity_clock_disarms_when_idle_hidden_or_reduced_motion() {
    let shared = Rc::new(RefCell::new(None));
    let out = shared.clone();
    let (root, mut tree) = mount_widget(Size::new(18, 10), move |cx| {
        let pose = cx.signal(AvatarPose::Ready);
        let active = cx.signal(true);
        let animated = cx.signal(true);
        *out.borrow_mut() = Some((pose, active, animated));
        Avatar::new(pose).active(active).animated(animated).view(cx)
    });
    settle(&mut tree, Size::new(18, 10));
    let (pose, active, animated) = shared.borrow().unwrap();
    assert!(next_timer_deadline().is_none(), "Ready must not animate");
    pose.set(AvatarPose::Thinking);
    flush_effects();
    let due = next_timer_deadline().expect("Thinking should arm one clock");
    assert_eq!(run_due_timers(due), 1);
    assert!(next_timer_deadline().unwrap() > due);
    active.set(false);
    flush_effects();
    assert!(
        next_timer_deadline().is_none(),
        "hidden stops the pending timer, not just its callback"
    );
    assert_eq!(run_due_timers(Instant::now() + Duration::from_secs(10)), 0);
    active.set(true);
    flush_effects();
    assert!(next_timer_deadline().is_some());
    animated.set(false);
    flush_effects();
    assert!(
        next_timer_deadline().is_none(),
        "reduced motion stops the clock"
    );
    animated.set(true);
    flush_effects();
    assert!(next_timer_deadline().is_some());
    for p in [AvatarPose::Ready, AvatarPose::Waiting, AvatarPose::Error] {
        pose.set(p);
        flush_effects();
        assert!(next_timer_deadline().is_none(), "{p:?}");
    }
    pose.set(AvatarPose::Working);
    flush_effects();
    assert!(next_timer_deadline().is_some());
    root.dispose();
    assert!(
        next_timer_deadline().is_none(),
        "scope disposal cancels the timer"
    );
}

#[test]
fn all_poses_and_styles_render_in_narrow_and_large_rectangles() {
    for size in [
        Size::new(1, 1),
        Size::new(4, 2),
        Size::new(18, 10),
        Size::new(80, 30),
    ] {
        for style in [AvatarStyle::Halo, AvatarStyle::Hover] {
            for pose in [
                AvatarPose::Ready,
                AvatarPose::Thinking,
                AvatarPose::Working,
                AvatarPose::Listening,
                AvatarPose::Speaking,
                AvatarPose::Waiting,
                AvatarPose::Error,
            ] {
                let (root, mut tree) = mount_widget(size, move |cx| {
                    Avatar::new(cx.signal(pose))
                        .style(style)
                        .level(cx.signal(1.0))
                        .layout(
                            LayoutStyle::default()
                                .width(Dimension::Percent(1.0))
                                .height(Dimension::Percent(1.0)),
                        )
                        .view(cx)
                });
                let canvas = settle(&mut tree, size);
                if size.w >= 18 {
                    assert!(
                        canvas.row_text(size.h - 1).contains(pose.label()),
                        "{size:?}/{style:?}/{pose:?}"
                    );
                }
                assert!(next_timer_deadline().is_none(), "animation is opt-in");
                root.dispose();
            }
        }
    }
}

#[test]
fn level_and_theme_change_art_without_mutating_the_theme() {
    let t = crate::theme::default_theme().tokens;
    for style in [AvatarStyle::Halo, AvatarStyle::Hover] {
        let image = |level| {
            (0..80)
                .flat_map(|y| {
                    (0..80).map(move |x| {
                        sample(
                            style,
                            AvatarPose::Speaking,
                            level,
                            0.0,
                            (x as f32 - 40.0) / 40.0,
                            (y as f32 - 40.0) / 40.0,
                            &t,
                        )
                    })
                })
                .collect::<Vec<_>>()
        };
        assert_ne!(image(0.0), image(1.0));
    }
    assert_eq!(normalized(f32::NAN), 0.0);
    assert_eq!(normalized(f32::INFINITY), 0.0);
    assert_eq!(normalized(-1.0), 0.0);
    assert_eq!(normalized(9.0), 1.0);
    assert_eq!(ink(AvatarPose::Error, &t), t.error);
    assert_eq!(ink(AvatarPose::Waiting, &t), t.warn);
}

#[test]
fn disabled_avatar_draws_nothing_and_keeps_no_clock() {
    let size = Size::new(18, 10);
    let (root, mut tree) = mount_widget(size, |cx| {
        Avatar::new(cx.signal(AvatarPose::Speaking))
            .active(cx.signal(false))
            .animated(cx.signal(true))
            .view(cx)
    });
    let canvas = settle(&mut tree, size);
    assert!((0..size.h).all(|y| canvas.row_text(y).trim().is_empty()));
    assert!(next_timer_deadline().is_none());
    root.dispose();
}
