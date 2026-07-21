//! ParticleField: a small, deterministic particle system for afterglow,
//! bursts and celebratory garnish (the boot splash's particle burst is
//! the design consumer).
//!
//! Pure simulation: seeded xorshift randomness, fixed-timestep-friendly
//! `step(dt)`, no clocks, no globals — the same seed and the same step
//! sequence reproduce the same field bit-for-bit (REDTEAM can golden it).
//! Rendering plots each live particle as one cell (glyph by remaining
//! life, fg = particle color faded toward transparent-ink levels), with
//! the y axis aspect-corrected so velocities read visually straight.
//! Positions live in CELL coordinates as f32 (sub-cell motion
//! accumulates; the plot rounds).

use crate::base::Rgba;
use crate::render::cell::Cell;
use crate::render::style::Style;
use crate::render::surface::Surface;

/// One live particle (cell-space position, cells/second velocity).
#[derive(Copy, Clone, Debug)]
pub struct Particle {
    /// Column (fractional; rendered at `round`).
    pub x: f32,
    /// Row (fractional).
    pub y: f32,
    /// Horizontal velocity, cells/second.
    pub vx: f32,
    /// Vertical velocity, cells/second (y grows downward).
    pub vy: f32,
    /// Seconds remaining; retired at ≤ 0.
    pub life: f32,
    /// Initial life (for fade fractions).
    pub life0: f32,
    /// Ink; alpha fades with remaining life at draw.
    pub color: Rgba,
}

/// Spawn parameters for one burst.
#[derive(Copy, Clone, Debug)]
pub struct Burst {
    /// Emission point in cell space.
    pub origin: (f32, f32),
    /// Number of particles.
    pub count: usize,
    /// Speed range (cells/second) — each particle draws uniformly.
    pub speed: (f32, f32),
    /// Life range in seconds.
    pub life: (f32, f32),
    /// Palette sampled per particle.
    pub colors: [Rgba; 3],
}

/// A seeded, deterministic particle system (bursts, gravity, drag; same
/// seed + same calls = same pixels on every platform).
pub struct ParticleField {
    rng: u64,
    /// Constant acceleration (cells/s²); y grows downward, so a gentle
    /// positive y reads as gravity.
    pub gravity: (f32, f32),
    /// Velocity multiplier per second (0.9 = light drag). Applied as
    /// `v *= drag.powf(dt)`-free linear form: `v *= 1 - (1-drag)*dt`
    /// clamped ≥ 0 (deterministic, no powf).
    pub drag: f32,
    particles: Vec<Particle>,
}

impl ParticleField {
    /// An empty field with a fixed random seed (determinism contract).
    pub fn new(seed: u64) -> ParticleField {
        ParticleField {
            rng: seed | 1, // xorshift must not start at 0
            gravity: (0.0, 0.0),
            drag: 1.0,
            particles: Vec::new(),
        }
    }

    /// Live particle count.
    pub fn len(&self) -> usize {
        self.particles.len()
    }

    /// True when nothing is alive — stop requesting frames.
    pub fn is_empty(&self) -> bool {
        self.particles.is_empty()
    }

    /// The live particles (read-only; mutate through `tick`/`spawn`).
    pub fn particles(&self) -> &[Particle] {
        &self.particles
    }

    fn next(&mut self) -> u64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        self.rng
    }

    fn uniform(&mut self, lo: f32, hi: f32) -> f32 {
        let u = (self.next() >> 40) as f32 / (1u64 << 24) as f32; // 24 exact bits
        lo + (hi - lo) * u
    }

    /// Emits one radial burst. Directions distribute uniformly around the
    /// circle (polynomial sine/cosine pair — deterministic, no libm),
    /// aspect-corrected so the burst LOOKS round on 1:2 cells.
    pub fn spawn(&mut self, burst: Burst) {
        self.particles.reserve(burst.count);
        for _ in 0..burst.count {
            let turns = self.uniform(0.0, 1.0);
            let (dx, dy) = unit_dir(turns);
            let speed = self.uniform(burst.speed.0, burst.speed.1);
            let life = self.uniform(burst.life.0.max(0.01), burst.life.1.max(0.01));
            let color = burst.colors[(self.next() % 3) as usize];
            self.particles.push(Particle {
                x: burst.origin.0,
                y: burst.origin.1,
                vx: dx * speed,
                // Halve vertical speed: cells are ~2x tall, so equal cell
                // velocity would look 2x faster vertically.
                vy: dy * speed * 0.5,
                life,
                life0: life,
                color,
            });
        }
    }

    /// Advances the simulation. Call with your frame dt (fixed or
    /// variable); the integration is explicit Euler — plenty for garnish,
    /// documented as such.
    pub fn step(&mut self, dt: f32) {
        let dt = dt.max(0.0);
        let drag = (1.0 - (1.0 - self.drag) * dt).clamp(0.0, 1.0);
        for p in &mut self.particles {
            p.vx += self.gravity.0 * dt;
            p.vy += self.gravity.1 * dt * 0.5; // aspect-corrected gravity
            p.vx *= drag;
            p.vy *= drag;
            p.x += p.vx * dt;
            p.y += p.vy * dt;
            p.life -= dt;
        }
        self.particles.retain(|p| p.life > 0.0);
    }

    /// Plots live particles into `s` (clipped). Glyph by remaining life
    /// (`•` fresh, `◦` mid, `·` fading); ink fades toward the surface's
    /// existing cell as life runs out (patch draw keeps the ground).
    /// One cell per particle; later particles overdraw earlier ones.
    pub fn render(&self, s: &mut Surface) {
        for p in &self.particles {
            let x = p.x.round() as i32;
            let y = p.y.round() as i32;
            if x < 0 || y < 0 || x >= s.width() || y >= s.height() {
                continue;
            }
            let frac = (p.life / p.life0).clamp(0.0, 1.0);
            let glyph = if frac > 0.66 {
                "•"
            } else if frac > 0.33 {
                "◦"
            } else {
                "·"
            };
            // Fade by scaling the ink toward black; the compositor's
            // Additive layers turn that into "less light" naturally.
            let fade = |v: u8| ((v as f32) * (0.35 + 0.65 * frac)).round() as u8;
            let ink = Rgba::rgb(fade(p.color.r), fade(p.color.g), fade(p.color.b));
            s.draw_text(x, y, glyph, Style::new().fg(ink));
        }
    }

    /// Convenience: EMPTY-clears `s` then renders (afterglow layers reuse
    /// one surface per frame).
    pub fn render_clear(&self, s: &mut Surface) {
        s.clear(Cell::EMPTY);
        self.render(s);
    }
}

/// (cos, sin) of `turns` (fraction of a full circle) via the polynomial
/// sine — deterministic across platforms.
fn unit_dir(turns: f32) -> (f32, f32) {
    let sin = super::easing::poly_sin_turns(turns);
    let cos = super::easing::poly_sin_turns(turns + 0.25);
    (cos, sin)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::Size;

    fn burst() -> Burst {
        Burst {
            origin: (10.0, 5.0),
            count: 12,
            speed: (2.0, 6.0),
            life: (0.4, 0.8),
            colors: [
                Rgba::rgb(255, 200, 80),
                Rgba::rgb(255, 120, 40),
                Rgba::rgb(240, 240, 240),
            ],
        }
    }

    #[test]
    fn deterministic_for_a_seed() {
        let run = |seed| {
            let mut f = ParticleField::new(seed);
            f.gravity = (0.0, 3.0);
            f.spawn(burst());
            let mut s = Surface::new(Size::new(24, 10), Cell::EMPTY);
            for _ in 0..5 {
                f.step(1.0 / 30.0);
            }
            f.render(&mut s);
            format!("{s:?}")
        };
        assert_eq!(run(7), run(7), "same seed, same pixels");
        assert_ne!(run(7), run(8), "different seed, different pixels");
    }

    #[test]
    fn particles_move_age_and_retire() {
        let mut f = ParticleField::new(42);
        f.spawn(burst());
        assert_eq!(f.len(), 12);
        let before: Vec<(f32, f32)> = f.particles().iter().map(|p| (p.x, p.y)).collect();
        f.step(0.1);
        let moved = f
            .particles()
            .iter()
            .zip(&before)
            .filter(|(p, (x, y))| (p.x - x).abs() > 1e-6 || (p.y - y).abs() > 1e-6)
            .count();
        assert!(moved > 0, "particles move");
        // Step past every lifetime: all retire.
        for _ in 0..30 {
            f.step(0.05);
        }
        assert!(f.is_empty(), "lifetimes expire");
    }

    #[test]
    fn gravity_and_drag_shape_the_motion() {
        let mut f = ParticleField::new(1);
        f.gravity = (0.0, 10.0);
        f.spawn(Burst {
            speed: (0.0, 0.0),
            ..burst()
        });
        f.step(0.5);
        assert!(
            f.particles().iter().all(|p| p.vy > 0.0),
            "gravity pulls down"
        );

        let mut d = ParticleField::new(1);
        d.drag = 0.0; // full drag: velocity dies fast
        d.spawn(Burst {
            speed: (4.0, 4.0),
            ..burst()
        });
        let v0: f32 = d.particles().iter().map(|p| p.vx.abs() + p.vy.abs()).sum();
        d.step(0.5);
        let v1: f32 = d.particles().iter().map(|p| p.vx.abs() + p.vy.abs()).sum();
        assert!(v1 < v0 * 0.6, "drag bleeds speed: {v0} -> {v1}");
    }

    #[test]
    fn render_clips_and_respects_ground() {
        let mut f = ParticleField::new(3);
        f.spawn(Burst {
            origin: (-5.0, -5.0),
            ..burst()
        }); // off-surface
        let mut s = Surface::new(Size::new(8, 4), Cell::EMPTY);
        s.fill_rect(s.bounds(), Cell::EMPTY.with_bg(Rgba::rgb(9, 9, 20)));
        f.render(&mut s); // no panic, clipped
                          // Ground preserved wherever a particle DID land (patch draw).
        f.spawn(Burst {
            origin: (4.0, 2.0),
            speed: (0.0, 0.0),
            ..burst()
        });
        f.render(&mut s);
        let cell = s.get(4, 2).unwrap();
        assert_eq!(
            cell.bg,
            Rgba::rgb(9, 9, 20),
            "particle ink patches over ground"
        );
        assert_eq!(s.glyph_str(cell), "•", "fresh particle glyph");
        s.debug_validate().unwrap();
    }
}
