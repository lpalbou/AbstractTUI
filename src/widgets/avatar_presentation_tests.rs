//! Exercise shared RGBA presentation through the real widget/cell compositor.
use super::*;
use crate::{
    base::Size,
    widgets::itest_util::{mount_widget, settle},
};

#[test]
fn cached_palette_matches_theme_owned_material_ramps() {
    let tokens = crate::theme::default_theme().tokens;
    let materials = Materials::new(&tokens);
    for (row, ramp) in [&materials.warm, &materials.eye, &materials.dim]
        .into_iter()
        .enumerate()
    {
        for index in 0..256 {
            let expected = ramp.sample(index as f32 / 255.0, 1.0);
            assert_eq!(
                materials.palette[row][index].to_be_bytes(),
                [expected.r, expected.g, expected.b, expected.a]
            );
        }
    }
}

#[test]
fn pointer_press_refreshes_attention_without_leaving_a_clock() {
    use crate::reactive::{
        create_root, flush_effects, next_timer_deadline, run_due_timers, set_loop_clock,
    };
    use std::time::{Duration, Instant};
    let start = Instant::now();
    set_loop_clock(Some(start));
    let (root, pointer) = create_root(AvatarPointer::new);
    pointer.pressed(Point::new(4, 9));
    flush_effects();
    assert_eq!(pointer.point.get_untracked(), Some(Point::new(4, 9)));
    assert_eq!(next_timer_deadline(), Some(start + Duration::from_secs(2)));
    set_loop_clock(Some(start + Duration::from_secs(1)));
    pointer.pressed(Point::new(4, 9));
    flush_effects();
    assert_eq!(next_timer_deadline(), Some(start + Duration::from_secs(3)));
    run_due_timers(start + Duration::from_secs(3));
    flush_effects();
    assert_eq!(pointer.point.get_untracked(), None);
    assert!(next_timer_deadline().is_none());
    root.dispose();
    set_loop_clock(None);
}

#[test]
fn shared_rgba_preserves_transparency_and_composites_opaque_ground() {
    let tokens = crate::theme::default_theme().tokens;
    let opaque = Materials::new(&tokens);
    let mut transparent = Materials::new(&tokens);
    transparent.transparent_background = true;
    for style in [
        AvatarStyle::WatchHalo,
        AvatarStyle::WatchWave,
        AvatarStyle::WatchHover,
        AvatarStyle::WatchRings,
        AvatarStyle::WatchEye,
    ] {
        let frame = abstractavatar::Frame::new(
            style,
            AvatarPose::Speaking,
            0.65,
            1.25,
            AvatarGaze::default(),
        );
        for y in 0..32 {
            for x in 0..32 {
                let (x, y) = ((x as f32 - 16.0) / 16.0, (y as f32 - 16.0) / 16.0);
                let foreground = transparent.rgba(&frame, x, y, 1.0 / 16.0);
                assert_eq!(
                    opaque.rgba(&frame, x, y, 1.0 / 16.0),
                    foreground.over(tokens.bg)
                );
            }
        }
    }
}

#[test]
fn explicit_mosaic_families_keep_visible_ink_and_preserve_empty_margins() {
    let size = Size::new(30, 16);
    for mode in [
        MosaicMode::HalfBlock,
        MosaicMode::Quadrant,
        MosaicMode::Sextant,
        MosaicMode::Braille,
    ] {
        let (root, mut tree) = mount_widget(size, move |cx| {
            Element::new()
                .draw(|canvas, rect| canvas.fill(rect, 'X', Rgba::WHITE, Rgba::rgb(20, 40, 90)))
                .child(
                    Avatar::new(cx.signal(AvatarPose::Speaking))
                        .style(AvatarStyle::WatchHover)
                        .mode(mode)
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
        });
        let frame = settle(&mut tree, size);
        assert_eq!(
            frame.cell(Point::ZERO).unwrap(),
            ('X', Rgba::WHITE, Rgba::rgb(20, 40, 90))
        );
        let mut art = 0;
        for y in 0..size.h {
            for x in 0..size.w {
                let (ch, fg, bg) = frame.cell(Point::new(x, y)).unwrap();
                if ch != 'X' {
                    art += 1;
                    assert!(
                        ch == ' ' || !fg.is_transparent(),
                        "{mode:?}: {ch:?}/{fg:?}/{bg:?}"
                    );
                }
            }
        }
        assert!(art > 10 && art < (size.w * size.h) as usize);
        root.dispose();
    }
}

#[test]
fn complementing_transparent_foregrounds_retains_subcell_coverage() {
    for (mode, chars) in [
        (
            MosaicMode::Quadrant,
            crate::canvas::QUADRANT_CHARS.as_slice(),
        ),
        (MosaicMode::Sextant, crate::gfx::SEXTANT_CHARS.as_slice()),
    ] {
        for index in 1..chars.len() - 1 {
            let (ch, fg, bg) = display_cell(
                crate::gfx::MosaicCell {
                    ch: chars[index],
                    fg: Rgba::TRANSPARENT,
                    bg: Rgba::WHITE,
                },
                mode,
                true,
            );
            assert_eq!(ch, chars[chars.len() - 1 - index]);
            assert_eq!(fg, Rgba::WHITE);
            assert!(bg.is_transparent());
        }
    }
}

#[test]
fn host_repaints_reuse_art_while_observed_inputs_invalidate_it() {
    use crate::{
        app::{App, Driver, RunConfig},
        reactive::set_loop_clock,
        testing::CaptureTerm,
    };
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
        time::{Duration, Instant},
    };

    assert!(crate::app::set_theme_by_id("abstract-dark"));
    let size = Size::new(30, 16);
    let start = Instant::now();
    let now = Rc::new(Cell::new(start));
    let clock = now.clone();
    set_loop_clock(Some(start));
    let output = Rc::new(RefCell::new(None));
    let controls = output.clone();
    let mut app = App::new(size);
    app.mount(move |cx| {
        let pose = cx.signal(AvatarPose::Speaking);
        let style = cx.signal(AvatarStyle::WatchHover);
        let level = cx.signal(0.3);
        let motion = cx.signal(false);
        let active = cx.signal(true);
        let pointer = AvatarPointer::new(cx);
        *controls.borrow_mut() = Some((pose, style, level, motion, active, pointer));
        Avatar::new(pose)
            .style_signal(style)
            .level(level)
            .animated(motion)
            .active(active)
            .follow_pointer(pointer)
            .show_label(false)
            .layout(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
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
            caps: Some(crate::term::Capabilities::with(|caps| {
                caps.truecolor = true;
                caps.colors_256 = true;
                caps.unicode_ok = true;
            })),
            ..Default::default()
        },
    )
    .unwrap();
    driver.set_clock(move || clock.get());
    let invalidate = app.tree().invalidator();
    let samples = || RASTER_SAMPLES.with(Cell::get);
    RASTER_SAMPLES.with(|count| count.set(0));
    driver.turn(&mut app, &mut term).unwrap();
    driver.turn(&mut app, &mut term).unwrap();
    let mut previous = samples();
    assert!(previous > 0);
    for _ in 0..5 {
        invalidate(); // A real complete tree paint, including the Avatar draw.
        driver.turn(&mut app, &mut term).unwrap();
        assert_eq!(
            samples(),
            previous,
            "unchanged host paint resampled the avatar"
        );
    }
    let (pose, style, level, motion, active, pointer) = output.borrow().unwrap();
    level.set(0.8);
    driver.turn(&mut app, &mut term).unwrap();
    assert!(
        samples() > previous,
        "audio input must invalidate static speaking art"
    );
    previous = samples();
    pose.set(AvatarPose::Thinking);
    driver.turn(&mut app, &mut term).unwrap();
    assert!(samples() > previous, "pose must invalidate");
    previous = samples();
    style.set(AvatarStyle::WatchRings);
    driver.turn(&mut app, &mut term).unwrap();
    assert!(samples() > previous, "design must invalidate");
    previous = samples();
    assert!(crate::app::set_theme_by_id("abstract-light"));
    driver.turn(&mut app, &mut term).unwrap();
    assert!(samples() > previous, "theme must invalidate");
    previous = samples();
    term.push_resize(Size::new(40, 20));
    driver.turn(&mut app, &mut term).unwrap();
    assert!(samples() > previous, "raster dimensions must invalidate");
    previous = samples();
    motion.set(true);
    driver.turn(&mut app, &mut term).unwrap();
    now.set(start + Duration::from_millis(100));
    driver.turn(&mut app, &mut term).unwrap();
    assert!(
        samples() > previous,
        "an actual frame-clock tick must invalidate"
    );
    pose.set(AvatarPose::Ready);
    driver.turn(&mut app, &mut term).unwrap();
    previous = samples();
    pointer.pressed(Point::ZERO);
    driver.turn(&mut app, &mut term).unwrap();
    driver.turn(&mut app, &mut term).unwrap();
    assert!(samples() > previous, "click attention must invalidate gaze");
    active.set(false);
    driver.turn(&mut app, &mut term).unwrap();
    previous = samples();
    invalidate();
    driver.turn(&mut app, &mut term).unwrap();
    assert_eq!(
        samples(),
        previous,
        "hidden host paints must do no generation work"
    );
    drop(driver);
    drop(app);
    set_loop_clock(None);
    assert!(crate::app::set_theme_by_id("abstract-dark"));
}

#[test]
fn cache_invalidation_includes_glyph_mode_and_transparent_composition() {
    use std::{cell::Cell, rc::Rc};
    let tokens = crate::theme::default_theme().tokens;
    let materials = Rc::new(Materials::new(&tokens));
    let key = |mode, materials: Rc<Materials>| RenderKey {
        style: AvatarStyle::WatchWave,
        pose: AvatarPose::Speaking,
        level: 0.6,
        time: 1.25,
        gaze: AvatarGaze::default(),
        cols: 20,
        rows: 10,
        mode,
        materials,
    };
    let mut cache = RenderCache::default();
    cache.prepare(key(MosaicMode::HalfBlock, materials.clone()));
    let first = RASTER_SAMPLES.with(Cell::get);
    let pixels = cache.image.pixels().as_ptr();
    let cells = cache.renderer.grid().cells().as_ptr();
    cache.prepare(key(MosaicMode::HalfBlock, materials.clone()));
    assert_eq!(RASTER_SAMPLES.with(Cell::get), first);
    assert_eq!(cache.image.pixels().as_ptr(), pixels);
    assert_eq!(cache.renderer.grid().cells().as_ptr(), cells);
    cache.prepare(key(MosaicMode::Quadrant, materials));
    let second = RASTER_SAMPLES.with(Cell::get);
    assert!(second > first);
    let mut transparent = Materials::new(&tokens);
    transparent.transparent_background = true;
    cache.prepare(key(MosaicMode::Quadrant, Rc::new(transparent)));
    assert!(RASTER_SAMPLES.with(Cell::get) > second);
    assert!(cache
        .image
        .pixels()
        .iter()
        .any(|pixel| pixel.is_transparent()));
}
