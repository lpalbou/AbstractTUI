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
    let materials_value = Materials::new(&t);
    let materials = &materials_value;
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
                            materials,
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
    assert_ne!(
        materials.warm.sample(0.65, 1.0),
        materials.eye.sample(0.65, 1.0)
    );
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

#[test]
fn audio_producer_updates_and_equal_state_writes_cannot_starve_or_accelerate_the_clock() {
    use crate::{
        app::{App, Driver, RunConfig},
        reactive::{batch, set_loop_clock},
        testing::CaptureTerm,
    };
    for producer_ms in [20u64, 50] {
        let size = Size::new(18, 9);
        let t0 = Instant::now();
        set_loop_clock(Some(t0));
        let state = Rc::new(RefCell::new(None));
        let out = state.clone();
        let mut app = App::new(size);
        app.mount(move |cx| {
            let pose = cx.signal(AvatarPose::Speaking);
            let active = cx.signal(true);
            let enabled = cx.signal(true);
            let level = cx.signal(0.0f32);
            *out.borrow_mut() = Some((pose, active, enabled, level));
            Avatar::new(pose)
                .active(active)
                .animated(enabled)
                .level(level)
                .view(cx)
        })
        .unwrap();
        let now = Rc::new(std::cell::Cell::new(t0));
        let clock = now.clone();
        let mut term = CaptureTerm::new(size);
        let mut driver = Driver::new(
            &mut app,
            &mut term,
            RunConfig {
                probe: false,
                ..Default::default()
            },
        )
        .unwrap();
        driver.set_clock(move || clock.get());
        for _ in 0..3 {
            driver.turn(&mut app, &mut term).unwrap();
        }
        term.take_bytes();
        let first_deadline = next_timer_deadline().unwrap();
        let (pose, active, enabled, level) = state.borrow().unwrap();
        let mut emitted = 0;
        for ms in 1..=6000u64 {
            now.set(t0 + Duration::from_millis(ms));
            set_loop_clock(Some(now.get()));
            if ms % producer_ms == 0 {
                batch(|| {
                    pose.set(AvatarPose::Speaking);
                    active.set(true);
                    enabled.set(true);
                    level.set(if ms % (producer_ms * 2) == 0 {
                        0.9
                    } else {
                        0.1
                    });
                });
            }
            if ms == producer_ms {
                assert_eq!(
                    next_timer_deadline(),
                    Some(first_deadline),
                    "producer must not postpone the first frame"
                );
            }
            if driver.turn(&mut app, &mut term).unwrap().emitted {
                emitted += 1;
            }
        }
        assert!((60..=72).contains(&emitted),"producer {producer_ms}ms: {emitted} emitted frames; producer cadence must be coalesced to the activity clock");
        active.set(false);
        driver.turn(&mut app, &mut term).unwrap();
        term.take_bytes();
        assert!(next_timer_deadline().is_none());
        for ms in 6001..=6200 {
            now.set(t0 + Duration::from_millis(ms));
            level.set(ms as f32);
            pose.set(AvatarPose::Speaking);
            active.set(false);
            enabled.set(true);
            driver.turn(&mut app, &mut term).unwrap();
        }
        assert!(
            term.bytes().is_empty(),
            "hidden avatars must not repaint when producer data changes"
        );
        assert!(next_timer_deadline().is_none());
        drop(driver);
        drop(app);
        set_loop_clock(None);
    }
}

#[test]
fn shaded_scene_ground_is_opaque_and_surfaces_have_continuous_tonal_detail() {
    let tokens = crate::theme::default_theme().tokens;
    let materials = Materials::new(&tokens);
    for style in [AvatarStyle::Halo, AvatarStyle::Hover] {
        assert_eq!(
            sample(style, AvatarPose::Ready, 0.0, 0.0, -0.99, -0.99, &materials),
            tokens.bg
        );
        let mut shades = std::collections::HashSet::new();
        for y in 0..96 {
            for x in 0..96 {
                let color = sample(
                    style,
                    AvatarPose::Ready,
                    0.0,
                    0.0,
                    (x as f32 - 48.0) / 48.0,
                    (y as f32 - 48.0) / 48.0,
                    &materials,
                );
                assert_eq!(
                    color.a, 255,
                    "half-block edges must not turn alpha-zero ink into default white"
                );
                shades.insert(color);
            }
        }
        assert!(
            shades.len() > 40,
            "material should retain smooth tonal shading, not a handful of flat token bands"
        );
    }
}
