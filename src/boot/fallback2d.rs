//! The 2D fallback splash: pure cell rendering of the boot identity for
//! terminals without a graphics channel (or until the 3D mark lands).
//!
//! Faithful to `docs/design/theme-identity.md` §2.3 and driven entirely by
//! `identity` constants — the 3D splash and this fallback read the same
//! numbers, so the two can never drift. Composition per frame (t in
//! seconds):
//!
//! - ground: vertical gradient `bg -> surface` (per-row mix);
//! - mark: the 5-line pure-ASCII "A" revealed bottom-up across the arrival
//!   phase, each line colored by `brand_ramp(row/4)` and faded in with the
//!   arrival easing;
//! - wordmark: per-letter fade with letter-spacing collapsing
//!   `WORDMARK_TRACKING.0 -> .1` cells (whole-cell stepping — documented
//!   fallback approximation of the sub-cell 3D version);
//! - accent underline sweep + tagline + skip hint per the storyboard.
//!
//! All color math goes through `theme::derive` (the RT1-9b rule: color
//! arithmetic lives in one audited place; this module composes, it never
//! invents arithmetic).
//!
//! OWNER: DESIGN.

use crate::anim::particles::{Burst, ParticleField};
use crate::anim::Easing;
use crate::base::{Rect, Rgba, Size};
use crate::render::{Cell, Glyph, Style, Surface};
use crate::theme::derive::mix;
use crate::theme::Theme;

use super::identity;
use super::player::SplashFrameSource;

/// Particle simulation quantum. The field advances in FIXED steps up to
/// the requested `t`, so any t-sequence (live playback, frame drops, a
/// fresh source asked for one arbitrary frame) produces the same pixels —
/// the determinism the beat tests pin, kept under history-bearing
/// garnish.
const SIM_STEP: f32 = 1.0 / 30.0;

/// Seconds helpers over the identity ms constants.
const S: f32 = 1.0 / 1000.0;

fn ease(params: [f32; 4]) -> Easing {
    Easing::CubicBezier(params[0], params[1], params[2], params[3])
}

/// Progress of `t` through the window `[start, start+dur]`, clamped 0..=1.
fn window(t: f32, start: f32, dur: f32) -> f32 {
    if dur <= 0.0 {
        return if t >= start { 1.0 } else { 0.0 };
    }
    ((t - start) / dur).clamp(0.0, 1.0)
}

/// The fallback frame source. Owns its surface; re-renders the full
/// composition every call (the player diffs, so bytes stay proportional
/// to change).
pub struct FallbackSplash {
    surface: Surface,
    /// Spark afterglow (cycle 7): landing kicks + the alignment burst.
    field: ParticleField,
    /// Simulated seconds so far (fixed-step accumulator).
    sim_t: f32,
    sim_size: Size,
}

impl FallbackSplash {
    pub fn new() -> Self {
        FallbackSplash {
            surface: Surface::new(Size::new(0, 0), Cell::EMPTY),
            field: new_field(),
            sim_t: 0.0,
            sim_size: Size::new(0, 0),
        }
    }

    /// Advance the particle simulation to `t` in fixed quanta, spawning
    /// timeline bursts as their moments pass. Deterministic over any
    /// t-sequence; rewinds (tests probing arbitrary frames) reset and
    /// replay from zero.
    fn simulate_to(&mut self, t: f32, size: Size) {
        if size.w < 8 || size.h < 8 {
            return; // no stage, no sparks
        }
        if t < self.sim_t || size != self.sim_size {
            self.field = new_field();
            self.sim_t = 0.0;
            self.sim_size = size;
        }
        let mark_h = identity::MARK_ASCII.len() as i32;
        let top = ((size.h - (mark_h + 4)) / 2).max(0) as f32;
        let mark_x = ((size.w - identity::MARK_ASCII_WIDTH as i32) / 2) as f32;
        let mark_w = identity::MARK_ASCII_WIDTH as f32;

        let arrival_end = identity::PHASE_ALIGN_START_MS as f32 / 1000.0;
        let line_dur = 0.25f32;
        let stagger = (arrival_end - line_dur) / (mark_h - 1).max(1) as f32;
        let ramp = identity::brand_ramp;

        while self.sim_t < t {
            let next = (self.sim_t + SIM_STEP).min(t);
            // Landing kicks: row k (bottom-up) settles at k*stagger+dur.
            for k in 0..mark_h {
                let land = k as f32 * stagger + line_dur;
                if self.sim_t < land && land <= next {
                    let row = top + (mark_h - 1 - k) as f32;
                    // Alternate the kick between the mark's stroke edges.
                    let x = if k % 2 == 0 {
                        mark_x + 2.0
                    } else {
                        mark_x + mark_w - 3.0
                    };
                    self.field.spawn(Burst {
                        origin: (x, row),
                        count: identity::LAND_SPARKS as usize,
                        speed: (3.0, 8.0),
                        life: (0.25, 0.5),
                        colors: [ramp(0.0), ramp(0.5), ramp(1.0)],
                    });
                }
            }
            // The alignment burst (storyboard 0.9 s).
            let burst_at = identity::BURST_AT_MS as f32 / 1000.0;
            if self.sim_t < burst_at && burst_at <= next {
                self.field.spawn(Burst {
                    origin: (mark_x + mark_w * 0.5, top + mark_h as f32 * 0.6),
                    count: identity::BURST_PARTICLES as usize,
                    speed: (5.0, 14.0),
                    life: (0.3, identity::BURST_LIFETIME_MS as f32 / 1000.0),
                    colors: [ramp(0.0), ramp(0.5), ramp(1.0)],
                });
            }
            self.field.step(next - self.sim_t);
            self.sim_t = next;
        }
    }

    fn ground_at(&self, theme: &Theme, y: i32, h: i32) -> Rgba {
        let frac = if h <= 1 {
            0.0
        } else {
            y as f32 / (h - 1) as f32
        };
        mix(theme.tokens.bg, theme.tokens.surface, frac)
    }
}

impl Default for FallbackSplash {
    fn default() -> Self {
        Self::new()
    }
}

/// Field posture: light drag, a whisper of gravity — sparks arc and
/// settle instead of flying off. Seeded (deterministic replay).
fn new_field() -> ParticleField {
    let mut field = ParticleField::new(0xA857_AC71);
    field.gravity = (0.0, 2.6);
    field.drag = 0.82;
    field
}

impl SplashFrameSource for FallbackSplash {
    fn render(&mut self, t: f32, size: Size, theme: &Theme) -> &Surface {
        if self.surface.size() != size {
            self.surface = Surface::new(size, Cell::EMPTY);
        }
        let (w, h) = (size.w, size.h);
        let tk = &theme.tokens;

        // Ground gradient.
        for y in 0..h {
            let g = self.ground_at(theme, y, h);
            self.surface.fill_rect(
                Rect::new(0, y, w, 1),
                Cell::new(Glyph::SPACE).with_fg(g).with_bg(g),
            );
        }

        // Vertical layout: mark (5) + gap + wordmark + underline + tagline.
        let mark_h = identity::MARK_ASCII.len() as i32;
        let block_h = mark_h + 4;
        let top = ((h - block_h) / 2).max(0);
        let wordmark_y = top + mark_h + 1;
        let underline_y = wordmark_y + 1;
        let tagline_y = underline_y + 1;

        // Spark afterglow (cycle 7): landing kicks + the alignment
        // burst, simulated in fixed steps to `t` (deterministic). Drawn
        // BENEATH the mark: sparks emanate from behind the strokes and
        // can never erase the letterform.
        self.simulate_to(t, size);
        self.field.render(&mut self.surface);

        // Mark: lines reveal bottom-up across the arrival phase.
        let arrival_end = identity::PHASE_ALIGN_START_MS as f32 * S; // 0.9 s
        let line_dur = 0.25f32;
        let stagger = (arrival_end - line_dur) / (mark_h - 1).max(1) as f32;
        let mark_x = (w - identity::MARK_ASCII_WIDTH as i32) / 2;
        for (row, line) in identity::MARK_ASCII.iter().enumerate() {
            let from_bottom = (mark_h - 1) as usize - row;
            let alpha = ease(identity::EASE_ARRIVAL).eval(window(
                t,
                from_bottom as f32 * stagger,
                line_dur,
            ));
            if alpha <= 0.0 {
                continue; // not yet arrived: draw nothing, not invisible ink
            }
            let y = top + row as i32;
            let ramp = identity::brand_ramp(row as f32 / (mark_h - 1) as f32);
            let color = mix(self.ground_at(theme, y, h), ramp, alpha);
            // Strokes only: interior spaces stay transparent so the
            // burst sparks show THROUGH the letter's counters instead of
            // being wiped by invisible ink.
            let style = Style::new().fg(color);
            for (i, ch) in line.chars().enumerate() {
                if ch != ' ' {
                    let mut buf = [0u8; 4];
                    self.surface
                        .draw_text(mark_x + i as i32, y, ch.encode_utf8(&mut buf), style);
                }
            }
        }

        // Wordmark: per-letter fade, tracking collapse.
        let reveal_start = identity::PHASE_REVEAL_START_MS as f32 * S; // 1.4 s
        let hold_start = identity::PHASE_HOLD_START_MS as f32 * S; // 1.85 s
        let (tr_from, tr_to) = identity::WORDMARK_TRACKING;
        let track =
            ease(identity::EASE_TRACKING).eval(window(t, reveal_start, hold_start - reveal_start));
        let spacing = tr_from as f32 + (tr_to as f32 - tr_from as f32) * track;
        let letters: Vec<char> = identity::WORDMARK.chars().collect();
        let n = letters.len() as i32;
        let step = 1.0 + spacing;
        let width_now = ((n - 1) as f32 * step) as i32 + 1;
        let final_width = (n - 1) * (1 + tr_to as i32) + 1;
        let wm_x = (w - width_now) / 2;
        if t >= reveal_start {
            let ground = self.ground_at(theme, wordmark_y, h);
            for (i, ch) in letters.iter().enumerate() {
                let alpha =
                    ease(identity::EASE_FADE).eval(window(t, reveal_start + i as f32 * 0.03, 0.18));
                if alpha <= 0.0 {
                    continue;
                }
                // The leading letter carries the house accent; the rest are
                // text ink (logo widget parity).
                let ink = if i == 0 { tk.accent } else { tk.text };
                let color = mix(ground, ink, alpha);
                let x = wm_x + (i as f32 * step).round() as i32;
                self.surface
                    .draw_text(x, wordmark_y, &ch.to_string(), Style::new().fg(color));
            }

            // Accent underline sweep, left to right, at the final width.
            let sweep = ease(identity::EASE_FADE).eval(window(t, reveal_start + 0.1, 0.4));
            let sweep_w = (final_width as f32 * sweep).round() as i32;
            if sweep_w > 0 {
                let ux = (w - final_width) / 2;
                let line: String = "-".repeat(sweep_w.max(0) as usize);
                self.surface
                    .draw_text(ux, underline_y, &line, Style::new().fg(tk.accent));
            }

            // Tagline fades with the wordmark's tail.
            let tag_alpha = ease(identity::EASE_FADE).eval(window(t, reveal_start + 0.2, 0.4));
            if tag_alpha > 0.0 {
                let ground = self.ground_at(theme, tagline_y, h);
                let color = mix(ground, tk.text_muted, tag_alpha);
                let tx = (w - identity::TAGLINE.len() as i32) / 2;
                self.surface
                    .draw_text(tx, tagline_y, identity::TAGLINE, Style::new().fg(color));
            }
        }

        // Skip hint, bottom-right, from 0.3 s (storyboard).
        let hint_alpha = ease(identity::EASE_FADE).eval(window(t, 0.3, 0.3));
        if hint_alpha > 0.0 {
            let hx = w - identity::SKIP_HINT.len() as i32 - 1;
            let hy = h - 1;
            let color = mix(self.ground_at(theme, hy, h), tk.text_faint, hint_alpha);
            self.surface
                .draw_text(hx, hy, identity::SKIP_HINT, Style::new().fg(color));
        }

        &self.surface
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::default_theme;

    fn row_text(s: &Surface, y: i32) -> String {
        (0..s.width())
            .map(|x| {
                let cell = s.get(x, y).copied().unwrap_or(Cell::EMPTY);
                let txt = cell.glyph.as_str(s.pool()).to_string();
                if txt.is_empty() {
                    ' '
                } else {
                    txt.chars().next().unwrap()
                }
            })
            .collect()
    }

    fn frame(t: f32, size: Size) -> (FallbackSplash, usize) {
        let mut src = FallbackSplash::new();
        src.render(t, size, default_theme());
        (src, 0)
    }

    const SIZE: Size = Size { w: 60, h: 16 };

    #[test]
    fn t0_shows_ground_only() {
        let (src, _) = frame(0.0, SIZE);
        for y in 0..SIZE.h {
            let row = row_text(&src.surface, y);
            assert!(
                row.trim().is_empty(),
                "t=0 must be a quiet ground (storyboard beat), row {y}: {row:?}"
            );
        }
        // ...but the gradient is painted (top row darker than bottom row).
        let top_bg = src.surface.get(0, 0).unwrap().bg;
        let bot_bg = src.surface.get(0, SIZE.h - 1).unwrap().bg;
        assert_ne!(top_bg, bot_bg, "vertical gradient must exist");
    }

    #[test]
    fn mark_reveals_bottom_up() {
        let (src, _) = frame(0.35, SIZE);
        let top = (SIZE.h - (identity::MARK_ASCII.len() as i32 + 4)) / 2;
        let top_row = row_text(&src.surface, top);
        let bottom_row = row_text(&src.surface, top + 4);
        assert!(
            bottom_row.contains('/'),
            "bottom mark line visible at 0.35s: {bottom_row:?}"
        );
        assert!(
            !top_row.contains('/'),
            "top mark line not yet arrived at 0.35s: {top_row:?}"
        );
        // By the alignment beat the whole mark stands. Sparks may live in
        // the letter's counters (drawn beneath, strokes win) — so assert
        // every STROKE cell exactly, not the whole row string.
        let (src, _) = frame(1.0, SIZE);
        let mark_x = (SIZE.w - identity::MARK_ASCII_WIDTH as i32) / 2;
        for (i, line) in identity::MARK_ASCII.iter().enumerate() {
            let row = row_text(&src.surface, top + i as i32);
            for (col, ch) in line.chars().enumerate() {
                if ch != ' ' {
                    let at = row.chars().nth((mark_x + col as i32) as usize).unwrap();
                    assert_eq!(at, ch, "stroke cell ({col},{i}) in {row:?}");
                }
            }
        }
        // …and exact rows once the sparks have died (burst 0.9 s + max
        // life 0.45 s < 1.38 s, before the wordmark).
        let (src, _) = frame(1.38, SIZE);
        for (i, line) in identity::MARK_ASCII.iter().enumerate() {
            let row = row_text(&src.surface, top + i as i32);
            assert_eq!(row.trim_end().trim_start(), line.trim(), "mark row {i}");
        }
    }

    #[test]
    fn sparks_fly_at_the_alignment_beat_and_replay_deterministically() {
        // Just after the burst: cells beyond the mark strokes are lit.
        let (a, _) = frame(0.95, SIZE);
        let (b, _) = frame(0.95, SIZE);
        let mut spark_cells = 0;
        for y in 0..SIZE.h {
            let ra = row_text(&a.surface, y);
            assert_eq!(
                ra,
                row_text(&b.surface, y),
                "fresh sources replay identically"
            );
            spark_cells += ra.chars().filter(|c| matches!(c, '•' | '·' | '◦')).count();
        }
        assert!(spark_cells > 0, "the alignment burst must be visible");
        // Long past every lifetime: the field is empty again.
        let (late, _) = frame(1.9, SIZE);
        for y in 0..SIZE.h {
            let row = row_text(&late.surface, y);
            assert!(
                !row.chars().any(|c| matches!(c, '•' | '◦')),
                "sparks must die: {row:?}"
            );
        }
    }

    #[test]
    fn wordmark_letters_collapse_to_snug_tracking() {
        let top = (SIZE.h - (identity::MARK_ASCII.len() as i32 + 4)) / 2;
        let wm_y = top + identity::MARK_ASCII.len() as i32 + 1;

        let (early, _) = frame(1.55, SIZE);
        let (done, _) = frame(2.0, SIZE);
        let early_row = row_text(&early.surface, wm_y);
        let done_row = row_text(&done.surface, wm_y);

        let squeeze = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
        assert_eq!(squeeze(&done_row), identity::WORDMARK, "all letters landed");
        // Tracking metric: the gap between the first two visible letters
        // (early frames show fewer letters — the per-letter fade — so row
        // span is not comparable; letter spacing is).
        let first_gap = |s: &str| {
            let mut cols = s
                .char_indices()
                .filter(|(_, c)| !c.is_whitespace())
                .map(|(i, _)| i);
            let a = cols.next().expect("at least one letter");
            let b = cols.next().expect("at least two letters");
            b - a
        };
        assert!(
            first_gap(&early_row) > first_gap(&done_row),
            "tracking must collapse: {early_row:?} -> {done_row:?}"
        );
        assert_eq!(
            first_gap(&done_row),
            1 + identity::WORDMARK_TRACKING.1 as usize,
            "final tracking is the identity constant"
        );

        // Underline sweep completed at the final width.
        let underline = row_text(&done.surface, wm_y + 1);
        let dash_count = underline.chars().filter(|c| *c == '-').count() as i32;
        let n = identity::WORDMARK.chars().count() as i32;
        assert_eq!(
            dash_count,
            (n - 1) * (1 + identity::WORDMARK_TRACKING.1 as i32) + 1
        );

        // Tagline present.
        let tagline = row_text(&done.surface, wm_y + 2);
        assert!(tagline.contains(identity::TAGLINE));
    }

    #[test]
    fn skip_hint_appears_after_grace() {
        let (early, _) = frame(0.1, SIZE);
        let (later, _) = frame(1.0, SIZE);
        let hint_row_early = row_text(&early.surface, SIZE.h - 1);
        let hint_row_later = row_text(&later.surface, SIZE.h - 1);
        assert!(!hint_row_early.contains("skip"));
        assert!(hint_row_later.contains(identity::SKIP_HINT));
    }

    #[test]
    fn tiny_terminals_never_panic() {
        for size in [
            Size::new(1, 1),
            Size::new(8, 2),
            Size::new(0, 0),
            Size::new(20, 4),
        ] {
            let mut src = FallbackSplash::new();
            for step in 0..=20 {
                src.render(step as f32 * 0.1, size, default_theme());
            }
        }
    }

    #[test]
    fn deterministic_at_equal_t() {
        let (a, _) = frame(1.7, SIZE);
        let (b, _) = frame(1.7, SIZE);
        for y in 0..SIZE.h {
            assert_eq!(row_text(&a.surface, y), row_text(&b.surface, y));
        }
    }
}
