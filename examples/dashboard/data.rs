//! Deterministic data for the dashboard: sin + hash-noise walks, the
//! session fixtures, the log line pool and the mini brandmark model.
//! No rand, no wall entropy — same tick, same frame (golden-friendly).
//!
//! OWNER: DESIGN.

use abstracttui::prelude::*;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::WINDOW;

pub fn hash(n: u64) -> u64 {
    (n ^ 0x9E37_79B9_7F4A_7C15)
        .wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
        .rotate_left(23)
        .wrapping_mul(0x2545_F491_4F6C_DD1D)
}

pub fn hash01(n: u64) -> f32 {
    (hash(n) >> 40) as f32 / (1u64 << 24) as f32
}

/// Sample a walk at history index `i` (0 = oldest of the window).
pub fn sample(now: u64, i: usize, phase: f32, speed: f32, base: f32, amp: f32, noise: f32) -> f32 {
    let t = now.saturating_sub((WINDOW - 1 - i) as u64) as f32;
    base + amp * (t * speed + phase).sin() + noise * (hash01(t as u64 + phase as u64) - 0.5)
}

pub fn rx_at(now: u64, i: usize) -> f32 {
    (sample(now, i, 0.0, 0.11, 52.0, 28.0, 14.0)).clamp(0.0, 100.0)
}

pub fn tx_at(now: u64, i: usize) -> f32 {
    (sample(now, i, 2.1, 0.07, 30.0, 16.0, 10.0)).clamp(0.0, 100.0)
}

pub fn cpu_at(now: u64, i: usize) -> f32 {
    (sample(now, i, 0.7, 0.13, 0.52, 0.34, 0.10)).clamp(0.02, 1.0)
}

pub fn mem_at(now: u64, i: usize) -> f32 {
    (sample(now, i, 4.2, 0.03, 0.63, 0.12, 0.04)).clamp(0.02, 1.0)
}

pub fn io_at(now: u64, i: usize) -> f32 {
    let burst = if hash(now.saturating_sub((WINDOW - 1 - i) as u64) / 16).is_multiple_of(5) {
        0.35
    } else {
        0.0
    };
    (sample(now, i, 1.3, 0.19, 0.28, 0.14, 0.12) + burst).clamp(0.02, 1.0)
}

pub fn clock_text() -> String {
    // Capture determinism: a fixed clock beats a live one in screenshot
    // harnesses (docs cycle); normal runs never set this.
    let secs = std::env::var("ABSTRACTTUI_FIXED_CLOCK")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        });
    format!(
        "{:02}:{:02}:{:02} UTC",
        (secs / 3600) % 24,
        (secs / 60) % 60,
        secs % 60
    )
}

pub fn guard_layout() -> LayoutStyle {
    LayoutStyle::default().absolute(abstracttui::layout::Inset {
        left: Some(0),
        right: Some(0),
        top: Some(0),
        bottom: Some(0),
    })
}

pub fn session_rows(now: u64) -> Vec<Vec<String>> {
    const HOSTS: [(&str, &str); 7] = [
        ("edge-1", "eu-w"),
        ("edge-2", "eu-w"),
        ("edge-3", "us-e"),
        ("core-a", "us-e"),
        ("core-b", "ap-s"),
        ("cache-1", "eu-n"),
        ("cache-2", "ap-s"),
    ];
    HOSTS
        .iter()
        .enumerate()
        .map(|(i, (host, region))| {
            let rx = 8.0 + 40.0 * hash01(now.wrapping_add(i as u64 * 13));
            let tx = 4.0 + 22.0 * hash01(now.wrapping_add(i as u64 * 29 + 5));
            let state = match hash(now.wrapping_add(i as u64 * 43)) % 12 {
                0 => "draining",
                1 | 2 => "syncing",
                _ => "healthy",
            };
            vec![
                host.to_string(),
                region.to_string(),
                format!("{rx:.1} MB/s"),
                format!("{tx:.1} MB/s"),
                state.to_string(),
            ]
        })
        .collect()
}

pub fn log_line(t: &TokenSet, idx: u64) -> (&'static str, Rgba, &'static str) {
    // Level and message travel together — an "error: cache warmed"
    // pairing reads wrong in a screenshot. Weighted picks keep errors
    // rare (2/12) the way a healthy system's tail looks.
    let entries: [(&'static str, Rgba, &'static str); 12] = [
        ("info", t.info, "session opened from edge-3"),
        ("info", t.info, "cache warmed in 412ms"),
        ("info", t.info, "checkpoint flushed"),
        ("info", t.info, "config reloaded (rev 41)"),
        ("info", t.info, "session opened from core-a"),
        ("ok", t.ok, "tls renewed for gateway"),
        ("ok", t.ok, "shard 2 caught up"),
        ("ok", t.ok, "backup verified"),
        ("warn", t.warn, "retry budget at 70%"),
        ("warn", t.warn, "backpressure on shard 2"),
        ("error", t.error, "edge-2 heartbeat missed"),
        ("error", t.error, "flush timed out, retrying"),
    ];
    // Deterministic anti-repeat: when the hash lands adjacent rows on
    // the same entry, nudge the newer one — repeated lines read like a
    // rendering bug in a still.
    let n = entries.len() as u64;
    let pick = hash(idx) % n;
    let prev = hash(idx.wrapping_sub(1)) % n;
    let pick = if pick == prev { (pick + 5) % n } else { pick };
    entries[pick as usize]
}

/// Three ramp-colored planes — the boot identity's mark as a tiny
/// static model (geometry inline; colors from `identity::brand_ramp`).
pub fn brandmark_model() -> abstracttui::three::Model {
    use abstracttui::boot::identity::brand_ramp;
    use abstracttui::three::primitives::cuboid;
    use abstracttui::three::texture::srgb8_to_linear;
    use abstracttui::three::{Mat4, MaterialData, MeshInstance, Model, Vec3};

    let mut instances = Vec::with_capacity(3);
    for i in 0..3u32 {
        let mut mesh = cuboid(if i == 2 { 0.7 } else { 1.0 }, 0.62, 0.04);
        let colors = mesh
            .positions
            .iter()
            .map(|p| {
                let xn = (p[0] + 0.5).clamp(0.0, 1.0);
                let c = brand_ramp((i as f32 + xn) / 3.0);
                [
                    srgb8_to_linear(c.r),
                    srgb8_to_linear(c.g),
                    srgb8_to_linear(c.b),
                    1.0,
                ]
            })
            .collect();
        mesh.colors = Some(colors);
        mesh.material = Some(0);
        let fi = i as f32 - 1.0;
        let world = Mat4::translate(Vec3::new(0.0, (fi + 1.0) * 0.18, fi * 0.22))
            .mul(&Mat4::rotate_x(12f32.to_radians()));
        instances.push(MeshInstance {
            data: mesh,
            world,
            source_node: None,
        });
    }
    Model {
        instances,
        materials: vec![MaterialData::default()],
        warnings: Vec::new(),
        rig: None,
    }
}
