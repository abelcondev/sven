//! Seeding a proposal from a natural-language description, without invoking any
//! model — the graceful fallback for `grok-sdd propose`. Ported from
//! `internal/sdd/propose.go`.

use crate::scaffold::{DIR_NAME, scaffold};
use crate::util::{slugify, split_frontmatter};
use anyhow::{Context, bail};
use chrono::{DateTime, Utc};
use std::fs;
use std::path::Path;

/// Bounds the title derived from a free-text description so the frontmatter title
/// (and the resulting decision slug) stays reasonable.
const MAX_SEED_TITLE_LEN: usize = 72;

/// Writes `sdd/proposal.md` as an in-review skeleton seeded from a description.
/// Runs [`scaffold`] first so a missing `sdd/` is created, and refuses to clobber
/// a proposal already in review. Returns the proposal path relative to root.
pub fn seed_proposal(root: &Path, description: &str, now: DateTime<Utc>) -> anyhow::Result<String> {
    let description = description.trim();
    if description.is_empty() {
        bail!("a proposal description is required");
    }
    scaffold(root).context("scaffold sdd/")?;

    let base = root.join(DIR_NAME);
    let proposal_path = base.join("proposal.md");
    if let Ok(raw) = fs::read_to_string(&proposal_path) {
        let (fm, _) = split_frontmatter(&raw);
        let status = fm.get("status").map(|s| s.trim()).unwrap_or("");
        if !status.is_empty() && status != "empty" {
            bail!(
                "sdd/proposal.md is already in review ({status:?}); approve or clear it before drafting a new one"
            );
        }
    }

    let title = derive_title(description);
    let doc = format!(
        "---\ntype: Proposal\ntitle: {title}\ndescription: \ntags: []\nstatus: in-review\ntimestamp: {ts}\n---\n\n\
         # Proposal\n\n{description}\n\n\
         _Seeded skeleton — this file already exists; edit it in place to expand the what/why above, then fill Context and Acceptance below._\n\n\
         # Context\n\nThe forces, constraints, and alternatives considered.\n\n\
         # Acceptance\n\nWhat must be true for this proposal to be approved and promoted to `decisions/NNN-name.md`.\n",
        ts = now.format("%Y-%m-%dT%H:%M:%SZ"),
    );
    fs::write(&proposal_path, doc)?;
    Ok(format!("{DIR_NAME}/proposal.md"))
}

/// Derives the feature branch a proposal — and all of its later work — lives on,
/// from the same title slug the approved decision will carry: `sdd/prop-<slug>`.
pub fn proposal_branch_name(description: &str) -> String {
    let mut slug = slugify(&derive_title(description));
    if slug.is_empty() {
        slug = "proposal".to_string();
    }
    format!("sdd/prop-{slug}")
}

/// Turns the first sentence/line of a description into a concise frontmatter
/// title, trimmed to [`MAX_SEED_TITLE_LEN`] bytes on a word boundary.
fn derive_title(description: &str) -> String {
    let mut first = description;
    if let Some(idx) = description.find(['.', '\n'])
        && idx > 0
    {
        first = &description[..idx];
    }
    let first: String = first.split_whitespace().collect::<Vec<_>>().join(" ");
    if first.is_empty() {
        return "Untitled proposal".to_string();
    }
    if first.len() <= MAX_SEED_TITLE_LEN {
        return first;
    }
    let budget = MAX_SEED_TITLE_LEN.min(first.len());
    // Prefer a word boundary within the byte budget; else floor to a char boundary.
    let clip = match first.as_bytes()[..budget].iter().rposition(|&b| b == b' ') {
        Some(sp) if sp > 0 => sp,
        _ => {
            let mut b = budget;
            while b > 0 && !first.is_char_boundary(b) {
                b -= 1;
            }
            b
        }
    };
    format!("{}…", first[..clip].trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-07-25T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn seed_proposal_writes_in_review_skeleton() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let rel = seed_proposal(
            root,
            "Build a POS for a single pollería. Offline-first cash flow.",
            fixed(),
        )
        .unwrap();
        assert_eq!(rel, "sdd/proposal.md");
        let body = fs::read_to_string(root.join(&rel)).unwrap();
        for want in [
            "status: in-review",
            "title: Build a POS for a single pollería",
            "# Proposal",
            "Offline-first cash flow",
            "# Acceptance",
        ] {
            assert!(body.contains(want), "proposal missing {want:?}:\n{body}");
        }
    }

    #[test]
    fn seed_proposal_scaffolds_when_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        seed_proposal(root, "First idea", fixed()).unwrap();
        assert!(
            root.join("sdd/index.md").exists(),
            "expected sdd/ scaffolded"
        );
    }

    #[test]
    fn seed_proposal_refuses_to_clobber_active_proposal() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        seed_proposal(root, "First idea", fixed()).unwrap();
        let err = seed_proposal(root, "Second idea", fixed()).unwrap_err();
        assert!(
            err.to_string().contains("already in review"),
            "error = {err}"
        );
    }

    #[test]
    fn seed_proposal_requires_description() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(seed_proposal(tmp.path(), "   ", fixed()).is_err());
    }

    #[test]
    fn derive_title_truncates_on_word_boundary() {
        let long = "word ".repeat(40);
        let got = derive_title(&long);
        assert!(
            got.len() <= MAX_SEED_TITLE_LEN + "…".len(),
            "title too long ({}): {got:?}",
            got.len()
        );
        assert!(got.ends_with('…'), "expected ellipsis suffix, got {got:?}");
    }

    #[test]
    fn proposal_branch_name_cases() {
        let cases = [
            (
                "SaaS multi-tenant para pollerías en Perú. Offline.",
                "sdd/prop-saas-multi-tenant-para-polleras-en-per",
            ),
            (
                "Architecture with TypeScript and InstantDB",
                "sdd/prop-architecture-with-typescript-and-instantdb",
            ),
            ("   ", "sdd/prop-untitled-proposal"),
            (
                "Fix vuelto rounding bug",
                "sdd/prop-fix-vuelto-rounding-bug",
            ),
        ];
        for (desc, want) in cases {
            assert_eq!(
                proposal_branch_name(desc),
                want,
                "ProposalBranchName({desc:?})"
            );
        }
    }
}
