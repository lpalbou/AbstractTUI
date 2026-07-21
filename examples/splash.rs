//! splash — the boot identity, 3D mark or 2D fallback.
//!
//! Demonstrates: the splash player (wall-clock pacing with frame drop,
//! per-frame skip, hard 2.5 s cutoff, tty/env gate) driving either frame
//! source through the same `SplashFrameSource` seam — GFX3D's three-planes
//! "A" or the pure-cell fallback. Both read the same `boot::identity`
//! constants, so timeline and wordmark can never drift.
//!
//! Source pick: `--3d` / `--2d` flags (or `ABSTRACTTUI_SPLASH=3d|2d`);
//! default AUTO = 3D on truecolor terminals, 2D otherwise. Any key skips.
//! Safe anywhere: exits instantly when not a tty / NO_COLOR / TERM=dumb.
//!
//! Try: `ABSTRACTTUI_THEME=catppuccin-mocha cargo run --example splash`
//!
//! OWNER: DESIGN.

fn main() {
    use abstracttui::boot::{
        play_source, should_splash, Brandmark3d, FallbackSplash, SplashOutcome,
    };
    use abstracttui::term::{Capabilities, EnterOptions};
    use abstracttui::theme;

    let caps = Capabilities::detect_env();
    // The one boot gate: tty + ABSTRACTTUI_NO_SPLASH + NO_COLOR +
    // TERM=dumb + caps verdict, one callable (what real apps ask).
    if let Err(reason) = should_splash(&caps) {
        println!("splash: skipped — {reason}");
        return;
    }
    let (theme, warning) = theme::resolve(
        &std::env::var("ABSTRACTTUI_THEME").unwrap_or_else(|_| theme::DEFAULT_THEME_ID.into()),
    );
    if let Some(w) = warning {
        eprintln!("{w}");
    }

    // Source selection: flags > env > auto (3D needs the color depth to
    // sell the gradient mark; 16/256-color terminals get the cell
    // fallback, which quantizes gracefully).
    let args: Vec<String> = std::env::args().collect();
    let env_pick = std::env::var("ABSTRACTTUI_SPLASH").unwrap_or_default();
    let want_3d = if args.iter().any(|a| a == "--3d") {
        true
    } else if args.iter().any(|a| a == "--2d") {
        false
    } else if env_pick == "3d" || env_pick == "three" {
        true
    } else if env_pick == "2d" {
        false
    } else {
        caps.truecolor
    };

    let mut term = match new_terminal() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("splash: no terminal: {e:?}");
            return;
        }
    };
    if let Err(e) = term.enter(&EnterOptions::default()) {
        eprintln!("splash: could not enter raw mode: {e:?}");
        return;
    }
    let outcome = if want_3d {
        let mut source = Brandmark3d::new();
        play_source(&mut *term, true, &caps, theme, &mut source)
    } else {
        let mut source = FallbackSplash::new();
        play_source(&mut *term, true, &caps, theme, &mut source)
    };
    let _ = term.leave();

    let label = if want_3d { "3d" } else { "2d" };
    match outcome {
        Ok((SplashOutcome::Completed, carry)) => {
            println!(
                "splash[{label}]: completed (2.0s timeline){}",
                note(carry.len())
            );
        }
        Ok((SplashOutcome::Skipped, carry)) => {
            println!("splash[{label}]: skipped by input{}", note(carry.len()));
        }
        Ok((SplashOutcome::CutOff, _)) => println!("splash[{label}]: hard 2.5s cutoff fired"),
        Ok((SplashOutcome::SkippedByGate(reason), _)) => println!("splash: gated — {reason}"),
        Err(e) => eprintln!("splash[{label}]: terminal error: {e:?}"),
    }
}

fn note(n: usize) -> String {
    if n == 0 {
        String::new()
    } else {
        format!("; {n} non-deliberate event(s) retained for the app")
    }
}

#[cfg(unix)]
fn new_terminal() -> abstracttui::base::Result<Box<dyn abstracttui::term::Terminal>> {
    Ok(Box::new(abstracttui::term::UnixTerminal::new()?))
}

#[cfg(windows)]
fn new_terminal() -> abstracttui::base::Result<Box<dyn abstracttui::term::Terminal>> {
    Ok(Box::new(abstracttui::term::WindowsTerminal::new()?))
}
