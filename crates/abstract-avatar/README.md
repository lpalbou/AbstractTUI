# Abstract Avatar

One procedural geometry and style catalog for terminal, Canvas and native GUI
hosts. No UI framework, image assets, network, model, microphone or timers.
This crate lives in the AbstractTUI repository but does not depend on AbstractTUI.

| Key | Name | Watch reference |
|---|---|---|
| halo | Halo orb | A |
| hover | Hover robot | HV |
| vortex | Vortex | B |
| eclipse | Eclipse | U |
| eye | Eye | K |
| rover | Rover | Q |

The watch SwiftUI scenes are the design reference, not a runtime dependency.
The new analytic forms adapt their vocabulary; they do not claim pixel-identical
Swift rendering. Existing `halo` and `hover` preference keys remain unchanged.
`AvatarStyle::ALL` and its metadata methods are the single catalog source.

## Rust

```rust
use abstract_avatar::{Frame, Gaze, AvatarStyle, AvatarPose};
let frame = Frame::new(AvatarStyle::Hover, AvatarPose::Ready, 0.6, 1.2, Gaze::new(-0.5, 0.2));
let pixel = frame.sample(0.2, -0.1);
// Host maps pixel.material / light / coverage through its theme's material ramp.
```

Coordinates are centered, radius1, y down. Inputs are explicit and deterministic;
invalid coordinates become empty, level is clamped0–1 and invalid time becomes0.
Error uses static time. Hover/Rover show red X eyes; Eye shows one central red X.
Gaze is a normalized, clamped target and is ignored outside Ready. Face eyes shift;
abstract fields turn subtly. Hosts feed real cursor events and reset gaze after2s
of inactivity, disabling it for hidden/reduced motion. `Frame` sanitizes inputs once
per raster; `sample(...)` remains a default-gaze convenience for individual pixels. Pose::period suggests2fps Ready,6fps Waiting,12fps active;
the host owns actual scheduling and must stop it when hidden or reduced-motion.

## Optional C / WebAssembly adapter

`cargo build -p abstract-avatar --release --features ffi` builds a native library.
Add `--target wasm32-unknown-unknown` for Wasm. See `avatar.h` for ABI1. The adapter
accepts palette lookup entries rather than application colors. It reuses fixed
RGBA buffers, caps each dimension at512 and live handles at16 (16MiB maximum live
pixel storage). No unchecked foreign writes or unsafe Rust blocks are used.

Store style keys, never numeric indices: enumerate the catalog to resolve them.
Handles are monotonic and not reused; destroyed handles return explicit errors.
A pixel pointer is borrowed until destroy. Native callers must copy before render/
destroy and synchronize concurrent pointer reads; GUI adapters use one UI thread.
Palette entries are0xRRGGBBAA, three materials(Warm,Eye,Dim),256 shades each. Set all
768 entries before rendering; updates/draws must be serialized by a native host.
Pixels are tightly packed RGBA8. Native and Wasm use exactly the same geometry.

`examples/qt_avatar.py` demonstrates a reusable QWidget and catalog/phase picker
using ctypes/PyQt5; PyQt is example-only. The Qt host supports Ready pointer tracking and2s idle expiry. Hidden widgets stop both clocks; release
owns buffer disposal. `examples/native_frames.py` exports fixtures for Wasm parity.
Production systray/widget adoption is a separate client integration.

## Web artifact

From the multi-repository workspace, run:

```
python3 abstracttui/scripts/build_avatar_artifacts.py
python3 abstracttui/scripts/build_avatar_artifacts.py --check
```

This builds the core and writes the sibling AbstractUIC artifact and provenance.
Normal npm builds verify/copy that artifact and do not require a Rust toolchain.
The UI kit exposes `AvatarRenderer`, `ActivityAvatar` and `avatarDesigns`. See its
README and live gallery. Hash verification fails for missing/changed artifact data;
`--check` also compares current Rust source, including Cargo.toml and all modules.

Appearance/voice identity, preferences and inference remain outside this crate.
