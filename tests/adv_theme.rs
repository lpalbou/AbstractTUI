//! REDTEAM cycle-2 attack: DESIGN's theme registry, the runtime
//! registration audit (RT1-9), the derivation helpers' contrast
//! guarantees under hostile seed palettes, and built-in family hygiene.

use abstracttui::base::Rgba;
use abstracttui::testing::Rng;
use abstracttui::theme::{
    audit, contrast_ratio, register, themes, RegisterError, RegisterMode, ThemeCandidate, TokenId,
    TokenSet,
};

fn base_tokens() -> TokenSet {
    // Start from a known-good built-in palette and corrupt from there.
    abstracttui::theme::get("abstract-dark")
        .expect("built-in exists")
        .tokens
}

fn unique_id(tag: &str) -> String {
    // Registration is process-global; ids must not collide across tests
    // (test binaries run tests in parallel threads by default).
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    format!("rt2-{tag}-{}", N.fetch_add(1, Ordering::Relaxed))
}

// ---------------------------------------------------------------------------
// Runtime registration audit (their RT1-9 deliverable).
// ---------------------------------------------------------------------------

#[test]
fn register_strict_refuses_text_equals_bg() {
    let mut tokens = base_tokens();
    let bg = tokens.get(TokenId::Bg);
    tokens.set(TokenId::Text, bg); // text vanishes entirely
    let err = register(
        ThemeCandidate {
            id: unique_id("textbg"),
            label: "Hostile".into(),
            dark: true,
            tokens,
        },
        RegisterMode::Strict,
    )
    .expect_err("text == bg must refuse in strict mode");
    match err {
        RegisterError::Rejected { .. } => {}
        other => panic!("expected structured Rejected, got {other:?}"),
    }
}

#[test]
fn register_labeled_admits_but_reports_violations() {
    let mut tokens = base_tokens();
    let bg = tokens.get(TokenId::Bg);
    tokens.set(TokenId::Text, bg);
    let id = unique_id("labeled");
    let reg = register(
        ThemeCandidate {
            id: id.clone(),
            label: "Degraded".into(),
            dark: true,
            tokens,
        },
        RegisterMode::Labeled,
    )
    .expect("labeled mode admits with warnings");
    let warnings = format!("{reg:?}");
    assert!(
        warnings.contains("FALLBACK")
            || warnings.to_lowercase().contains("warn")
            || warnings.to_lowercase().contains("violation"),
        "labeled registration must carry the violation report, got {warnings}"
    );
    // The registered theme is visible through the unified lookup.
    assert!(
        abstracttui::theme::get(&id).is_some(),
        "labeled registration must actually register"
    );
}

#[test]
fn register_rejects_reserved_and_invalid_ids() {
    for bad in ["nord", "abstract-dark", "tokyo-night"] {
        let err = register(
            ThemeCandidate {
                id: bad.into(),
                label: "Spoof".into(),
                dark: true,
                tokens: base_tokens(),
            },
            RegisterMode::Labeled, // reserved ids refuse in BOTH modes
        )
        .expect_err("shadowing a built-in must refuse");
        assert!(
            matches!(err, RegisterError::ReservedId(_)),
            "{bad}: {err:?}"
        );
    }
    for bad in ["", "Has Space", "UPPER", "emoji🎉", "../escape"] {
        let err = register(
            ThemeCandidate {
                id: bad.into(),
                label: "Bad id".into(),
                dark: true,
                tokens: base_tokens(),
            },
            RegisterMode::Strict,
        )
        .expect_err("invalid id must refuse");
        assert!(
            matches!(err, RegisterError::InvalidId(_)),
            "{bad:?} expected InvalidId, got {err:?}"
        );
    }
}

#[test]
fn register_strict_refuses_indecisive_ground() {
    let mut tokens = base_tokens();
    tokens.set(TokenId::Bg, Rgba::rgb(120, 120, 120)); // mid-gray ground
    let err = register(
        ThemeCandidate {
            id: unique_id("midgray"),
            label: "Mid".into(),
            dark: true,
            tokens,
        },
        RegisterMode::Strict,
    );
    assert!(
        err.is_err(),
        "indecisive ground must fail the audit (decisiveness rule)"
    );
}

#[test]
fn register_wrong_polarity_declaration_caught() {
    // A light palette declared dark: the audit's declared-flag check.
    let light = abstracttui::theme::get("abstract-light")
        .expect("built-in")
        .tokens;
    let res = register(
        ThemeCandidate {
            id: unique_id("liar"),
            label: "Liar".into(),
            dark: true, // lie
            tokens: light,
        },
        RegisterMode::Strict,
    );
    assert!(res.is_err(), "polarity lie must be caught by the audit");
}

// ---------------------------------------------------------------------------
// Built-in family hygiene: zero violations, every theme.
// ---------------------------------------------------------------------------

#[test]
fn every_builtin_theme_passes_the_audit_modulo_declared_exceptions() {
    use abstracttui::theme::contrast::AUDIT_EXCEPTIONS;
    // The exceptions list is a pressure valve; the audit only means
    // something if the valve stays nearly closed and every entry is used.
    assert!(
        AUDIT_EXCEPTIONS.len() <= 2,
        "audit exceptions growing ({}) — the floor is becoming a suggestion",
        AUDIT_EXCEPTIONS.len()
    );
    let mut fired: Vec<(&str, &str)> = Vec::new();
    for theme in themes() {
        let violations = audit(theme.id, &theme.tokens);
        for v in &violations {
            let excused = AUDIT_EXCEPTIONS
                .iter()
                .any(|(id, rule)| *id == theme.id && *rule == v.rule);
            assert!(
                excused,
                "built-in {} violates {} ({:.2} < {:.2}) with NO declared exception",
                theme.id, v.rule, v.measured, v.required
            );
            fired.push((theme.id, v.rule));
        }
    }
    // Staleness: every declared exception must actually fire.
    for (id, rule) in AUDIT_EXCEPTIONS {
        assert!(
            fired.iter().any(|(i, r)| i == id && r == rule),
            "stale exception ({id}, {rule}) — remove it"
        );
    }
    assert!(themes().len() >= 10, "the family should be seeded by now");
}

// ---------------------------------------------------------------------------
// Derivation helpers under hostile palettes (their confessed risk 1):
// the *_until_* helpers claim to reach floors — feed them 1000 random
// palettes and hold them to it.
// ---------------------------------------------------------------------------

#[test]
fn mix_until_contrast_upward_walk_holds_for_random_palettes() {
    use abstracttui::theme::derive::mix_until_contrast;
    let mut rng = Rng::new(0x7EAE);
    let mut reached = 0;
    for i in 0..1000 {
        let ground = Rgba::rgb(rng.byte(), rng.byte(), rng.byte());
        let ink = Rgba::rgb(rng.byte(), rng.byte(), rng.byte());
        let floor = *rng.pick(&[1.5f32, 2.0, 3.0]);
        let out = mix_until_contrast(ground, ink, ground, 0.10, 0.02, floor);
        let got = contrast_ratio(out, ground);
        // The walk ends at the ink itself (t=1) when the floor is out of
        // reach: the result must never be WORSE than the raw ink.
        let ink_ratio = contrast_ratio(ink, ground);
        if got + 0.01 >= floor {
            reached += 1;
        } else {
            assert!(
                got + 0.05 >= ink_ratio.min(floor),
                "case {i}: walk returned {got:.2}:1, raw ink {ink_ratio:.2}:1 \
                 (floor {floor}) — derivation made contrast WORSE"
            );
            assert!(
                (out.r, out.g, out.b) == (ink.r, ink.g, ink.b),
                "case {i}: floor unreachable must end AT the ink, got {} vs {}",
                out.to_hex(),
                ink.to_hex()
            );
        }
    }
    // With uniformly random ground/ink pairs the ink itself often cannot
    // clear the floor (the walk can never exceed the ink's own contrast) —
    // the hard guarantees are the per-case asserts above. This count is a
    // canary against a catastrophically broken walk, not a target.
    assert!(
        reached >= 350,
        "floor reached in only {reached}/1000 random cases — the walk looks broken"
    );
}

#[test]
fn tint_until_readable_downward_walk_holds_for_random_palettes() {
    use abstracttui::theme::derive::tint_until_readable;
    let mut rng = Rng::new(0x71E7);
    for i in 0..1000 {
        let ground = Rgba::rgb(rng.byte(), rng.byte(), rng.byte());
        let accent = Rgba::rgb(rng.byte(), rng.byte(), rng.byte());
        let text = Rgba::rgb(rng.byte(), rng.byte(), rng.byte());
        let tinted = tint_until_readable(ground, accent, text, 0.45, 0.03, 0.0, 4.5);
        let got = contrast_ratio(text, tinted);
        // At t_min = 0 the candidate IS the ground: the documented
        // convergence guarantee. The tint must never end up less readable
        // than the ground itself.
        let baseline = contrast_ratio(text, ground);
        assert!(
            got + 0.05 >= baseline.min(4.5),
            "case {i}: tinted {got:.2}:1 vs ground baseline {baseline:.2}:1 \
             (ground {} accent {} text {})",
            ground.to_hex(),
            accent.to_hex(),
            text.to_hex()
        );
    }
}

// ---------------------------------------------------------------------------
// Chart ramp separation (their confessed risk 2): categorical series must
// stay pairwise distinguishable on their theme's ground.
// ---------------------------------------------------------------------------

#[test]
fn chart_ramp_pairwise_separation_on_every_builtin() {
    for theme in themes() {
        let charts: Vec<Rgba> = theme.tokens.chart.to_vec();
        let bg = theme.tokens.get(TokenId::Bg);
        for (i, &a) in charts.iter().enumerate() {
            // Every series must be visible on the ground.
            let vs_bg = contrast_ratio(a, bg);
            assert!(
                vs_bg >= 1.8,
                "{}: chart[{i}] {} nearly invisible on bg ({vs_bg:.2}:1)",
                theme.id,
                a.to_hex()
            );
            for (j, &b) in charts.iter().enumerate().skip(i + 1) {
                let d = color_distance(a, b);
                assert!(
                    d >= 40.0,
                    "{}: chart[{i}] {} vs chart[{j}] {} too close (dist {d:.0})",
                    theme.id,
                    a.to_hex(),
                    b.to_hex()
                );
            }
        }
    }
}

/// Redmean-ish perceptual distance — decouples the test's notion of
/// "distinguishable" from any helper the theme code itself uses.
fn color_distance(a: Rgba, b: Rgba) -> f32 {
    let rmean = (a.r as f32 + b.r as f32) / 2.0;
    let dr = a.r as f32 - b.r as f32;
    let dg = a.g as f32 - b.g as f32;
    let db = a.b as f32 - b.b as f32;
    ((2.0 + rmean / 256.0) * dr * dr + 4.0 * dg * dg + (2.0 + (255.0 - rmean) / 256.0) * db * db)
        .sqrt()
}

// ---------------------------------------------------------------------------
// Splash pacing: DELIVERED as its own suite (tests/adv_splash.rs,
// cycle 3) — virtual-clock pacing honesty, drop-not-queue, hard
// ceiling, fade-over-wide-glyphs, gate reasons, register() races.
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Ink on an APP-DECLARED ground (`RunConfig::extra_grounds`).
// claim:tui-ink-on-a-declared-ground
//
// `audit` guarantees `text` reads on the theme's OWN grounds. It cannot
// say anything about a ground the theme never saw, and `extra_grounds`
// exists so an application can declare exactly that. These tests pin the
// size of the gap and the fact that `ink_on` closes it using only
// authored tokens.
// ---------------------------------------------------------------------------

/// A mid-dark declared panel — the one `examples/grounds.rs` uses.
const DECLARED_PANEL: Rgba = Rgba::rgb(41, 49, 62);
/// A bright declared panel — laurent's "black font on bright panel".
const DECLARED_BRIGHT: Rgba = Rgba::rgb(240, 200, 90);

/// The premise: reaching for `t.text` on your own panel is a coin flip.
///
/// If this ever goes to zero the registry has changed such that every
/// theme's body ink happens to read on these two grounds — which would
/// make `ink_on` unnecessary for THESE panels and still not for an
/// arbitrary one. Re-measure with a third panel before deleting anything.
#[test]
fn plain_text_on_a_declared_ground_is_unreadable_in_much_of_the_registry() {
    use abstracttui::theme::contrast::floors;
    let count = |ground: Rgba| {
        themes()
            .iter()
            .filter(|t| contrast_ratio(t.tokens.text, ground) < floors::TEXT)
            .count()
    };
    let (dark_fails, bright_fails) = (count(DECLARED_PANEL), count(DECLARED_BRIGHT));
    assert_eq!(
        (dark_fails, bright_fails),
        (8, 19),
        "the declared-ground readability gap CHANGED. This is the reason \
         `ink_on` exists; it is not a contract that it stay broken."
    );
    // The worst instance, named so a regression cannot hide behind a count.
    let worst = themes()
        .iter()
        .find(|t| t.id == "solarized-light")
        .expect("solarized-light is the headline case");
    let c = contrast_ratio(worst.tokens.text, DECLARED_PANEL);
    assert!(
        c < 1.1,
        "solarized-light body text on the declared panel measured {c:.2} — \
         it was 1.01, text the same colour as the panel it sits on"
    );
}

/// `ink_on` clears the text floor wherever ANY authored ink can, and
/// reports it when none can.
#[test]
fn ink_on_clears_the_text_floor_wherever_an_authored_ink_can() {
    use abstracttui::theme::contrast::{floors, ink_on};
    let mut unrescuable = vec![];
    let mut total = 0;
    for t in themes() {
        for ground in [DECLARED_PANEL, DECLARED_BRIGHT] {
            total += 1;
            let ink = ink_on(&t.tokens, ground);
            // Whatever it returns must be a colour the THEME authored.
            assert!(
                ink.color == t.tokens.text || ink.color == t.tokens.bg,
                "{} : ink_on minted a colour that is in no token",
                t.id
            );
            // It must never be beaten by the naive choice it replaces.
            assert!(
                ink.contrast >= contrast_ratio(t.tokens.text, ground) - 1e-6,
                "{} : ink_on picked worse than plain t.text",
                t.id
            );
            if ink.contrast < floors::TEXT {
                unrescuable.push(format!("{} {:.2}", t.id, ink.contrast));
            }
        }
    }
    assert_eq!(
        total, 52,
        "registry size changed; re-measure the counts below"
    );
    assert_eq!(
        unrescuable.len(),
        1,
        "the set of theme/ground pairs NO authored ink can serve changed: \
         {unrescuable:?}"
    );
    assert!(
        unrescuable[0].starts_with("everforest-light"),
        "expected everforest-light (a soft light palette holds no ink dark \
         enough for a bright panel), got {:?}",
        unrescuable[0]
    );
}

/// The polarity actually flips — this is the whole behaviour, and a
/// helper that always returned `text` would pass the two tests above on
/// the themes where `text` already wins.
#[test]
fn ink_on_flips_polarity_between_a_light_and_a_dark_theme() {
    use abstracttui::theme::contrast::ink_on;
    let pick = |id: &str, ground: Rgba| {
        let t = themes().iter().find(|t| t.id == id).expect("theme").tokens;
        ink_on(&t, ground).token
    };
    // A dark theme on a BRIGHT panel must reach for its background pole.
    assert_eq!(pick("abstract-dark", DECLARED_BRIGHT), TokenId::Bg);
    // ...and on a dark panel, for its text pole.
    assert_eq!(pick("abstract-dark", DECLARED_PANEL), TokenId::Text);
    // A light theme is the mirror image.
    assert_eq!(pick("one-light", DECLARED_PANEL), TokenId::Bg);
    assert_eq!(pick("one-light", DECLARED_BRIGHT), TokenId::Text);
}

// ---------------------------------------------------------------------
// border: guaranteed on `bg`, drawn on everything else
// ---------------------------------------------------------------------
//
// CHARACTERIZATION, NOT A GUARANTEE. `border` is derived as
// `mix_until_contrast(bg, text, bg, ..., floors::BORDER)` — it earns its
// floor against `bg` and against nothing else. The documented panel
// recipe in `widgets::block` is `.fill(t.surface)`, so the idiomatic
// bordered container puts that border on a ground its derivation never
// looked at.
//
// The three tests below pin what the engine does TODAY so that fixing it
// has something to turn red. When the derivation starts earning its floor
// on the grounds it is drawn on, they go red BY DESIGN: invert them into
// guarantees, do not delete them. A deleted characterization test is how
// a fix ships without anyone measuring what it moved.

/// The guarantee that exists, restated here as the baseline the other two
/// are measured against. Duplicates `registry::borders_stay_subtle_not_
/// shouting` on purpose: that test is inside the module that derives the
/// value, this one is outside it, and the pair is the point.
#[test]
fn border_clears_its_floor_on_bg_in_every_theme() {
    use abstracttui::theme::contrast::floors;
    for t in themes() {
        let r = contrast_ratio(t.tokens.border, t.tokens.bg);
        assert!(
            r >= floors::BORDER,
            "[{}] border on bg measures {r:.3} (floor {})",
            t.id,
            floors::BORDER
        );
        assert!(r < 3.2, "[{}] border shouting on bg at {r:.3}", t.id);
    }
}

/// THE DEFECT. A `Block` filled with `t.surface` — the recipe in the
/// widget's own module docs — draws a border that misses the floor in
/// well over half the registry.
#[test]
fn border_on_surface_misses_the_floor_in_15_of_26_themes() {
    use abstracttui::theme::contrast::floors;
    let mut below: Vec<(String, f32)> = themes()
        .iter()
        .map(|t| {
            (
                t.id.to_string(),
                contrast_ratio(t.tokens.border, t.tokens.surface),
            )
        })
        .filter(|(_, r)| *r < floors::BORDER)
        .collect();
    below.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

    assert_eq!(
        themes().len(),
        26,
        "the pinned counts below are against a 26-theme registry"
    );
    assert_eq!(
        below.len(),
        15,
        "border/surface below {}: expected the pinned 15, got {} — {below:?}. \
         If a fix landed, invert this test into a guarantee.",
        floors::BORDER,
        below.len()
    );
    let (worst_id, worst) = &below[0];
    assert_eq!(worst_id, "gruvbox", "worst offender moved: {below:?}");
    assert!(
        (1.20..1.22).contains(worst),
        "gruvbox border/surface measured {worst:.3}, pinned at ~1.209"
    );
}

/// Worse, and not in the row that opened this: on `surface_raised` — the
/// ground `Code`, `Badge`, `Progress` and the drawer panels sit on — the
/// border clears the floor in ZERO of 26. Any fix that guarantees
/// {bg, surface} and stops there leaves this whole set unaddressed.
#[test]
fn border_on_surface_raised_misses_the_floor_in_every_theme() {
    use abstracttui::theme::contrast::floors;
    let mut best = (0.0f32, String::new());
    for t in themes() {
        let r = contrast_ratio(t.tokens.border, t.tokens.surface_raised);
        assert!(
            r < floors::BORDER,
            "[{}] border on surface_raised measures {r:.3} — that is ABOVE \
             the floor, which this test pins as impossible today. A fix \
             landed: invert this into a guarantee.",
            t.id
        );
        if r > best.0 {
            best = (r, t.id.to_string());
        }
    }
    assert!(
        best.0 < floors::BORDER,
        "best case {} at {:.3} clears the floor",
        best.1,
        best.0
    );
    assert!(
        (1.38..1.40).contains(&best.0),
        "best case moved: {} at {:.3}, pinned at ~1.388",
        best.1,
        best.0
    );
}
