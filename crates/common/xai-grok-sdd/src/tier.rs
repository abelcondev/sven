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
