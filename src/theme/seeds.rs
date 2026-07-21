//! Faithful theme seeds: the AbstractUIC family, ported hex-for-hex.
//!
//! Source of truth: `abstractuic/ui-kit/src/theme.css` (per-theme
//! `:root.theme-*` blocks) and `theme.ts` (ids, labels, curated swatches).
//! Mapping:
//!
//! | seed field       | theme.css variable        |
//! | ---------------- | ------------------------- |
//! | `bg`             | `--bg-primary`            |
//! | `surface`        | `--bg-secondary`          |
//! | `surface_raised` | `--bg-tertiary`           |
//! | `text`           | `--text-primary`          |
//! | `text_muted`     | `--text-secondary`        |
//! | `text_faint`     | `--text-muted`            |
//! | `accent`         | `--accent`                |
//! | `ok`             | `--success`               |
//! | `warn`           | `--warning`               |
//! | `error`          | `--error`                 |
//! | `info`           | `--info`                  |
//! | `accent_alt`     | `theme.ts` `swatches[4]`  |
//!
//! `accent_alt` has no theme.css variable; the theme.ts swatch strip's
//! second accent entry is the family's curated companion color (it equals
//! `info` in the families that curate it so — a source fact, not a bug).
//! Do NOT edit hex values here without re-checking the upstream file; the
//! whole point of this table is byte-level diffability against the CSS.
//! Tokens the CSS lacks (borders, selection, cursor, link, overlay,
//! shadow, chart ramp) are derived in `registry.rs` by documented rules.
//!
//! CSS shorthand note: upstream `#eee`/`#aaa`/`#666` expand to
//! `#eeeeee`/`#aaaaaa`/`#666666` below (identical colors, grep-friendly).
//!
//! OWNER: DESIGN.

/// Raw hex seed for one theme. Strings, not `Rgba`, so this file stays a
/// pure data table diffable against `theme.css`; parsing happens once in
/// `registry::build`.
pub struct ThemeSeed {
    pub id: &'static str,
    pub label: &'static str,
    pub dark: bool,
    pub bg: &'static str,
    pub surface: &'static str,
    pub surface_raised: &'static str,
    pub text: &'static str,
    pub text_muted: &'static str,
    pub text_faint: &'static str,
    pub accent: &'static str,
    pub accent_alt: &'static str,
    pub ok: &'static str,
    pub warn: &'static str,
    pub error: &'static str,
    pub info: &'static str,
}

/// The registry order: the Abstract house pair first (abstract-dark is the
/// engine default), then dark families grouped by similarity, then light —
/// mirroring the theme.ts presentation order — then the ORIGINAL
/// AbstractTUI themes (no upstream source; authored for this engine,
/// cycle 5) at the tail. Originals ride the same derivation + audit
/// pipeline as the ports: the registry is open, and these prove it.
pub const SEEDS: [ThemeSeed; 26] = [
    // -- Abstract house palettes -------------------------------------------
    ThemeSeed {
        id: "abstract-dark",
        label: "Dark (Abstract)",
        dark: true,
        bg: "#1a1a2e",
        surface: "#16213e",
        surface_raised: "#0f3460",
        text: "#eeeeee",
        text_muted: "#aaaaaa",
        text_faint: "#666666",
        accent: "#e94560",
        accent_alt: "#60a5fa",
        ok: "#27ae60",
        warn: "#f39c12",
        error: "#e74c3c",
        info: "#60a5fa",
    },
    ThemeSeed {
        id: "abstract-light",
        label: "Light (Abstract)",
        dark: false,
        bg: "#f7f7fb",
        surface: "#ffffff",
        surface_raised: "#e6e8f0",
        text: "#0f172a",
        text_muted: "#334155",
        text_faint: "#64748b",
        accent: "#e94560",
        accent_alt: "#2563eb",
        ok: "#12883e",
        warn: "#b16105",
        error: "#dc2626",
        info: "#2563eb",
    },
    // -- Dark families ------------------------------------------------------
    ThemeSeed {
        id: "observer-night",
        label: "Observer Night",
        dark: true,
        bg: "#0b0f14",
        surface: "#10161e",
        surface_raised: "#161e28",
        text: "#d7dee8",
        text_muted: "#8593a5",
        text_faint: "#748096",
        accent: "#e8a54a",
        accent_alt: "#6ea8d8",
        ok: "#7bc98c",
        warn: "#e0af68",
        error: "#d9705a",
        info: "#6ea8d8",
    },
    ThemeSeed {
        id: "catppuccin-mocha",
        label: "Catppuccin Mocha",
        dark: true,
        bg: "#1e1e2e",
        surface: "#181825",
        surface_raised: "#313244",
        text: "#cdd6f4",
        text_muted: "#bac2de",
        text_faint: "#7e8297",
        accent: "#cba6f7",
        accent_alt: "#89b4fa",
        ok: "#a6e3a1",
        warn: "#f9e2af",
        error: "#f38ba8",
        info: "#89b4fa",
    },
    ThemeSeed {
        id: "catppuccin-macchiato",
        label: "Catppuccin Macchiato",
        dark: true,
        bg: "#24273a",
        surface: "#1e2030",
        surface_raised: "#363a4f",
        text: "#cad3f5",
        text_muted: "#b8c0e0",
        text_faint: "#8087a2",
        accent: "#c6a0f6",
        accent_alt: "#8aadf4",
        ok: "#a6da95",
        warn: "#eed49f",
        error: "#ed8796",
        info: "#8aadf4",
    },
    ThemeSeed {
        id: "catppuccin-frappe",
        label: "Catppuccin Frappe",
        dark: true,
        bg: "#303446",
        surface: "#292c3c",
        surface_raised: "#414559",
        text: "#c6d0f5",
        text_muted: "#b5bfe2",
        text_faint: "#8c93ad",
        accent: "#ca9ee6",
        accent_alt: "#8caaee",
        ok: "#a6d189",
        warn: "#e5c890",
        error: "#e78284",
        info: "#8caaee",
    },
    ThemeSeed {
        id: "rose-pine",
        label: "Rose Pine",
        dark: true,
        bg: "#191724",
        surface: "#1f1d2e",
        surface_raised: "#26233a",
        text: "#e0def4",
        text_muted: "#908caa",
        text_faint: "#88859e",
        accent: "#c4a7e7",
        accent_alt: "#31748f",
        ok: "#9ccfd8",
        warn: "#f6c177",
        error: "#eb6f92",
        info: "#3d90b1",
    },
    ThemeSeed {
        id: "rose-pine-moon",
        label: "Rose Pine Moon",
        dark: true,
        bg: "#232136",
        surface: "#2a273f",
        surface_raised: "#393552",
        text: "#e0def4",
        text_muted: "#938fac",
        text_faint: "#9390a7",
        accent: "#c4a7e7",
        accent_alt: "#3e8fb0",
        ok: "#9ccfd8",
        warn: "#f6c177",
        error: "#eb6f92",
        info: "#459bbd",
    },
    ThemeSeed {
        id: "tokyo-night",
        label: "Tokyo Night",
        dark: true,
        bg: "#1a1b26",
        surface: "#24283b",
        surface_raised: "#414868",
        text: "#c0caf5",
        text_muted: "#a9b1d6",
        text_faint: "#878fb4",
        accent: "#7aa2f7",
        accent_alt: "#2ac3de",
        ok: "#9ece6a",
        warn: "#e0af68",
        error: "#f7768e",
        info: "#2ac3de",
    },
    ThemeSeed {
        id: "nord",
        label: "Nord",
        dark: true,
        bg: "#2e3440",
        surface: "#3b4252",
        surface_raised: "#434c5e",
        text: "#eceff4",
        text_muted: "#d8dee9",
        text_faint: "#99b3cd",
        accent: "#88c0d0",
        accent_alt: "#5e81ac",
        ok: "#a3be8c",
        warn: "#ebcb8b",
        error: "#d89fa4",
        info: "#9bb0cb",
    },
    ThemeSeed {
        id: "one-dark",
        label: "One Dark",
        dark: true,
        bg: "#282c34",
        surface: "#21252b",
        surface_raised: "#3a3f4b",
        text: "#abb2bf",
        text_muted: "#9da5b4",
        text_faint: "#848c9a",
        accent: "#61afef",
        accent_alt: "#c678dd",
        ok: "#98c379",
        warn: "#e5c07b",
        error: "#e06c75",
        info: "#c678dd",
    },
    ThemeSeed {
        id: "dracula",
        label: "Dracula",
        dark: true,
        bg: "#282a36",
        surface: "#343746",
        surface_raised: "#44475a",
        text: "#f8f8f2",
        text_muted: "#d4d4de",
        text_faint: "#96a0c2",
        accent: "#ff79c6",
        accent_alt: "#8be9fd",
        ok: "#50fa7b",
        warn: "#f1fa8c",
        error: "#ff7979",
        info: "#8be9fd",
    },
    ThemeSeed {
        id: "monokai",
        label: "Monokai",
        dark: true,
        bg: "#272822",
        surface: "#2d2e27",
        surface_raised: "#3e3d32",
        text: "#f8f8f2",
        text_muted: "#e2e2dc",
        text_faint: "#9a9581",
        accent: "#66d9ef",
        accent_alt: "#a1efe4",
        ok: "#a6e22e",
        warn: "#e6db74",
        error: "#fb5d95",
        info: "#a1efe4",
    },
    ThemeSeed {
        id: "gruvbox",
        label: "Gruvbox",
        dark: true,
        bg: "#282828",
        surface: "#3c3836",
        surface_raised: "#504945",
        text: "#ebdbb2",
        text_muted: "#d5c4a1",
        text_faint: "#ada296",
        accent: "#fe8019",
        accent_alt: "#83a598",
        ok: "#b8bb26",
        warn: "#fabd2f",
        error: "#fc7f70",
        info: "#89a99d",
    },
    ThemeSeed {
        id: "solarized-dark",
        label: "Solarized Dark",
        dark: true,
        bg: "#002b36",
        surface: "#073642",
        surface_raised: "#0b4b5a",
        text: "#eee8d5",
        text_muted: "#93a1a1",
        text_faint: "#879ca3",
        accent: "#268bd2",
        accent_alt: "#2aa198",
        ok: "#8ea300",
        warn: "#c49500",
        error: "#e87775",
        info: "#2ca9a0",
    },
    ThemeSeed {
        id: "everforest-dark",
        label: "Everforest Dark",
        dark: true,
        bg: "#2b3339",
        surface: "#323c41",
        surface_raised: "#3a464c",
        text: "#d3c6aa",
        text_muted: "#a7c080",
        text_faint: "#9da8a0",
        accent: "#83c092",
        accent_alt: "#7fbbb3",
        ok: "#a7c080",
        warn: "#dbbc7f",
        error: "#e88b8d",
        info: "#7fbbb3",
    },
    // -- Light families -----------------------------------------------------
    ThemeSeed {
        id: "catppuccin-latte",
        label: "Catppuccin Latte",
        dark: false,
        bg: "#eff1f5",
        surface: "#e6e9ef",
        surface_raised: "#ccd0da",
        text: "#4c4f69",
        text_muted: "#5c5f77",
        text_faint: "#6f758b",
        accent: "#8839ef",
        accent_alt: "#1e66f5",
        ok: "#358423",
        warn: "#a46915",
        error: "#d20f39",
        info: "#1e66f5",
    },
    ThemeSeed {
        id: "rose-pine-dawn",
        label: "Rose Pine Dawn",
        dark: false,
        bg: "#faf4ed",
        surface: "#fffaf3",
        surface_raised: "#f2e9e1",
        text: "#575279",
        text_muted: "#6e6a86",
        text_faint: "#787289",
        accent: "#907aa9",
        accent_alt: "#56949f",
        ok: "#286983",
        warn: "#a76711",
        error: "#af5971",
        info: "#497e88",
    },
    ThemeSeed {
        id: "one-light",
        label: "One Light",
        dark: false,
        bg: "#fafafa",
        surface: "#ffffff",
        surface_raised: "#e5e5e6",
        text: "#383a42",
        text_muted: "#4f525d",
        text_faint: "#74757d",
        accent: "#4078f2",
        accent_alt: "#a626a4",
        ok: "#418240",
        warn: "#9d6c01",
        error: "#de3121",
        info: "#a626a4",
    },
    ThemeSeed {
        id: "everforest-light",
        label: "Everforest Light",
        dark: false,
        bg: "#fdf6e3",
        surface: "#fffbef",
        surface_raised: "#e8e0cc",
        text: "#5c6a72",
        text_muted: "#67767e",
        text_faint: "#6b7a6f",
        accent: "#2e8f6a",
        accent_alt: "#3a94c5",
        ok: "#488252",
        warn: "#9b6b21",
        error: "#ea0e09",
        info: "#317ca5",
    },
    ThemeSeed {
        id: "solarized-light",
        label: "Solarized Light",
        dark: false,
        bg: "#fdf6e3",
        surface: "#eee8d5",
        surface_raised: "#e3ddc9",
        text: "#073642",
        text_muted: "#566b72",
        text_faint: "#637880",
        accent: "#268bd2",
        accent_alt: "#2aa198",
        ok: "#6a7a00",
        warn: "#916e00",
        error: "#dc322f",
        info: "#228179",
    },
    // -- Original AbstractTUI themes (authored here, cycle 5) ---------------
    // Aurora: polar night — ice text over deep blue-black, an aurora
    // green-teal accent with a violet companion.
    ThemeSeed {
        id: "abstract-aurora",
        label: "Aurora (Abstract)",
        dark: true,
        bg: "#0a0f1a",
        surface: "#101827",
        surface_raised: "#1a2438",
        text: "#dbe7f4",
        text_muted: "#9db1c7",
        text_faint: "#6f8199",
        accent: "#43d9ad",
        accent_alt: "#9d7cf4",
        ok: "#86d780",
        warn: "#e8c26a",
        error: "#f06d7d",
        info: "#62b6f7",
    },
    // Paper: warm reading light — soft cream grounds, brown ink,
    // terracotta accent with a plum companion; semantics kept earthy.
    ThemeSeed {
        id: "abstract-paper",
        label: "Paper (Abstract)",
        dark: false,
        bg: "#f6f1e7",
        surface: "#fdf9f0",
        surface_raised: "#eae2d3",
        text: "#3d3428",
        text_muted: "#6b5d4c",
        text_faint: "#8a7d6d",
        accent: "#b4552d",
        accent_alt: "#7d5aa6",
        ok: "#55771c",
        warn: "#96660a",
        error: "#bd3934",
        info: "#2d6f8e",
    },
    // Ember: charcoal warmed by firelight — parchment text, ember
    // orange accent, gold companion.
    ThemeSeed {
        id: "abstract-ember",
        label: "Ember (Abstract)",
        dark: true,
        bg: "#16130f",
        surface: "#1d1915",
        surface_raised: "#2a241d",
        text: "#e8ded0",
        text_muted: "#b3a793",
        text_faint: "#857a68",
        accent: "#ff9d45",
        accent_alt: "#e0c068",
        ok: "#97c069",
        warn: "#e5a83d",
        error: "#ee6a5f",
        info: "#6aaed6",
    },
    // Midnight (cycle 7): the striking dark — near-black violet field,
    // electric cyan accent with a magenta companion. Photographs loud.
    ThemeSeed {
        id: "abstract-midnight",
        label: "Midnight (Abstract)",
        dark: true,
        bg: "#0a0a12",
        surface: "#12121e",
        surface_raised: "#1c1c2e",
        text: "#e6e6f0",
        text_muted: "#a2a2bd",
        text_faint: "#6c6c86",
        accent: "#4fd6e3",
        accent_alt: "#d465e8",
        ok: "#62d196",
        warn: "#eec26a",
        error: "#f2637e",
        info: "#6a9df7",
    },
    // Dawn (cycle 7): the calm light — blue-grey morning air, one calm
    // blue accent, soft violet companion. Photographs quiet.
    ThemeSeed {
        id: "abstract-dawn",
        label: "Dawn (Abstract)",
        dark: false,
        bg: "#f2f4f7",
        surface: "#fbfcfe",
        surface_raised: "#e3e7ee",
        text: "#2b3442",
        text_muted: "#55617a",
        text_faint: "#7c8699",
        accent: "#4a72c4",
        accent_alt: "#9a6bc4",
        ok: "#3d8a4e",
        warn: "#a06b12",
        error: "#c44536",
        info: "#2f7fa8",
    },
];

/// Upstream AbstractUIC theme ids that map onto renamed engine ids. The
/// house pair carries the product name here; every other id matches
/// upstream verbatim.
pub const UPSTREAM_ALIASES: [(&str, &str); 2] =
    [("dark", "abstract-dark"), ("light", "abstract-light")];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_ids_unique_and_hexes_parse() {
        let mut ids: Vec<&str> = SEEDS.iter().map(|s| s.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), SEEDS.len(), "duplicate theme id in SEEDS");

        for s in &SEEDS {
            for (field, hex) in [
                ("bg", s.bg),
                ("surface", s.surface),
                ("surface_raised", s.surface_raised),
                ("text", s.text),
                ("text_muted", s.text_muted),
                ("text_faint", s.text_faint),
                ("accent", s.accent),
                ("accent_alt", s.accent_alt),
                ("ok", s.ok),
                ("warn", s.warn),
                ("error", s.error),
                ("info", s.info),
            ] {
                assert!(
                    crate::base::Rgba::from_hex(hex).is_some(),
                    "theme {} field {} has unparseable hex {}",
                    s.id,
                    field,
                    hex
                );
            }
        }
    }

    #[test]
    fn aliases_point_at_registered_ids() {
        for (alias, target) in UPSTREAM_ALIASES {
            assert!(
                SEEDS.iter().any(|s| s.id == target),
                "alias {alias} -> missing {target}"
            );
            assert!(
                !SEEDS.iter().any(|s| s.id == alias),
                "alias {alias} shadows a real id"
            );
        }
    }
}
