use super::*;
use crate::{
    base::Size,
    reactive::{flush_effects, next_timer_deadline, run_due_timers},
    widgets::itest_util::{mount_widget, settle},
};
use std::{cell::RefCell, rc::Rc, time::Instant};

#[test]
fn activity_clock_uses_slow_idle_cadence_and_disarms_when_hidden_reduced_or_error() {
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
    let idle_deadline = next_timer_deadline().expect("opt-in Ready breathes gently");
    assert_eq!(run_due_timers(idle_deadline), 1);
    assert_eq!(
        next_timer_deadline().unwrap().duration_since(idle_deadline),
        Duration::from_millis(500),
        "idle must not repaint at active cadence"
    );
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
    pose.set(AvatarPose::Waiting);
    flush_effects();
    let waiting_deadline = next_timer_deadline().expect("Waiting pulses on its slower cadence");
    assert_eq!(run_due_timers(waiting_deadline), 1);
    assert_eq!(
        next_timer_deadline()
            .unwrap()
            .duration_since(waiting_deadline),
        Duration::from_millis(167)
    );
    pose.set(AvatarPose::Error);
    flush_effects();
    assert!(next_timer_deadline().is_none(), "Error stays static");
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

#[test]
fn hover_listens_with_a_cocked_posture_waits_still_and_works_with_a_larger_bob() {
    let tokens = crate::theme::default_theme().tokens;
    let materials = Materials::new(&tokens);
    let center = |pose, time| {
        let mut x_sum = 0.0;
        let mut y_sum = 0.0;
        let mut count = 0.0;
        for y in 0..96 {
            for x in 0..96 {
                if sample(
                    AvatarStyle::Hover,
                    pose,
                    0.5,
                    time,
                    (x as f32 - 48.0) / 48.0,
                    (y as f32 - 48.0) / 48.0,
                    &materials,
                ) != tokens.bg
                {
                    x_sum += x as f32;
                    y_sum += y as f32;
                    count += 1.0;
                }
            }
        }
        (x_sum / count, y_sum / count)
    };
    let ready = center(AvatarPose::Ready, 0.0);
    let listening = center(AvatarPose::Listening, 0.0);
    let waiting = center(AvatarPose::Waiting, 0.0);
    assert!(
        listening.0 < ready.0 - 2.0,
        "listening should visibly turn toward the user: {ready:?} vs {listening:?}"
    );
    assert!(
        waiting.1 > ready.1 + 1.5,
        "waiting should have its own lower held posture: {ready:?} vs {waiting:?}"
    );
    let later_waiting = center(AvatarPose::Waiting, 2.0);
    assert!(
        (waiting.0 - later_waiting.0).abs() < 1.0 && (waiting.1 - later_waiting.1).abs() < 1.0,
        "the waiting pulse should keep its held posture"
    );
    let first = center(AvatarPose::Working, 0.0);
    let raised = center(AvatarPose::Working, 1.0);
    assert!(
        (raised.1 - first.1).abs() > 2.0,
        "working hover motion should be visible: {first:?} vs {raised:?}"
    );
}

#[test]
fn hover_upper_silhouette_keeps_ground_as_background_in_actual_terminal_cells() {
    use crate::{
        app::{App, Driver, RunConfig},
        testing::CaptureTerm,
    };
    let tokens = crate::theme::default_theme().tokens;
    for size in [Size::new(32, 17), Size::new(48, 25)] {
        let mut app = App::new(size);
        app.mount(move |cx| {
            Avatar::new(cx.signal(AvatarPose::Ready))
                .style(AvatarStyle::Hover)
                .layout(
                    LayoutStyle::default()
                        .width(Dimension::Cells(size.w))
                        .height(Dimension::Cells(size.h)),
                )
                .element(cx, &tokens)
                .build()
        })
        .unwrap();
        let mut term = CaptureTerm::new(size);
        let mut driver = Driver::new(
            &mut app,
            &mut term,
            RunConfig {
                probe: false,
                caps: Some(crate::term::Capabilities::with(|c| {
                    c.truecolor = true;
                    c.colors_256 = true;
                    c.unicode_ok = true
                })),
                ..Default::default()
            },
        )
        .unwrap();
        driver.turn(&mut app, &mut term).unwrap();
        let edge = (0..size.h)
            .filter_map(|y| term.screen().cell(size.w / 2, y))
            .find(|cell| cell.paint.bg != Some(tokens.bg) || cell.paint.fg != Some(tokens.bg))
            .expect("dome edge");
        assert_eq!(
            edge.ch(),
            '▄',
            "use lower-half ink rather than a colored background behind upper ground glyph"
        );
        assert!(
            edge.paint.bg.unwrap().luminance() <= edge.paint.fg.unwrap().luminance(),
            "upper background must be the darker ground-facing sample, not bright lower-edge metal"
        );
    }
}

#[test]
fn every_nonerror_pose_has_visible_motion_at_main_and_preview_sizes_with_fixed_audio_level() {
    use crate::{
        app::{App, Driver, RunConfig},
        reactive::set_loop_clock,
        testing::CaptureTerm,
    };
    for size in [Size::new(32, 17), Size::new(48, 25)] {
        for style in [AvatarStyle::Halo, AvatarStyle::Hover] {
            for pose in [
                AvatarPose::Ready,
                AvatarPose::Thinking,
                AvatarPose::Working,
                AvatarPose::Listening,
                AvatarPose::Speaking,
                AvatarPose::Waiting,
            ] {
                let t0 = Instant::now();
                set_loop_clock(Some(t0));
                let mut app = App::new(size);
                app.mount(move |cx| {
                    Avatar::new(cx.signal(pose))
                        .style(style)
                        .level(cx.signal(0.65))
                        .animated(cx.signal(true))
                        .layout(
                            LayoutStyle::default()
                                .width(Dimension::Cells(size.w))
                                .height(Dimension::Cells(size.h)),
                        )
                        .view(cx)
                })
                .unwrap();
                let mut term = CaptureTerm::new(size);
                let mut driver = Driver::new(
                    &mut app,
                    &mut term,
                    RunConfig {
                        probe: false,
                        caps: Some(crate::term::Capabilities::with(|c| {
                            c.truecolor = true;
                            c.colors_256 = true;
                            c.unicode_ok = true
                        })),
                        ..Default::default()
                    },
                )
                .unwrap();
                let now = Rc::new(std::cell::Cell::new(t0));
                let clock = now.clone();
                driver.set_clock(move || clock.get());
                driver.turn(&mut app, &mut term).unwrap();
                let cells = |term: &CaptureTerm| {
                    (0..size.h)
                        .flat_map(|y| {
                            (0..size.w).map(move |x| term.screen().cell(x, y).unwrap().clone())
                        })
                        .collect::<Vec<_>>()
                };
                let first = cells(&term);
                term.take_bytes();
                let mut frames = 0;
                for ms in 1..=2000 {
                    now.set(t0 + Duration::from_millis(ms));
                    if driver.turn(&mut app, &mut term).unwrap().rendered {
                        frames += 1;
                    }
                }
                let final_cells = cells(&term);
                let changed = first
                    .iter()
                    .zip(&final_cells)
                    .filter(|(a, b)| a != b)
                    .count();
                assert!(changed>=5,"{style:?}/{pose:?}/{size:?}: motion must visibly change the artwork, got {changed} cells");
                let expected = 2000 / pose.period().unwrap().as_millis() as usize;
                assert_eq!(
                    frames, expected,
                    "{style:?}/{pose:?}: one bounded pose clock"
                );
                assert!(!term.bytes().is_empty());
                drop(driver);
                drop(app);
                set_loop_clock(None);
            }
        }
    }
}

#[test]
fn halo_intake_and_spoken_emission_are_distinct_without_relying_on_label_or_audio_changes() {
    let materials = Materials::new(&crate::theme::default_theme().tokens);
    for level in [0.0, 0.7] {
        for time in [0.25, 1.2] {
            let mut changed = 0;
            for y in 0..64 {
                for x in 0..64 {
                    let at = |pose| {
                        sample(
                            AvatarStyle::Halo,
                            pose,
                            level,
                            time,
                            (x as f32 - 32.0) / 32.0,
                            (y as f32 - 32.0) / 32.0,
                            &materials,
                        )
                    };
                    if at(AvatarPose::Listening) != at(AvatarPose::Speaking) {
                        changed += 1;
                    }
                }
            }
            assert!(changed>100,"inward listening and outward speaking must differ in artwork: {level}/{time}/{changed}");
        }
    }
}

#[test]
fn transparent_art_preserves_underlying_text_attributes_and_ink_outside_its_shape() {
    use crate::{
        render::{Attrs, Style},
        ui::Element,
    };
    let size = Size::new(32, 17);
    for light in [false, true] {
        let mut tokens = crate::theme::default_theme().tokens;
        if light {
            tokens.bg = Rgba::rgb(238, 242, 252);
        }
        for style in [AvatarStyle::Halo, AvatarStyle::Hover] {
            for pose in [
                AvatarPose::Ready,
                AvatarPose::Listening,
                AvatarPose::Thinking,
                AvatarPose::Working,
                AvatarPose::Speaking,
                AvatarPose::Waiting,
                AvatarPose::Error,
            ] {
                let (root, mut tree) = mount_widget(size, move |cx| {
                    Element::new()
                        .style(LayoutStyle::column())
                        .draw(|canvas, rect| {
                            canvas.fill_styled(
                                rect,
                                'X',
                                &Style::new()
                                    .fg(Rgba::WHITE)
                                    .bg(Rgba::rgb(20, 40, 90))
                                    .bold(),
                            );
                        })
                        .child(
                            Avatar::new(cx.signal(pose))
                                .style(style)
                                .transparent_background(true)
                                .layout(
                                    LayoutStyle::default()
                                        .width(Dimension::Percent(1.0))
                                        .height(Dimension::Percent(1.0)),
                                )
                                .element(cx, &tokens)
                                .build(),
                        )
                        .build()
                });
                let frame = settle(&mut tree, size);
                for point in [
                    Point::new(0, 0),
                    Point::new(31, 0),
                    Point::new(0, 14),
                    Point::new(31, 14),
                ] {
                    assert_eq!(
                        frame.cell(point).unwrap(),
                        ('X', Rgba::WHITE, Rgba::rgb(20, 40, 90)),
                        "outside shape: {style:?}/{pose:?}/light={light}/{point:?}"
                    );
                    assert!(frame.attrs_at(point).contains(Attrs::BOLD));
                }
                let mut occupied = 0;
                for y in 0..16 {
                    for x in 0..size.w {
                        let (glyph, fg, _) = frame.cell(Point::new(x, y)).unwrap();
                        if glyph != 'X' {
                            occupied += 1;
                            assert!(
                                glyph == ' ' || !fg.is_transparent(),
                                "a visible half-block must never carry default/alpha-zero ink"
                            );
                        }
                    }
                }
                assert!(
                    occupied > 30 && occupied < 450,
                    "art visible with empty margins: {occupied}"
                );
                let caption_width = crate::text::width(pose.label());
                let caption_x = (size.w - caption_width) / 2;
                for x in 0..size.w {
                    let cell = frame.cell(Point::new(x, size.h - 1)).unwrap();
                    if x >= caption_x - 1 && x < caption_x + caption_width + 1 {
                        assert_eq!(cell.2, tokens.bg, "tight caption surface is legible");
                    } else {
                        assert_eq!(cell, ('X', Rgba::WHITE, Rgba::rgb(20, 40, 90)));
                        assert!(frame
                            .attrs_at(Point::new(x, size.h - 1))
                            .contains(Attrs::BOLD));
                    }
                }
                root.dispose();
            }
        }
    }
}

#[test]
fn transparent_animation_damage_restores_old_silhouette_and_hidden_underlay() {
    use crate::{
        app::{App, Driver, RunConfig},
        reactive::{next_timer_deadline, set_loop_clock},
        render::Style,
        testing::CaptureTerm,
        ui::Element,
    };
    for style in [AvatarStyle::Halo, AvatarStyle::Hover] {
        assert!(crate::app::set_theme_by_id("abstract-dark"));
        let size = Size::new(32, 17);
        let start = Instant::now();
        set_loop_clock(Some(start));
        let shared = Rc::new(RefCell::new(None));
        let out = shared.clone();
        let mut app = App::new(size);
        app.mount(move |cx| {
            let pose = cx.signal(AvatarPose::Waiting);
            let active = cx.signal(false);
            let motion = cx.signal(true);
            *out.borrow_mut() = Some((pose, active, motion));
            Element::new()
                .style(LayoutStyle::column())
                .draw(|canvas, rect| {
                    canvas.fill_styled(
                        rect,
                        'X',
                        &Style::new()
                            .fg(Rgba::WHITE)
                            .bg(Rgba::rgb(20, 40, 90))
                            .bold(),
                    );
                })
                .child(
                    Avatar::new(pose)
                        .style(style)
                        .active(active)
                        .animated(motion)
                        .transparent_background(true)
                        .show_label(false)
                        .layout(
                            LayoutStyle::default()
                                .width(Dimension::Percent(1.0))
                                .height(Dimension::Percent(1.0)),
                        )
                        .view(cx),
                )
                .build()
        })
        .unwrap();
        let mut term = CaptureTerm::new(size);
        let mut driver = Driver::new(
            &mut app,
            &mut term,
            RunConfig {
                probe: false,
                caps: Some(crate::term::Capabilities::with(|c| {
                    c.truecolor = true;
                    c.colors_256 = true;
                    c.unicode_ok = true;
                })),
                ..Default::default()
            },
        )
        .unwrap();
        let now = Rc::new(std::cell::Cell::new(start));
        let clock = now.clone();
        driver.set_clock(move || clock.get());
        driver.turn(&mut app, &mut term).unwrap();
        let cells = |term: &CaptureTerm| {
            (0..size.h)
                .flat_map(|y| (0..size.w).map(move |x| term.screen().cell(x, y).unwrap().clone()))
                .collect::<Vec<_>>()
        };
        let baseline = cells(&term);
        let (pose, active, motion) = shared.borrow().unwrap();
        active.set(true);
        driver.turn(&mut app, &mut term).unwrap();
        let first = cells(&term);
        pose.set(AvatarPose::Listening);
        driver.turn(&mut app, &mut term).unwrap();
        let second = cells(&term);
        let restored = first
            .iter()
            .zip(&second)
            .zip(&baseline)
            .filter(|((old, current), base)| old != base && current == base)
            .count();
        assert!(
            restored > 0,
            "pose change restores cells occupied only by old pose: {style:?}"
        );
        for ms in [84, 168, 252, 336, 420] {
            now.set(start + Duration::from_millis(ms));
            driver.turn(&mut app, &mut term).unwrap();
            assert_eq!(term.screen().cell(0, 0).unwrap(), &baseline[0]);
        }
        let dark = cells(&term);
        assert!(crate::app::set_theme_by_id("abstract-light"));
        driver.turn(&mut app, &mut term).unwrap();
        assert_ne!(cells(&term), dark, "live theme change updates materials");
        assert_eq!(term.screen().cell(0, 0).unwrap(), &baseline[0]);
        motion.set(false);
        driver.turn(&mut app, &mut term).unwrap();
        assert!(next_timer_deadline().is_none());
        term.take_bytes();
        now.set(start + Duration::from_secs(2));
        driver.turn(&mut app, &mut term).unwrap();
        assert!(term.take_bytes().is_empty(), "reduced motion remains quiet");
        active.set(false);
        driver.turn(&mut app, &mut term).unwrap();
        assert_eq!(
            cells(&term),
            baseline,
            "hidden restores every character/style under old shape"
        );
        assert!(next_timer_deadline().is_none());
        drop(driver);
        drop(app);
        assert!(next_timer_deadline().is_none());
        set_loop_clock(None);
        assert!(crate::app::set_theme_by_id("abstract-dark"));
    }
}
