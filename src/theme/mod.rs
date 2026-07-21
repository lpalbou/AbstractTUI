//! Themes: semantic design tokens and the AbstractUIC theme family.
//!
//! Widgets consume [`TokenId`]s resolved against the active theme's
//! [`TokenSet`]; they never hold raw colors. The registry ports the
//! AbstractUIC palette family (abstract dark/light, observer-night,
//! catppuccin x4, rose-pine x3, tokyo-night, nord, one-dark/light,
//! dracula, monokai, gruvbox, solarized x2, everforest x2) hex-for-hex
//! from `theme.css`; tokens the CSS lacks are derived by documented,
//! contrast-guarded rules. Runtime theme switching is a signal write.
//!
//! Contrast floors and role hygiene are test-pinned (see
//! `docs/design/theme-identity.md` for the full token model and floors).
//!
//! OWNER: DESIGN.

pub mod contrast;
pub mod derive;
pub mod register;
pub mod registry;
pub mod seeds;
pub mod tokens;

pub use contrast::{audit, contrast_ratio, Violation};
pub use register::{register, RegisterError, RegisterMode, Registration, ThemeCandidate};
pub use registry::{default_theme, get, list, resolve, themes, Theme, DEFAULT_THEME_ID};
pub use tokens::{TokenId, TokenSet};
