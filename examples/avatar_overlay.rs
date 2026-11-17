//! avatar_overlay — the activity avatar floating over a busy transcript.
//!
//! Demonstrates: `Avatar::transparent_background` on an overlay layer, and
//! `Avatar::tint_below`. A terminal cell holds one character, so any cell
//! the artwork draws a mosaic character into loses the text beneath it.
//! Faint cells (the glow, anti-aliased edges) now TINT the text beneath
//! instead, keeping it readable; only cells with real ink replace it.
//! Press `0` to see the 0.8 behavior (every touched cell erases its text)
//! and `5` to come back to the default.
//!
//! Keys: arrows move the avatar · `[` / `]` tint threshold −/+ 0.1 ·
//! `0` old behavior (0.0) · `5` default (0.5) · `g` glyph mode (auto,
//! quadrant, sextant, half block, braille) · `b` transparent/opaque card ·
//! `p` pose · `s` design · `space` animation · `t` theme · `q` quit.
//!
//! Try: `cargo run --example avatar_overlay --features avatar`
//!
//! OWNER: DESIGN.

mod common;

use abstracttui::gfx::MosaicMode;
use abstracttui::prelude::*;

const MODES: [Option<MosaicMode>; 5] = [
    None,
    Some(MosaicMode::Quadrant),
    Some(MosaicMode::Sextant),
    Some(MosaicMode::HalfBlock),
    Some(MosaicMode::Braille),
];

const POSES: [AvatarPose; 5] = [
    AvatarPose::Thinking,
    AvatarPose::Speaking,
    AvatarPose::Listening,
    AvatarPose::Working,
    AvatarPose::Ready,
];

const AVATAR: Size = Size { w: 26, h: 14 };

fn main() -> abstracttui::base::Result<()> {
    if !abstracttui::term::have_tty() {
        println!("avatar_overlay: needs an interactive terminal — skipping cleanly");
        return Ok(());
    }
    if let Ok(id) = std::env::var("ABSTRACTTUI_THEME") {
        set_theme_by_id(&id);
    }

    let mut app = App::new(Size::new(110, 32));
    let overlays = app.overlays();
    let quit = app.quitter();
    app.mount(move |cx| {
        let vp = use_viewport(cx);
        let theme = use_theme(cx);
        let tint = cx.signal(AVATAR_TINT_BELOW);
        let mode = cx.signal(0usize);
        let transparent = cx.signal(true);
        let pose_i = cx.signal(0usize);
        let pose = cx.signal(POSES[0]);
        let style = cx.signal(AVATAR_DEFAULT_STYLE);
        let animated = cx.signal(true);
        let styles = avatar_ready_styles();

        // The avatar floats on its own NON-modal tree layer above the page,
        // exactly like a host app's corner avatar. Rebuilt (fresh scope)
        // whenever a setting that the builder takes changes.
        // `None` = the bottom-right corner of whatever the viewport is (it
        // is not published yet at mount); arrows switch to an explicit spot.
        let at: Signal<Option<Point>> = cx.signal(None);
        let corner = |size: Size| {
            Point::new(
                (size.w - AVATAR.w - 4).max(0),
                (size.h - AVATAR.h - 3).max(0),
            )
        };
        let layer = overlays.layer_tree(
            500,
            Rect::new(0, 0, AVATAR.w, AVATAR.h),
            false,
            cx.child(),
            dyn_view_scoped(LayoutStyle::fill(), move |acx| {
                Avatar::new(pose)
                    .style_signal(style)
                    .animated(animated)
                    .transparent_background(transparent.get())
                    .tint_below(tint.get())
                    .mode_opt(MODES[mode.get()])
                    .layout(LayoutStyle::fill())
                    .view(acx)
            }),
        );
        {
            let layer = layer.clone();
            cx.effect(move || {
                let size = vp.get();
                let p = at.get().unwrap_or_else(|| corner(size));
                layer.set_offset(p);
                layer.damage();
            });
        }
        let nudge = move |dx: i32, dy: i32| {
            let size = vp.get_untracked();
            let p = at.get_untracked().unwrap_or_else(|| corner(size));
            at.set(Some(Point::new(
                (p.x + dx).clamp(0, (size.w - AVATAR.w).max(0)),
                (p.y + dy).clamp(0, (size.h - AVATAR.h - 1).max(0)),
            )));
        };
        let next_theme = move |_: &mut abstracttui::ui::EventCtx| {
            let all = abstracttui::theme::themes();
            let now = theme.get_untracked().id;
            let i = all
                .iter()
                .position(|t| t.id == now)
                .map_or(0, |i| (i + 1) % all.len());
            set_theme_by_id(all[i].id);
        };

        Element::new()
            .style(
                LayoutStyle::column()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .shortcut(KeyChord::plain(Key::Char('q')), move |_| quit.quit())
            .shortcut(KeyChord::plain(Key::Left), move |_| nudge(-2, 0))
            .shortcut(KeyChord::plain(Key::Right), move |_| nudge(2, 0))
            .shortcut(KeyChord::plain(Key::Up), move |_| nudge(0, -1))
            .shortcut(KeyChord::plain(Key::Down), move |_| nudge(0, 1))
            .shortcut(KeyChord::plain(Key::Char('[')), move |_| {
                tint.update(|v| *v = ((*v - 0.1) * 10.0).round().max(0.0) / 10.0)
            })
            .shortcut(KeyChord::plain(Key::Char(']')), move |_| {
                tint.update(|v| *v = ((*v + 0.1) * 10.0).round().min(10.0) / 10.0)
            })
            .shortcut(KeyChord::plain(Key::Char('0')), move |_| tint.set(0.0))
            .shortcut(KeyChord::plain(Key::Char('5')), move |_| {
                tint.set(AVATAR_TINT_BELOW)
            })
            .shortcut(KeyChord::plain(Key::Char('g')), move |_| {
                mode.update(|m| *m = (*m + 1) % MODES.len())
            })
            .shortcut(KeyChord::plain(Key::Char('b')), move |_| {
                transparent.update(|v| *v = !*v)
            })
            .shortcut(KeyChord::plain(Key::Char(' ')), move |_| {
                animated.update(|v| *v = !*v)
            })
            .shortcut(KeyChord::plain(Key::Char('p')), move |_| {
                pose_i.update(|i| *i = (*i + 1) % POSES.len());
                pose.set(POSES[pose_i.get_untracked()]);
            })
            .shortcut(KeyChord::plain(Key::Char('s')), move |_| {
                let now = style.get_untracked();
                let i = styles
                    .iter()
                    .position(|s| *s == now)
                    .map_or(0, |i| (i + 1) % styles.len());
                style.set(styles[i]);
            })
            .shortcut(KeyChord::plain(Key::Char('t')), next_theme)
            .child(dyn_view(
                LayoutStyle::default()
                    .grow(1.0)
                    .width(Dimension::Percent(1.0)),
                move || {
                    let t = theme.get().tokens;
                    Element::new()
                        .style(LayoutStyle::fill())
                        .draw(move |canvas, rect| common::paint_busy_page(canvas, rect, &t))
                        .build()
                },
            ))
            .child(dyn_view(
                LayoutStyle::line(1).width(Dimension::Percent(1.0)),
                move || {
                    let t = theme.get().tokens;
                    let tint_v = tint.get();
                    let tint_label = if tint_v == 0.0 {
                        "tint below 0.0 (0.8 behavior)".to_string()
                    } else {
                        format!("tint below {tint_v:.1}")
                    };
                    let mode_label = match MODES[mode.get()] {
                        None => "glyphs auto".to_string(),
                        Some(m) => format!("glyphs {m:?}"),
                    };
                    let card = if transparent.get() {
                        "transparent"
                    } else {
                        "opaque card"
                    };
                    let state = format!("{:?} · {} · {:?}", pose.get(), card, style.get());
                    Element::new()
                        .style(LayoutStyle::fill())
                        .draw(move |canvas, rect| {
                            canvas.fill(rect, ' ', t.text, t.surface);
                            common::key_legend(
                                canvas,
                                rect,
                                &t,
                                &[
                                    ("arrows", "move"),
                                    ("[ ]", &tint_label),
                                    ("0/5", "old/default"),
                                    ("g", &mode_label),
                                    ("b", "card"),
                                    ("p", "pose"),
                                    ("s", "design"),
                                    ("t", "theme"),
                                    ("q", "quit"),
                                ],
                            );
                            let x = rect.right() - abstracttui::text::width(&state) - 1;
                            canvas.print(
                                Point::new(x.max(rect.x), rect.y),
                                &state,
                                t.text_muted,
                                Rgba::TRANSPARENT,
                            );
                        })
                        .build()
                },
            ))
            .build()
    })?;
    app.run()
}

/// `Avatar::mode` takes a pinned mode; `None` keeps the capability default.
trait ModeOpt {
    fn mode_opt(self, mode: Option<MosaicMode>) -> Self;
}

impl ModeOpt for Avatar {
    fn mode_opt(self, mode: Option<MosaicMode>) -> Self {
        match mode {
            Some(m) => self.mode(m),
            None => self,
        }
    }
}
