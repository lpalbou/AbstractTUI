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
