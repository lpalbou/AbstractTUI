//! Runtime theme registration (RT1-9a): user/app themes enter the registry
//! through the same contrast audit the built-in family passes in CI.
//!
//! ## Refuse vs label — both, caller's choice
//!
//! [`register`] always RUNS the full audit (contrast floors + role
//! hygiene + decisive ground). What happens to findings is the caller's
//! declared policy:
//!
//! - [`RegisterMode::Strict`] — findings refuse the registration
//!   ([`RegisterError::Rejected`] carries the structured violation list;
//!   nothing is stored). For apps that treat their theme file as code.
//! - [`RegisterMode::Labeled`] — the theme registers anyway and the
//!   returned [`Registration::warnings`] carries one `#FALLBACK:`-prefixed
//!   line per finding. For user-supplied themes where refusing would strand
//!   the user; callers surface the warnings, never swallow them.
//!
//! Identity problems (empty/malformed id, shadowing a built-in id or an
//! upstream alias) refuse in BOTH modes: a user theme silently replacing
//! `nord` is spoofing, not a preference.
//!
//! ## Why leak-to-'static
//!
//! The damage contract (`docs/design/01-damage-contract.md` §5) fixes the
//! app-level theme handle to `Signal<&'static Theme>`. Accepted
//! registrations are therefore `Box::leak`ed:
//!
//! - a handle captured by any view/signal can never dangle, even if the
//!   same id is re-registered later (the old allocation stays valid);
//! - the cost is bounded and small (~300 bytes per accepted registration:
//!   28 tokens + id/label strings); a live theme editor re-registering on
//!   every tweak leaks per *changed* candidate only — re-registering a
//!   byte-identical candidate returns the existing handle without leaking
//!   (dedup below);
//! - the alternative (`Arc<Theme>` payloads) would ripple a contract
//!   amendment through REACT/RENDER for a problem measured in kilobytes.
//!
//! Storage is a `RwLock<Vec<&'static Theme>>` in registration order;
//! lookups scan newest-first so re-registering an id replaces it for all
//! FUTURE lookups while old handles stay alive.
//!
//! OWNER: DESIGN.

use std::sync::RwLock;

use crate::theme::contrast::{audit, Violation};
use crate::theme::registry::{hygiene_violations, themes, Theme};
use crate::theme::seeds::UPSTREAM_ALIASES;
use crate::theme::tokens::TokenSet;

/// A theme proposed for runtime registration. Owned strings: nothing leaks
/// unless the candidate is accepted.
#[derive(Clone, Debug)]
pub struct ThemeCandidate {
    /// Kebab-case machine id: `[a-z0-9]` and `-`/`_`, non-empty.
    pub id: String,
    /// Human label for pickers (empty label falls back to the id).
    pub label: String,
    /// Declared polarity; audited against the measured ground luminance.
    pub dark: bool,
    pub tokens: TokenSet,
}

/// What to do when the audit finds violations.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RegisterMode {
    /// Violations refuse the registration (nothing stored).
    Strict,
    /// Violations register anyway and come back as `#FALLBACK:` warnings.
    Labeled,
}

/// A successful registration.
#[derive(Debug)]
pub struct Registration {
    pub theme: &'static Theme,
    /// Empty in `Strict` mode by construction; in `Labeled` mode, one
    /// `#FALLBACK:` line per audit finding. Callers surface these.
    pub warnings: Vec<String>,
}

#[derive(Debug)]
pub enum RegisterError {
    /// Id is empty or contains characters outside `[a-z0-9-_]`.
    InvalidId(String),
    /// Id collides with a built-in theme or an upstream alias — refused in
    /// both modes (shadowing `nord` is spoofing, not customization).
    ReservedId(String),
    /// Strict mode: the audit found violations. Structured (RT1-9 demand:
    /// the caller gets the list, not a boolean) plus role-hygiene findings.
    Rejected {
        violations: Vec<Violation>,
        hygiene: Vec<String>,
    },
}

impl std::fmt::Display for RegisterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegisterError::InvalidId(id) => {
                write!(f, "invalid theme id '{id}': need non-empty [a-z0-9-_]")
            }
            RegisterError::ReservedId(id) => {
                write!(f, "theme id '{id}' is reserved by a built-in theme")
            }
            RegisterError::Rejected {
                violations,
                hygiene,
            } => {
                write!(
                    f,
                    "theme rejected: {} contrast violation(s), {} hygiene finding(s)",
                    violations.len(),
                    hygiene.len()
                )?;
                for v in violations {
                    write!(f, "\n  {v}")?;
                }
                for h in hygiene {
                    write!(f, "\n  {h}")?;
                }
                Ok(())
            }
        }
    }
}

/// Registered user themes, registration order. Newest-first lookup gives
/// replace-on-re-register semantics without ever invalidating old handles.
static USER_THEMES: RwLock<Vec<&'static Theme>> = RwLock::new(Vec::new());

/// Read the user list, surviving a poisoned lock (a panicked registration
/// cannot corrupt an append-only Vec of shared refs — the data is still
/// consistent, so reads continue).
fn read_user() -> Vec<&'static Theme> {
    USER_THEMES
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

/// Register a theme at runtime. Runs the full audit in every mode; see the
/// module docs for the refuse-vs-label contract.
pub fn register(
    candidate: ThemeCandidate,
    mode: RegisterMode,
) -> Result<Registration, RegisterError> {
    let id = candidate.id.trim();
    let id_ok = !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');
    if !id_ok {
        return Err(RegisterError::InvalidId(candidate.id));
    }
    let reserved = themes().iter().any(|t| t.id == id)
        || UPSTREAM_ALIASES.iter().any(|(alias, _)| *alias == id);
    if reserved {
        return Err(RegisterError::ReservedId(id.to_string()));
    }

    // Audit the candidate BEFORE anything leaks: rejected strict
    // registrations must cost zero permanent memory.
    let label = if candidate.label.trim().is_empty() {
        id.to_string()
    } else {
        candidate.label.trim().to_string()
    };
    let probe = Theme {
        // Temporary borrow trick is not possible for &'static fields; audit
        // functions take &str/&Theme, so build the audit inputs directly.
        id: "",
        label: "",
        dark: candidate.dark,
        tokens: candidate.tokens,
    };
    let violations = audit(id, &candidate.tokens);
    let mut hygiene = hygiene_probe(id, &probe);
    // The declared polarity must agree with the measured ground — a "dark"
    // theme on a white ground breaks downstream grouping (shadow strength,
    // image dithering, splash vignette).
    let measured_dark = candidate.tokens.bg.luminance() < 0.5;
    if measured_dark != candidate.dark {
        hygiene.push(format!(
            "[{}] declared {} but ground measures {}",
            id,
            if candidate.dark { "dark" } else { "light" },
            if measured_dark { "dark" } else { "light" }
        ));
    }

    let clean = violations.is_empty() && hygiene.is_empty();
    if !clean && mode == RegisterMode::Strict {
        return Err(RegisterError::Rejected {
            violations,
            hygiene,
        });
    }
    let warnings: Vec<String> = violations
        .iter()
        .map(|v| format!("#FALLBACK: registered theme {v}"))
        .chain(
            hygiene
                .iter()
                .map(|h| format!("#FALLBACK: registered theme hygiene {h}")),
        )
        .collect();

    // Dedup: a byte-identical re-registration returns the existing handle
    // (theme-editor loops leak only on change).
    if let Some(existing) = read_user().iter().rev().find(|t| t.id == id) {
        if existing.label == label
            && existing.dark == candidate.dark
            && existing.tokens == candidate.tokens
        {
            return Ok(Registration {
                theme: existing,
                warnings,
            });
        }
    }

    let theme: &'static Theme = Box::leak(Box::new(Theme {
        id: Box::leak(id.to_string().into_boxed_str()),
        label: Box::leak(label.into_boxed_str()),
        dark: candidate.dark,
        tokens: candidate.tokens,
    }));
    USER_THEMES
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .push(theme);
    Ok(Registration { theme, warnings })
}

/// Hygiene findings for a candidate under its runtime id (the probe Theme
/// carries an empty id; substitute the real one into the messages).
fn hygiene_probe(id: &str, probe: &Theme) -> Vec<String> {
    hygiene_violations(probe)
        .into_iter()
        .map(|line| line.replacen("[]", &format!("[{id}]"), 1))
        .collect()
}

/// Newest matching user theme for `id`, if any.
pub fn user_get(id: &str) -> Option<&'static Theme> {
    let id = id.trim();
    read_user().into_iter().rev().find(|t| t.id == id)
}

/// All visible user themes, registration order, deduped to the newest
/// registration per id (pickers must not show stale replaced entries).
pub fn user_list() -> Vec<&'static Theme> {
    let all = read_user();
    let mut seen: Vec<&str> = Vec::new();
    let mut out: Vec<&'static Theme> = Vec::new();
    for t in all.iter().rev() {
        if !seen.contains(&t.id) {
            seen.push(t.id);
            out.push(t);
        }
    }
    out.reverse();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::registry::{default_theme, get, resolve};

    /// Unique ids per test: the registry is process-global and tests run
    /// concurrently in one binary.
    fn candidate(id: &str) -> ThemeCandidate {
        let base = default_theme();
        ThemeCandidate {
            id: id.to_string(),
            label: format!("Test {id}"),
            dark: base.dark,
            tokens: base.tokens,
        }
    }

    #[test]
    fn strict_accepts_a_clean_theme_and_lookup_finds_it() {
        let reg = register(candidate("reg-clean"), RegisterMode::Strict).expect("clean");
        assert!(reg.warnings.is_empty());
        assert_eq!(reg.theme.id, "reg-clean");
        assert_eq!(user_get("reg-clean").unwrap().id, "reg-clean");
        // The unified lookup path sees user themes too.
        assert_eq!(get("reg-clean").unwrap().tokens, reg.theme.tokens);
        let (t, warn) = resolve("reg-clean");
        assert_eq!(t.id, "reg-clean");
        assert!(warn.is_none());
    }

    #[test]
    fn strict_refuses_a_sub_contrast_theme_and_stores_nothing() {
        let mut c = candidate("reg-bad-strict");
        c.tokens.text = c.tokens.bg; // unreadable by construction
        match register(c, RegisterMode::Strict) {
            Err(RegisterError::Rejected { violations, .. }) => {
                assert!(violations.iter().any(|v| v.rule == "text/bg"));
                assert!(violations.iter().all(|v| v.theme == "reg-bad-strict"));
            }
            other => panic!("expected Rejected, got {other:?}"),
        }
        assert!(
            user_get("reg-bad-strict").is_none(),
            "rejected must not store"
        );
    }

    #[test]
    fn labeled_registers_with_fallback_warnings() {
        let mut c = candidate("reg-bad-labeled");
        c.tokens.text_muted = c.tokens.bg;
        let reg = register(c, RegisterMode::Labeled).expect("labeled stores");
        assert!(!reg.warnings.is_empty());
        assert!(
            reg.warnings.iter().all(|w| w.starts_with("#FALLBACK")),
            "warnings must be labeled: {:?}",
            reg.warnings
        );
        assert!(user_get("reg-bad-labeled").is_some(), "labeled mode stores");
    }

    #[test]
    fn identity_problems_refuse_in_both_modes() {
        for mode in [RegisterMode::Strict, RegisterMode::Labeled] {
            assert!(matches!(
                register(candidate("nord"), mode),
                Err(RegisterError::ReservedId(_))
            ));
            // Upstream aliases are reserved too.
            assert!(matches!(
                register(candidate("dark"), mode),
                Err(RegisterError::ReservedId(_))
            ));
            assert!(matches!(
                register(candidate("Bad Id!"), mode),
                Err(RegisterError::InvalidId(_))
            ));
            assert!(matches!(
                register(candidate("  "), mode),
                Err(RegisterError::InvalidId(_))
            ));
        }
    }

    #[test]
    fn polarity_lie_is_caught() {
        let mut c = candidate("reg-polarity");
        c.dark = false; // claims light on the abstract-dark ground
        match register(c, RegisterMode::Strict) {
            Err(RegisterError::Rejected { hygiene, .. }) => {
                assert!(hygiene.iter().any(|h| h.contains("declared light")));
            }
            other => panic!("expected polarity rejection, got {other:?}"),
        }
    }

    #[test]
    fn re_register_replaces_lookup_and_dedups_identical() {
        let first = register(candidate("reg-replace"), RegisterMode::Strict).expect("v1");
        // Identical re-registration: same handle, no second entry.
        let again = register(candidate("reg-replace"), RegisterMode::Strict).expect("dedup");
        assert!(
            std::ptr::eq(first.theme, again.theme),
            "identical candidate dedups"
        );

        // Changed candidate: new handle wins lookups, old handle stays valid.
        let mut v2 = candidate("reg-replace");
        v2.label = "Test reg-replace v2".to_string();
        let second = register(v2, RegisterMode::Strict).expect("v2");
        assert!(!std::ptr::eq(first.theme, second.theme));
        assert_eq!(
            user_get("reg-replace").unwrap().label,
            "Test reg-replace v2"
        );
        assert_eq!(first.theme.id, "reg-replace", "old handle never dangles");
        // Pickers see exactly one entry for the id.
        let entries: Vec<_> = user_list()
            .into_iter()
            .filter(|t| t.id == "reg-replace")
            .collect();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].label, "Test reg-replace v2");
    }
}
