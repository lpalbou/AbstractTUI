//! capture — the deterministic screenshot pipeline for the docs cycle.
//!
//! Runs the shipped examples under a real pty at fixed sizes/themes,
//! interprets the emitted bytes with the testing rig's `VtScreen`, and
//! dumps two artifacts per shot into `docs/captures/`:
//!
//!   `<name>.txt`         plain-text screen render (fenced "screenshot")
//!   `<name>.styled.txt`  deterministic styled dump (text + style runs)
//!
//! Also produces `themes-table.md` (all registered themes with their
//! token hex values, straight from the registry) and in-process splash
//! frames (2D + 3D at t = 1.0 s) — no pty involved for those.
//!
//! Usage:
//!   cargo build --examples                    # shots run the built binaries
//!   cargo run --example capture               # everything
//!   cargo run --example capture -- themes     # just themes-table.md
//!   cargo run --example capture -- splash     # just the splash frames
//!   cargo run --example capture -- shots      # just the pty captures
//!
//! Determinism: sizes, themes, and demo data are fixed (the dashboard
//! honors `ABSTRACTTUI_FIXED_CLOCK`/`ABSTRACTTUI_START_THEME`); the one
//! honest wobble is wall-clock frame pacing — a tick more or fewer of
//! animated data between regenerations. Regenerate deliberately and
//! diff by eye; these are documentation stills, not golden tests.

use std::fs;
use std::path::{Path, PathBuf};

use abstracttui::base::Size;
use abstracttui::boot::{Brandmark3d, FallbackSplash, SplashFrameSource};
use abstracttui::render::{Cell, FrameDiff, PresentCaps, Presenter, Surface};
use abstracttui::testing::VtScreen;
use abstracttui::theme::{default_theme, themes, TokenSet};

/// One pty capture: which example, at what size, with which env, how
/// long to let it settle before the quit key lands.
struct Shot {
    /// Artifact base name in docs/captures/.
    name: &'static str,
    /// Example binary name under target/…/examples/.
    example: &'static str,
    cols: i32,
    rows: i32,
    /// Settle time before `keys` are typed into the pty.
    delay_ms: u64,
    /// Bytes typed after the delay (usually just `q`).
    keys: &'static str,
    /// Extra env for the child (TERM/COLORTERM are always set).
    env: &'static [(&'static str, &'static str)],
}

/// Fixed demo clock: 12:34:56 UTC — reads well in a screenshot.
const FIXED_CLOCK: (&str, &str) = ("ABSTRACTTUI_FIXED_CLOCK", "45296");

const SHOTS: &[Shot] = &[
    Shot {
        name: "hello",
        example: "hello",
        cols: 80,
        rows: 24,
        delay_ms: 1500,
        keys: "q",
        env: &[],
    },
    Shot {
        name: "dashboard-dark",
        example: "dashboard",
        cols: 120,
        rows: 35,
        delay_ms: 3500,
        keys: "q",
        env: &[FIXED_CLOCK],
    },
    Shot {
        name: "dashboard-dawn",
        example: "dashboard",
        cols: 120,
        rows: 35,
        delay_ms: 3500,
        keys: "q",
        env: &[FIXED_CLOCK, ("ABSTRACTTUI_START_THEME", "abstract-dawn")],
    },
    Shot {
        name: "themes",
        example: "themes",
        cols: 110,
        rows: 30,
        delay_ms: 1800,
        keys: "q",
        env: &[],
    },
    Shot {
        name: "widgets",
        example: "widgets",
        cols: 110,
        rows: 32,
        delay_ms: 1800,
        keys: "q",
        env: &[],
    },
    Shot {
        name: "components",
        example: "components",
        cols: 100,
        rows: 30,
        delay_ms: 1800,
        keys: "q",
        env: &[],
    },
    Shot {
        name: "grid",
        example: "grid",
        cols: 110,
        rows: 30,
        delay_ms: 1800,
        keys: "q",
        env: &[],
    },
    Shot {
        name: "gallery",
        example: "gallery",
        cols: 112,
        rows: 32,
        delay_ms: 1800,
        keys: "q",
        env: &[],
    },
    Shot {
        name: "viewer3d",
        example: "viewer3d",
        cols: 100,
        rows: 30,
        delay_ms: 2000,
        keys: "q",
        env: &[],
    },
    Shot {
        name: "images",
        example: "images",
        cols: 100,
        rows: 30,
        delay_ms: 1800,
        keys: "q",
        env: &[],
    },
];

fn main() {
    let what = std::env::args().nth(1).unwrap_or_else(|| "all".into());
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/captures");
    if let Err(e) = fs::create_dir_all(&out) {
        eprintln!("capture: cannot create {}: {e}", out.display());
        std::process::exit(1);
    }
    let mut produced: Vec<String> = Vec::new();
    if matches!(what.as_str(), "all" | "themes") {
        produced.push(themes_table(&out));
    }
    if matches!(what.as_str(), "all" | "splash") {
        produced.extend(splash_frames(&out));
    }
    if matches!(what.as_str(), "all" | "shots") {
        produced.extend(pty_shots(&out));
    }
    if produced.is_empty() {
        eprintln!("capture: nothing produced (arg {what:?}; expected all|themes|splash|shots)");
        std::process::exit(1);
    }
    // The manifest indexes the DIRECTORY, not this run — partial runs
    // (`-- shots`) must not delist the other artifact families.
    write_manifest(&out);
    println!("capture: {} artifacts in {}", produced.len(), out.display());
}

// ---------------------------------------------------------------- themes

/// `themes-table.md`: a scannable summary row per theme (the headline
/// tokens), then a full per-theme token listing. Deterministic registry
/// walk — the docs embed this verbatim.
fn themes_table(out: &Path) -> String {
    let mut md = String::new();
    md.push_str("# Theme reference\n\n");
    md.push_str("Generated by `cargo run --example capture -- themes` — do not edit.\n\n");
    md.push_str("## Summary\n\n");
    md.push_str(
        "| id | label | polarity | bg | surface | text | accent | ok | warn | error | info |\n",
    );
    md.push_str("|---|---|---|---|---|---|---|---|---|---|---|\n");
    for th in themes() {
        let t = &th.tokens;
        md.push_str(&format!(
            "| `{}` | {} | {} | `{}` | `{}` | `{}` | `{}` | `{}` | `{}` | `{}` | `{}` |\n",
            th.id,
            th.label,
            if th.is_dark() { "dark" } else { "light" },
            t.bg.to_hex(),
            t.surface.to_hex(),
            t.text.to_hex(),
            t.accent.to_hex(),
            t.ok.to_hex(),
            t.warn.to_hex(),
            t.error.to_hex(),
            t.info.to_hex(),
        ));
    }
    md.push_str("\n## Full token values\n");
    for th in themes() {
        md.push_str(&format!(
            "\n### {} (`{}`, {})\n\n| token | hex |\n|---|---|\n",
            th.label,
            th.id,
            if th.is_dark() { "dark" } else { "light" }
        ));
        for (name, value) in token_rows(&th.tokens) {
            md.push_str(&format!("| {name} | `{value}` |\n"));
        }
    }
    let path = out.join("themes-table.md");
    fs::write(&path, md).expect("write themes-table.md");
    rel(&path)
}

/// Every token as (name, hex) — chart entries flattened. Order mirrors
/// `TokenId::ALL` for scannability.
fn token_rows(t: &TokenSet) -> Vec<(String, String)> {
    use abstracttui::theme::TokenId;
    TokenId::ALL
        .iter()
        .map(|id| (id.name().to_string(), t.get(*id).to_hex()))
        .collect()
}

// ---------------------------------------------------------------- splash

/// In-process splash stills: both frame sources render two beats — t =
/// 1.0 s (just after the alignment burst, sparks in flight) and t =
/// 1.95 s (wordmark settled, afterglow gone) — into a Surface;
/// a full diff against an empty surface plus the Presenter turns that
/// into the same bytes a terminal would receive, and `VtScreen` renders
/// them. History-dependent trails get a monotonic render walk.
fn splash_frames(out: &Path) -> Vec<String> {
    let size = Size::new(100, 30);
    let theme = default_theme();
    let mut produced = Vec::new();
    let mut sources: [(&str, Box<dyn SplashFrameSource>); 2] = [
        ("splash-2d", Box::new(FallbackSplash::new())),
        ("splash-3d", Box::new(Brandmark3d::new())),
    ];
    for (name, source) in sources.iter_mut() {
        for (t, suffix) in [(1.0f32, ""), (1.95, "-reveal")] {
            let surface = source.render(t, size, theme);
            let screen = surface_screen(surface, size);
            produced.extend(write_shot(out, &format!("{name}{suffix}"), &screen));
        }
    }
    produced
}

/// Surface -> terminal bytes -> interpreted screen.
fn surface_screen(surface: &Surface, size: Size) -> VtScreen {
    let empty = Surface::new(size, Cell::EMPTY);
    let mut diff = FrameDiff::new();
    let runs: Vec<_> = diff.compute_full(&empty, surface).to_vec();
    let mut presenter = Presenter::new();
    let mut bytes = Vec::new();
    presenter.emit(&runs, surface, &PresentCaps::FULL, &mut bytes);
    let mut screen = VtScreen::new(size);
    screen.feed(&bytes);
    screen
}

// ----------------------------------------------------------------- shots

/// Run each example binary under a real pty (`script`), interpret the
/// bytes it wrote, and dump the LAST alternate-screen frame (bytes are
/// truncated at the final `1049l` so the app's exit doesn't wipe the
/// capture back to the primary screen).
#[cfg(unix)]
fn pty_shots(out: &Path) -> Vec<String> {
    let mut produced = Vec::new();
    let bin_dir = example_bin_dir();
    for shot in SHOTS {
        let bin = bin_dir.join(shot.example);
        if !bin.exists() {
            eprintln!(
                "capture: {} missing ({}) — run `cargo build --examples` first",
                shot.example,
                bin.display()
            );
            continue;
        }
        match run_pty(shot, &bin) {
            Ok(raw) => {
                if raw.is_empty() {
                    eprintln!("capture: {} produced no bytes — skipped", shot.name);
                    continue;
                }
                // Asset-guarded examples print a plain-text notice and
                // exit without entering the alternate screen.
                if !raw.windows(8).any(|w| w == b"\x1b[?1049h") {
                    let text = String::from_utf8_lossy(&raw);
                    eprintln!(
                        "capture: {} never entered the app screen — skipped ({})",
                        shot.name,
                        text.lines().next().unwrap_or("no output").trim()
                    );
                    continue;
                }
                let cut = raw
                    .windows(8)
                    .rposition(|w| w == b"\x1b[?1049l")
                    .unwrap_or(raw.len());
                let mut screen = VtScreen::new(Size::new(shot.cols, shot.rows));
                screen.feed(&raw[..cut]);
                produced.extend(write_shot(out, shot.name, &screen));
            }
            Err(e) => eprintln!("capture: {} failed: {e}", shot.name),
        }
    }
    produced
}

#[cfg(not(unix))]
fn pty_shots(_out: &Path) -> Vec<String> {
    eprintln!("capture: pty shots need a unix `script`; themes/splash artifacts still work");
    Vec::new()
}

/// Drive one binary under `script -q` with the pty sized via `stty`,
/// keys typed after a settle delay. macOS and util-linux `script` spell
/// the command differently; both are handled.
#[cfg(unix)]
fn run_pty(shot: &Shot, bin: &Path) -> std::io::Result<Vec<u8>> {
    let raw_path = std::env::temp_dir().join(format!("abstracttui-capture-{}.raw", shot.name));
    let _ = fs::remove_file(&raw_path);
    let inner = format!(
        "stty rows {} cols {}; exec {}",
        shot.rows,
        shot.cols,
        sh_quote(&bin.to_string_lossy())
    );
    let script_cmd = if cfg!(target_os = "macos") {
        format!(
            "script -q {} sh -c {}",
            sh_quote(&raw_path.to_string_lossy()),
            sh_quote(&inner)
        )
    } else {
        format!(
            "script -q -c {} {}",
            sh_quote(&inner),
            sh_quote(&raw_path.to_string_lossy())
        )
    };
    let line = format!(
        "(sleep {}; printf '%s' {}) | {} >/dev/null 2>&1",
        shot.delay_ms as f64 / 1000.0,
        sh_quote(shot.keys),
        script_cmd
    );
    let mut cmd = std::process::Command::new("sh");
    cmd.arg("-c").arg(&line);
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    cmd.env_remove("NO_COLOR");
    cmd.env("ABSTRACTTUI_NO_SPLASH", "1");
    for (k, v) in shot.env {
        cmd.env(k, v);
    }
    let status = cmd.status()?;
    if !status.success() {
        eprintln!(
            "capture: {} exited {status} (continuing — bytes may exist)",
            shot.name
        );
    }
    fs::read(&raw_path)
}

/// Minimal POSIX single-quote escaping.
#[cfg(unix)]
fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

#[cfg(unix)]
fn example_bin_dir() -> PathBuf {
    // capture itself lives in target/…/examples/ — siblings are there too.
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("target/debug/examples"))
}

// ------------------------------------------------------------- artifacts

/// Write both views of a screen; returns the relative artifact names.
fn write_shot(out: &Path, name: &str, screen: &VtScreen) -> Vec<String> {
    let text_path = out.join(format!("{name}.txt"));
    let styled_path = out.join(format!("{name}.styled.txt"));
    fs::write(&text_path, screen.to_text()).expect("write text capture");
    fs::write(&styled_path, screen.to_styled_dump()).expect("write styled capture");
    vec![rel(&text_path), rel(&styled_path)]
}

fn rel(p: &Path) -> String {
    p.strip_prefix(Path::new(env!("CARGO_MANIFEST_DIR")))
        .unwrap_or(p)
        .display()
        .to_string()
}

/// `docs/captures/README.md`: what exists, how it was made, how to
/// regenerate — the docs cycle's index into this material. Lists the
/// directory contents so partial regenerations keep the index whole.
fn write_manifest(out: &Path) {
    let mut names: Vec<String> = fs::read_dir(out)
        .map(|it| {
            it.filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n != "README.md")
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    let mut md = String::new();
    md.push_str("# Captures\n\n");
    md.push_str(
        "Deterministic text \"screenshots\" of the shipped examples, generated by\n\
         `cargo build --examples && cargo run --example capture`. Each shot has a\n\
         plain render (`.txt`) and a styled dump (`.styled.txt`: text plus style\n\
         runs). `themes-table.md` lists every registered theme's token values.\n\n\
         Sizes and demo data are fixed; wall-clock frame pacing may shift animated\n\
         data by a tick between regenerations — regenerate deliberately, diff by eye.\n\n",
    );
    md.push_str("| artifact |\n|---|\n");
    for name in names {
        md.push_str(&format!("| `{name}` |\n"));
    }
    fs::write(out.join("README.md"), md).expect("write captures README");
}
