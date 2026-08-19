//! Task ceremony weight (tier) — how much design gate and review a change
//! deserves, so the loop scales effort to risk instead of treating a placeholder
//! screen like a payment flow. Ported from the Go `internal/sdd/tier.go`.

use std::collections::HashMap;

/// Tier is the ceremony weight of a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// A copy tweak, a rename, a screen composing only existing base components.
    /// Skips the design gate; a single, shallow review.
    Trivial,
    /// The default: a normal feature with some logic. Full review, but the second
    /// round runs only if the first changed code.
    Standard,
    /// Money, auth, or data mutation. Full ceremony, both review rounds always,
    /// security lens mandatory.
    Critical,
}

impl Tier {
    /// The lowercase wire form written to / read from task frontmatter.
    pub fn as_str(self) -> &'static str {
        match self {
            Tier::Trivial => "trivial",
            Tier::Standard => "standard",
            Tier::Critical => "critical",
        }
    }

    /// The concrete implement-phase loop for this tier. The engine renders this
    /// into the next-step block so the ceremony scales deterministically instead
    /// of depending on the model correctly interpreting dense skill prose (a slow
    /// model reads "scale to tier" loosely and defaults to full ceremony).
    pub fn plan(self) -> TierPlan {
        match self {
            // A copy tweak or a screen composed only of existing base components:
            // no test-first ceremony, one shallow review, and the whole tail may
            // run in a single turn.
            Tier::Trivial => TierPlan {
                tdd: false,
                review_rounds: 1,
                craft_lens: false,
                security_lens: false,
                batch: true,
            },
            // The default feature: TDD, both craft + correctness lenses, a second
            // round only if the first changed code. Checkpoint between phases.
            Tier::Standard => TierPlan {
                tdd: true,
                review_rounds: 2,
                craft_lens: true,
                security_lens: false,
                batch: false,
            },
            // Money, auth, or data mutation: full ceremony, both rounds always,
            // security lens mandatory.
            Tier::Critical => TierPlan {
                tdd: true,
                review_rounds: 2,
                craft_lens: true,
                security_lens: true,
                batch: false,
            },
        }
    }
}

/// The concrete, model-facing loop a task's tier prescribes. Every field maps to
/// one ceremony knob the review/implement skills otherwise scale by prose. A full
/// production build never runs in the fix loop for any tier — it runs once at ship
/// — so that is a global rule, not a tier knob here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TierPlan {
    /// Write the failing test first (TDD). Off for trivial (a composed screen's
    /// logic, if any, is still tested — but not test-first).
    pub tdd: bool,
    /// Maximum review rounds (round 2 runs only if round 1 changed code; critical
    /// always runs both).
    pub review_rounds: u8,
    /// Run Lens B — the fresh-context craft/maintainability reviewer.
    pub craft_lens: bool,
    /// Security lens is mandatory (never skipped) for this tier.
    pub security_lens: bool,
    /// The implement → review → ship tail may run in a single turn (no per-phase
    /// stop). True only for trivial, where the checkpoints cost more than they save.
    pub batch: bool,
}

impl std::fmt::Display for Tier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Strong, unambiguous risk signals. Kept conservative on purpose: inference
/// should escalate only when the signal is clear, and the override handles
/// anything it misses.
const CRITICAL_KEYWORDS: &[&str] = &[
    "pago",
    "pagos",
    "cobro",
    "cobrar",
    "checkout",
    "payment",
    "billing",
    "factura",
    "boleta",
    "invoice",
    "precio",
    "auth",
    "login",
    "signin",
    "sign-in",
    "token",
    "password",
    "contraseña",
    "credencial",
    "credential",
    "session",
    "sesion",
    "sesión",
    "oauth",
    "security",
    "seguridad",
    "migration",
    "migracion",
    "migración",
    "schema",
    "backup",
    "encrypt",
    "secret",
    "delete",
    "drop",
];

/// Weak signals — trivial is best set via the override; these only catch the
/// obvious chores.
const TRIVIAL_KEYWORDS: &[&str] = &[
    "typo",
    "rename",
    "renombrar",
    "chore",
    "docs",
    "placeholder",
    "alias",
    "comment",
];

/// Returns a task's tier: the frontmatter `tier:` override when set to a valid
/// value, otherwise inferred from the task's title and tags. It never silently
/// under-rates — a critical signal wins over a trivial one.
pub fn resolve_tier(task_fm: &HashMap<String, String>) -> Tier {
    if let Some(t) = parse_tier(task_fm.get("tier").map(String::as_str).unwrap_or("")) {
        return t;
    }
    let title = task_fm.get("title").map(String::as_str).unwrap_or("");
    let tags = task_fm.get("tags").map(String::as_str).unwrap_or("");
    infer_tier(&format!("{title} {tags}"))
}

/// Classifies free text by keyword. Critical is checked first so a money/auth/
/// data signal is never masked by a trivial one; trivial only applies when there
/// is no critical signal.
pub fn infer_tier(text: &str) -> Tier {
    let blob = text.to_lowercase();
    for kw in CRITICAL_KEYWORDS {
        if blob.contains(kw) {
            return Tier::Critical;
        }
    }
    for kw in TRIVIAL_KEYWORDS {
        if blob.contains(kw) {
            return Tier::Trivial;
        }
    }
    Tier::Standard
}

/// Validates a raw frontmatter/flag tier value. Empty or unknown values return
/// `None` so the caller falls back to inference.
pub fn parse_tier(raw: &str) -> Option<Tier> {
    match raw.trim().to_lowercase().as_str() {
        "trivial" => Some(Tier::Trivial),
        "standard" => Some(Tier::Standard),
        "critical" => Some(Tier::Critical),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fm(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn infer_tier_cases() {
        let cases = [
            ("PIN sign-in backend issuing staff tokens", Tier::Critical),
            ("Cobro de comanda con vuelto", Tier::Critical),
            ("Data migration: add workspaces table", Tier::Critical),
            ("Owner dashboard placeholder screen", Tier::Trivial),
            ("Rename helper for clarity", Tier::Trivial),
            ("Staff list ordering by name", Tier::Standard),
            ("", Tier::Standard),
        ];
        for (text, want) in cases {
            assert_eq!(infer_tier(text), want, "infer_tier({text:?})");
        }
    }

    #[test]
    fn infer_tier_critical_beats_trivial() {
        assert_eq!(infer_tier("auth login placeholder screen"), Tier::Critical);
    }

    #[test]
    fn tier_plan_scales_ceremony() {
        let t = Tier::Trivial.plan();
        assert!(!t.tdd && t.batch && t.review_rounds == 1 && !t.craft_lens && !t.security_lens);
        let s = Tier::Standard.plan();
        assert!(s.tdd && !s.batch && s.review_rounds == 2 && s.craft_lens && !s.security_lens);
        let c = Tier::Critical.plan();
        assert!(c.tdd && !c.batch && c.review_rounds == 2 && c.craft_lens && c.security_lens);
    }

    #[test]
    fn resolve_tier_override_wins() {
        assert_eq!(
            resolve_tier(&fm(&[
                ("title", "payment checkout flow"),
                ("tier", "trivial")
            ])),
            Tier::Trivial
        );
        assert_eq!(
            resolve_tier(&fm(&[
                ("title", "payment checkout flow"),
                ("tier", "bogus")
            ])),
            Tier::Critical
        );
        assert_eq!(
            resolve_tier(&fm(&[("title", "tweak list layout")])),
            Tier::Standard
        );
    }
}
