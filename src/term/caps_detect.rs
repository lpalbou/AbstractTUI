//! Passive environment detection for [`Capabilities`]: the TERM /
//! TERM_PROGRAM / COLORTERM evidence pass. `#[path]` sibling of
//! caps.rs (file-size split) — `detect_env` stays a `Capabilities`
//! method; only the file moved.
//!
//! OWNER: PROBE.

use super::Capabilities;

impl Capabilities {
    /// Passive detection from the process environment.
    ///
    /// Free and instant — run it before first paint; the active probe
    /// (`input::probe_active`) upgrades the result concurrently with the
    /// first frame (design doc §2.3).
    ///
    /// ```
    /// use abstracttui::term::Capabilities;
    ///
    /// let caps = Capabilities::detect_env();
    /// // Feed the presenter and the graphics ladder from the same facts:
    /// let present = caps.present_caps();
    /// let gfx = caps.graphics();
    /// println!("{}", caps.summary_line()); // e.g. "truecolor, kitty-kbd, …"
    /// # let _ = (present, gfx);
    /// ```
    pub fn detect_env() -> Self {
        Self::detect_env_with(&|k| std::env::var(k).ok())
    }

    /// Same, with an injectable lookup so tests never touch process env.
    pub fn detect_env_with(lookup: &dyn Fn(&str) -> Option<String>) -> Self {
        let get = |k: &str| lookup(k).unwrap_or_default();
        let term = get("TERM").to_ascii_lowercase();
        let term_program = get("TERM_PROGRAM");
        let colorterm = get("COLORTERM").to_ascii_lowercase();

        let mut c = Capabilities::default();

        // A terminal that identifies as dumb (or nothing at all) gets
        // nothing; emitting escapes at a dumb terminal is worse than plain
        // text. The linux console parses CSI but has no mouse/paste/focus.
        let dumb = term.is_empty() && term_program.is_empty() || term == "dumb";
        let linux_console = term == "linux";

        c.in_tmux =
            !get("TMUX").is_empty() || term.starts_with("tmux") || term.starts_with("screen");

        let kitty = !get("KITTY_WINDOW_ID").is_empty() || term == "xterm-kitty";
        let wezterm = term_program == "WezTerm" || !get("WEZTERM_EXECUTABLE").is_empty();
        let ghostty = term_program == "ghostty" || !get("GHOSTTY_RESOURCES_DIR").is_empty();
        let iterm2 = term_program == "iTerm.app" || !get("ITERM_SESSION_ID").is_empty();
        let windows_terminal = !get("WT_SESSION").is_empty();
        let apple_terminal = term_program == "Apple_Terminal";
        // TERM_PROGRAM=vscode: VS Code's integrated terminal and its
        // forks (Cursor, VSCodium…) — all xterm.js. Env evidence covers
        // truecolor, OSC 8 hyperlinks (xterm.js >= 4.3) and focus
        // reporting (DEC 1004) through the `modern` set below. NOT
        // claimed from env: OSC 52 (settings-gated permission prompt),
        // kitty keyboard/graphics and sixel (absent or addon, off by
        // default) — the active probe is the only door for those.
        let vscode = term_program == "vscode";
        let foot = term == "foot" || term.starts_with("foot-");
        let vte_version: u32 = get("VTE_VERSION").parse().unwrap_or(0);
        let modern = kitty || wezterm || ghostty || iterm2 || windows_terminal || foot || vscode;

        c.truecolor =
            colorterm == "truecolor" || colorterm == "24bit" || term.contains("direct") || modern;
        c.colors_256 = c.truecolor || term.contains("256color") || apple_terminal;

        // Kitty keyboard: claimed only for terminals that speak it OUT OF
        // THE BOX (it is part of their identity). WezTerm supports the
        // protocol but ships `enable_kitty_keyboard = false` by default —
        // an env claim there over-promises exactly where the user did not
        // configure it (backlog 0293), so WezTerm's claim is evidence-
        // gated: the active probe's `CSI ? u` raises it when the user
        // enabled the protocol, and the driver pushes the enter flags at
        // that moment (`Terminal::set_kitty_keyboard`).
        c.kitty_keyboard = kitty || ghostty || foot;
        // WezTerm's kitty-graphics support is partial; its iTerm2 path is
        // complete, so we prefer that there and let the active probe raise
        // kitty_graphics only when the terminal proves it.
        c.kitty_graphics = kitty || ghostty;
        c.iterm2_images = iterm2 || wezterm;
        c.sixel = foot || term.contains("sixel");
        c.sync_output_2026 = modern;
        c.hyperlinks = modern || vte_version >= 5000;
        // SGR 4:3 undercurl: kitty lineage + VTE 0.52+ (52xx) + iTerm2 +
        // Windows Terminal (1.18+; env cannot see versions, accept).
        c.undercurl = kitty
            || wezterm
            || ghostty
            || foot
            || iterm2
            || windows_terminal
            || vte_version >= 5200;
        c.underline_color = c.undercurl; // same lineage, same evidence today
                                         // OSC 52 write: default-on in the kitty/wezterm/ghostty/foot/iterm2
                                         // lineage and Windows Terminal; xterm gates it behind allowWindowOps
                                         // (off by default) and VTE only grew it recently — both stay false
                                         // without evidence. tmux translates OSC 52 itself (set-clipboard
                                         // defaults to "external"), so in_tmux does not clear it.
        c.osc52_copy = kitty || wezterm || ghostty || foot || iterm2 || windows_terminal;
        // Desktop notifications, two dialects: OSC 9 (iTerm2 convention:
        // iTerm2/WezTerm/ghostty) and OSC 99 (kitty's protocol — kitty
        // never adopted OSC 9). ghostty speaks both; it stays on OSC 9 so
        // it can never double-notify. foot's OSC 777 remains deferred
        // until a consumer asks.
        c.osc9_notify = iterm2 || wezterm || ghostty;
        c.osc99_notify = kitty;

        // Near-universal in the modern era; the exceptions are terminals
        // that predate the modern era entirely.
        let interactive = !dumb && !linux_console;
        c.sgr_mouse = interactive;
        c.bracketed_paste = interactive;
        c.focus_events = interactive && !apple_terminal;

        // The user's explicit no-color request outranks terminal ability
        // (informal NO_COLOR spec: any non-empty value counts).
        c.no_color = !get("NO_COLOR").is_empty();
        if c.no_color {
            c.truecolor = false;
            c.colors_256 = false;
        }

        if c.in_tmux {
            // Graphics escapes reach the outer terminal only through
            // passthrough wrapping (`term::tmux_wrap`) AND the user's
            // allow-passthrough setting, which is off by default since
            // tmux 3.3a and undetectable from env. Claiming support would
            // draw garbage on default configs: disabled, labeled, and the
            // passthrough need is recorded for a verified path later.
            c.kitty_graphics = false;
            c.iterm2_images = false;
            c.sixel = false;
            c.needs_tmux_passthrough = true;
            if term_program == "tmux" {
                let v = get("TERM_PROGRAM_VERSION");
                if !v.is_empty() {
                    c.tmux_version = Some(v);
                }
            }
        }

        #[cfg(windows)]
        {
            c.unicode_ok = true; // enter() sets the UTF-8 codepage.
                                 // RT1-12b: classic conhost does not translate mouse into VT
                                 // sequences under ENABLE_VIRTUAL_TERMINAL_INPUT — mouse would
                                 // be silently dead. Claim SGR mouse only inside a terminal
                                 // that identified itself as a modern emulator (Windows
                                 // Terminal, kitty, WezTerm, ghostty, iTerm2, foot, VS Code,
                                 // or anything setting TERM_PROGRAM); bare conhost degrades
                                 // to keyboard-only, honestly.
            c.sgr_mouse = c.sgr_mouse && (modern || !term_program.is_empty());
        }
        #[cfg(not(windows))]
        {
            let locale = [get("LC_ALL"), get("LC_CTYPE"), get("LANG")]
                .into_iter()
                .find(|v| !v.is_empty())
                .unwrap_or_default()
                .to_ascii_lowercase();
            c.unicode_ok = locale.contains("utf-8") || locale.contains("utf8");
        }

        if dumb {
            let unicode_ok = c.unicode_ok;
            let in_tmux = c.in_tmux;
            let no_color = c.no_color;
            let needs_tmux_passthrough = c.needs_tmux_passthrough;
            let tmux_version = c.tmux_version.take();
            c = Capabilities::default();
            c.unicode_ok = unicode_ok;
            c.in_tmux = in_tmux;
            c.no_color = no_color;
            c.needs_tmux_passthrough = needs_tmux_passthrough;
            c.tmux_version = tmux_version;
            c.dumb = true;
        }
        c
    }
}
