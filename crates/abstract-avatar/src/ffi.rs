//! Optional C/WebAssembly adapter. Handles own bounded, reusable pixel buffers.
//! Borrowed pixel pointers remain valid until destroy; callers must copy before
//! another render/destroy and synchronize native cross-thread pointer reads.
use super::*;
use std::{
    collections::BTreeMap,
    sync::{LazyLock, Mutex},
};
const LIMIT: u32 = 512;
const MAX_LIVE: usize = 16;
struct Frame {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    palette: [[u32; 256]; 3],
    set: [[bool; 256]; 3],
    ready: usize,
    ground: u32,
    gaze: Gaze,
}
struct Registry {
    next: u32,
    frames: BTreeMap<u32, Frame>,
}
static FRAMES: LazyLock<Mutex<Registry>> = LazyLock::new(|| {
    Mutex::new(Registry {
        next: 1,
        frames: BTreeMap::new(),
    })
});
#[unsafe(no_mangle)]
pub extern "C" fn avatar_abi_version() -> u32 {
    1
}
#[unsafe(no_mangle)]
pub extern "C" fn avatar_style_count() -> u32 {
    AvatarStyle::ALL.len() as u32
}
#[unsafe(no_mangle)]
pub extern "C" fn avatar_style_field_ptr(style: u32, field: u32) -> *const u8 {
    AvatarStyle::from_code(style)
        .and_then(|s| s.fields().get(field as usize).copied())
        .map_or(std::ptr::null(), |s| s.as_ptr())
}
#[unsafe(no_mangle)]
pub extern "C" fn avatar_style_field_len(style: u32, field: u32) -> usize {
    AvatarStyle::from_code(style)
        .and_then(|s| s.fields().get(field as usize).copied())
        .map_or(0, |s| s.len())
}
#[unsafe(no_mangle)]
pub extern "C" fn avatar_create(width: u32, height: u32) -> u32 {
    if width == 0 || height == 0 || width > LIMIT || height > LIMIT {
        return 0;
    }
    let Ok(mut r) = FRAMES.lock() else {
        return 0;
    };
    if r.frames.len() >= MAX_LIVE || r.next == u32::MAX {
        return 0;
    }
    let id = r.next;
    r.next += 1;
    r.frames.insert(
        id,
        Frame {
            width,
            height,
            pixels: vec![0; (width * height * 4) as usize],
            palette: [[0; 256]; 3],
            set: [[false; 256]; 3],
            ready: 0,
            ground: 0,
            gaze: Gaze::default(),
        },
    );
    id
}
#[unsafe(no_mangle)]
pub extern "C" fn avatar_set_shade(handle: u32, material: u32, index: u32, rgba: u32) -> i32 {
    if material >= 3 || index >= 256 {
        return -2;
    }
    let Ok(mut r) = FRAMES.lock() else {
        return -1;
    };
    let Some(f) = r.frames.get_mut(&handle) else {
        return -1;
    };
    let (m, i) = (material as usize, index as usize);
    f.palette[m][i] = rgba;
    if !f.set[m][i] {
        f.set[m][i] = true;
        f.ready += 1;
    }
    0
}
#[unsafe(no_mangle)]
pub extern "C" fn avatar_set_ground(handle: u32, rgba: u32) -> i32 {
    let Ok(mut r) = FRAMES.lock() else {
        return -1;
    };
    let Some(f) = r.frames.get_mut(&handle) else {
        return -1;
    };
    f.ground = rgba;
    0
}
#[unsafe(no_mangle)]
pub extern "C" fn avatar_set_gaze(handle: u32, x: f32, y: f32) -> i32 {
    if !x.is_finite() || !y.is_finite() {
        return -4;
    }
    let Ok(mut r) = FRAMES.lock() else {
        return -1;
    };
    let Some(f) = r.frames.get_mut(&handle) else {
        return -1;
    };
    f.gaze = Gaze::new(x, y);
    0
}
#[unsafe(no_mangle)]
pub extern "C" fn avatar_render(
    handle: u32,
    style: u32,
    pose: u32,
    level: f32,
    time: f32,
    transparent: u32,
) -> i32 {
    let Some(style) = AvatarStyle::from_code(style) else {
        return -2;
    };
    let Some(pose) = AvatarPose::from_code(pose) else {
        return -3;
    };
    if !level.is_finite() || !time.is_finite() || transparent > 1 {
        return -4;
    }
    let Ok(mut r) = FRAMES.lock() else {
        return -1;
    };
    let Some(f) = r.frames.get_mut(&handle) else {
        return -1;
    };
    if f.ready != 768 {
        return -5;
    }
    let radius = f.width.min(f.height) as f32 * 0.5;
    let ground = f.ground.to_be_bytes();
    let frame = super::Frame::new(style, pose, level, time, f.gaze);
    for y in 0..f.height {
        for x in 0..f.width {
            let s = frame.sample(
                (x as f32 + 0.5 - f.width as f32 * 0.5) / radius,
                (y as f32 + 0.5 - f.height as f32 * 0.5) / radius,
            );
            let index = (s.light * 255.).round() as usize;
            let ink = f.palette[s.material as usize][index].to_be_bytes();
            let a = s.coverage * ink[3] as f32 / 255.;
            let offset = ((y * f.width + x) * 4) as usize;
            let dest_alpha = ground[3] as f32 / 255.0;
            let out_alpha = a + dest_alpha * (1.0 - a);
            for c in 0..3 {
                f.pixels[offset + c] = if transparent == 1 {
                    ink[c]
                } else if out_alpha > 0.0 {
                    ((ink[c] as f32 * a + ground[c] as f32 * dest_alpha * (1.0 - a)) / out_alpha)
                        .round() as u8
                } else {
                    0
                };
            }
            f.pixels[offset + 3] =
                ((if transparent == 1 { a } else { out_alpha }) * 255.0).round() as u8;
        }
    }
    0
}
#[unsafe(no_mangle)]
pub extern "C" fn avatar_pixels(handle: u32) -> *const u8 {
    let Ok(r) = FRAMES.lock() else {
        return std::ptr::null();
    };
    r.frames
        .get(&handle)
        .map_or(std::ptr::null(), |f| f.pixels.as_ptr())
}
#[unsafe(no_mangle)]
pub extern "C" fn avatar_pixels_len(handle: u32) -> usize {
    let Ok(r) = FRAMES.lock() else {
        return 0;
    };
    r.frames.get(&handle).map_or(0, |f| f.pixels.len())
}
#[unsafe(no_mangle)]
pub extern "C" fn avatar_destroy(handle: u32) -> i32 {
    let Ok(mut r) = FRAMES.lock() else {
        return -1;
    };
    if r.frames.remove(&handle).is_some() {
        0
    } else {
        -1
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounds_invalid_handles_and_disposal_are_explicit() {
        assert_eq!(avatar_create(0, 32), 0);
        assert_eq!(avatar_create(513, 32), 0);
        let h = avatar_create(32, 32);
        assert_ne!(h, 0);
        assert_eq!(avatar_pixels_len(h), 4096);
        assert_eq!(avatar_render(h, 0, 0, 0.5, 0., 1), -5);
        for m in 0..3 {
            for i in 0..256 {
                assert_eq!(
                    avatar_set_shade(h, m, i, (i << 24) | (i << 16) | (i << 8) | 255),
                    0
                );
            }
        }
        assert_eq!(avatar_render(h, 0, 0, 0.5, 0., 1), 0);
        assert_eq!(avatar_render(h, 99, 0, 0., 0., 1), -2);
        assert_eq!(avatar_render(h, 0, 99, 0., 0., 1), -3);
        assert_eq!(avatar_render(h, 0, 0, f32::NAN, 0., 1), -4);
        assert_eq!(avatar_set_ground(h, 0x123456ff), 0);
        assert_eq!(avatar_render(h, 0, 0, 0.5, 0., 0), 0);
        assert_eq!(
            &FRAMES.lock().unwrap().frames[&h].pixels[..4],
            &[0x12, 0x34, 0x56, 0xff]
        );
        assert_eq!(avatar_destroy(h), 0);
        assert_eq!(avatar_destroy(h), -1);
        assert_eq!(avatar_pixels_len(h), 0);
        assert!(avatar_pixels(h).is_null());
    }
}
