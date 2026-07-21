//! AbstractTUI visual identity: the ~2s boot splash (3D mark rendered by
//! `three`, themed by `theme`, animated by `anim`), skippable on any key,
//! auto-skipped when not a TTY or when ABSTRACTTUI_NO_SPLASH is set.
//!
//! `identity` holds the brand constants (timeline, easing, ramp, wordmark,
//! fallback mark); `player` is the self-contained splash engine (wall-clock
//! pacing with frame drop, per-frame skip checks, hard cutoff — RT1-10);
//! `fallback2d` renders the identity in pure cells. GFX3D's 3D mark plugs
//! into the same `SplashFrameSource` seam in cycle 6. Art direction:
//! `docs/design/theme-identity.md`, section 2.
//!
//! OWNER: DESIGN (art direction) + GFX3D (3D frame source).

pub mod brandmark3d;
pub mod fallback2d;
pub mod identity;
pub mod player;

pub use brandmark3d::Brandmark3d;
pub use fallback2d::FallbackSplash;
pub use player::{
    play, play_fallback, play_source, should_splash, skip_reason, SplashFrameSource, SplashIo,
    SplashOptions, SplashOutcome, SplashWait, TerminalIo,
};
