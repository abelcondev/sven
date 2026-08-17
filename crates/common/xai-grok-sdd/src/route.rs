//! Deterministic implementation-route advisor. Each turn it recommends the
//! *smallest useful* route — do it inline, delegate a narrow slice, or continue
//! the active SDD proposal. The load-bearing rule: size, file count, and risk
//! NEVER select SDD on their own. Ported from the Go `internal/route`.

/// One of the three implementation routes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// Handles small, understood work inline (no delegation, no SDD).
    Direct,
    /// Hands a narrow slice to the Task tool, WITHOUT creating any SDD state.
    Delegated,
    /// Continues an already-active SDD proposal's tasks.
    Sdd,
}

impl Route {
    pub fn as_str(self) -> &'static str {
        match self {
            Route::Direct => "direct",
            Route::Delegated => "delegated",
            Route::Sdd => "sdd",
        }
    }
}

/// Returns the default route from the only signal that can be derived
/// deterministically: whether an SDD proposal is already active. SDD is never
/// selected here from size or risk.
pub fn recommend(sdd_active: bool) -> Route {
    if sdd_active {
        Route::Sdd
    } else {
        Route::Direct
    }
}

/// Renders the per-turn routing guidance block for the system prompt.
/// `changed_files` is shown only as context — it never forces SDD.
pub fn advisory(sdd_active: bool, changed_files: i64) -> String {
    if sdd_active {
        return "### Implementation route\n\n\
            An SDD proposal is active. Continue its tasks under the SDD loop \
            (`grok-sdd next`); do not start parallel ad-hoc work."
            .to_string();
    }
    format!(
        "### Implementation route\n\n\
        No SDD proposal is active ({changed_files} file(s) currently changed). Pick the \
        smallest useful route:\n\
        - **Direct (default):** 1–3 files of understood work — edit inline yourself.\n\
        - **Delegated:** 4+ files, broad exploration, or running tests/build/adversarial review — \
        delegate a narrow slice with the `Task` tool. This creates NO SDD state.\n\
        - **SDD:** only after the user explicitly asks for it or a proposal is accepted.\n\n\
        Hard rule: size, file count, and risk never select SDD on their own. \
        Never run `grok-sdd propose` just because a change looks big."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recommend_never_selects_sdd_from_size() {
        assert_eq!(recommend(false), Route::Direct);
        assert_eq!(recommend(true), Route::Sdd);
    }

    #[test]
    fn advisory_encodes_hard_rule_when_no_proposal() {
        let text = advisory(false, 7);
        for want in ["Direct", "Delegated", "Task", "never select SDD"] {
            assert!(text.contains(want), "advisory missing {want:?}:\n{text}");
        }
        assert!(
            text.contains("7 file(s)"),
            "advisory should surface the changed-file count:\n{text}"
        );
    }

    #[test]
    fn advisory_defers_to_sdd_loop_when_active() {
        let text = advisory(true, 0);
        assert!(text.contains("SDD proposal is active") && text.contains("grok-sdd next"));
    }
}
