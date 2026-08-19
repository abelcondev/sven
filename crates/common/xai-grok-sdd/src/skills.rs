//! The SDD phase skills, embedded in the binary and self-extracted to
//! `$GROK_HOME/skills/` on startup. This is what makes the loop ship in a single
//! `grok` binary — no separate installer, no `sdd-ext/` layer. Each skill is a
//! prompt file the agent loads on demand (`sdd-implement`, `sdd-review`, …); the
//! `sdd` tool's next-step hints name them. grok-build discovers them from
//! `$GROK_HOME/skills/<name>/SKILL.md` like any user skill.

use std::path::Path;

/// The marker recording the binary version whose skills were last written, so a
/// version bump re-extracts fresh bodies without rewriting on every startup.
const VERSION_MARKER: &str = ".sdd_skills_version";

/// The marker (under a project's `.grok-sdd/`) recording the version whose rules
/// file was last written, so a bump refreshes it without rewriting every startup.
const RULES_VERSION_MARKER: &str = ".sdd_rules_version";

/// `(skill name, SKILL.md body)` for every SDD phase skill, embedded at compile
/// time. The name is both the directory and the frontmatter `name:`.
pub const SKILLS: &[(&str, &str)] = &[
    (
        "sdd-discovery",
        include_str!("../assets/skills/sdd-discovery/SKILL.md"),
    ),
    (
        "sdd-stack",
        include_str!("../assets/skills/sdd-stack/SKILL.md"),
    ),
    (
        "sdd-design-system",
        include_str!("../assets/skills/sdd-design-system/SKILL.md"),
    ),
    (
        "sdd-design",
        include_str!("../assets/skills/sdd-design/SKILL.md"),
    ),
    (
        "sdd-task",
        include_str!("../assets/skills/sdd-task/SKILL.md"),
    ),
    (
        "sdd-implement",
        include_str!("../assets/skills/sdd-implement/SKILL.md"),
    ),
    (
        "sdd-test",
        include_str!("../assets/skills/sdd-test/SKILL.md"),
    ),
    (
        "sdd-review",
        include_str!("../assets/skills/sdd-review/SKILL.md"),
    ),
    (
        "sdd-ship",
        include_str!("../assets/skills/sdd-ship/SKILL.md"),
    ),
];

/// Extracts the embedded SDD skills into `$GROK_HOME/skills/<name>/SKILL.md`.
///
/// Version-gated to mirror the built-in metadata extractor: when the marker
/// already records `version`, this is a no-op (so it costs one file read per
/// startup and never clobbers edits a user made after extraction). On a version
/// change — a fresh binary — the canonical bodies are rewritten so skill fixes
/// ship with the upgrade. Best-effort: individual write failures are logged, not
/// fatal, so a read-only or partially-writable home never blocks startup.
pub fn extract(grok_home: &Path, version: &str) {
    let marker = grok_home.join(VERSION_MARKER);
    if let Ok(existing) = std::fs::read_to_string(&marker)
        && existing.trim() == version
    {
        return;
    }

    let mut wrote_any = false;
    for &(name, body) in SKILLS {
        let dir = grok_home.join("skills").join(name);
        if let Err(e) = std::fs::create_dir_all(&dir) {
            tracing::debug!(error = %e, skill = name, "failed to create SDD skill dir");
            continue;
        }
        match std::fs::write(dir.join("SKILL.md"), body) {
            Ok(()) => wrote_any = true,
            Err(e) => tracing::debug!(error = %e, skill = name, "failed to write SDD skill"),
        }
    }

    // Only stamp the marker once at least one skill landed, so a fully failed
    // extraction retries next startup instead of being marked done.
    if wrote_any && let Err(e) = std::fs::write(&marker, version) {
        tracing::debug!(error = %e, "failed to write SDD skills version marker");
    }
    tracing::debug!(version, "extracted SDD skills");
}

/// Refreshes an already-`init`ed SDD project's standing rules
/// (`<project_root>/.grok/rules/sdd.md`) to the canonical template on a version
/// bump. `init` writes that file once and never re-touches it, so without this an
/// existing project (e.g. one scaffolded months ago) never picks up rules fixes
/// when its `grok` binary upgrades.
///
/// Scoped and version-gated to stay a cheap, safe no-op:
/// - acts only when the project opted into SDD (`<root>/sdd/` exists) **and**
///   already has a rules file — it refreshes, never creates (`init` owns creation);
/// - the marker lives at `<root>/.grok-sdd/.sdd_rules_version`; when it already
///   records `version` this is one file read and returns.
///
/// Same tradeoff as [`extract`]: a version bump rewrites the canonical body, so a
/// user's local edits to `.grok/rules/sdd.md` are replaced on upgrade — rules
/// fixes must ship. Best-effort: failures are logged, never fatal.
pub fn refresh_project_rules(project_root: &Path, version: &str) {
    if !project_root.join(crate::scaffold::DIR_NAME).is_dir() {
        return; // not an SDD project
    }
    let rules = project_root.join(".grok").join("rules").join("sdd.md");
    if !rules.exists() {
        return; // init owns creation; nothing to refresh
    }
    let marker_dir = project_root.join(".grok-sdd");
    let marker = marker_dir.join(RULES_VERSION_MARKER);
    if let Ok(existing) = std::fs::read_to_string(&marker)
        && existing.trim() == version
    {
        return;
    }
    if let Err(e) = std::fs::write(&rules, crate::cli::RULES_TEMPLATE) {
        tracing::debug!(error = %e, "failed to refresh SDD project rules");
        return;
    }
    if let Err(e) = std::fs::create_dir_all(&marker_dir) {
        tracing::debug!(error = %e, "failed to create SDD marker dir");
        return;
    }
    if let Err(e) = std::fs::write(&marker, version) {
        tracing::debug!(error = %e, "failed to write SDD rules version marker");
    }
    tracing::debug!(version, "refreshed SDD project rules");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_skill_has_frontmatter_name_matching_its_key() {
        for &(name, body) in SKILLS {
            assert!(
                body.starts_with("---"),
                "{name} SKILL.md must open with YAML frontmatter"
            );
            assert!(
                body.contains(&format!("name: {name}")),
                "{name} SKILL.md frontmatter must declare `name: {name}`"
            );
        }
    }

    #[test]
    fn covers_the_nine_phase_skills() {
        let mut names: Vec<&str> = SKILLS.iter().map(|&(n, _)| n).collect();
        names.sort_unstable();
        assert_eq!(
            names,
            [
                "sdd-design",
                "sdd-design-system",
                "sdd-discovery",
                "sdd-implement",
                "sdd-review",
                "sdd-ship",
                "sdd-stack",
                "sdd-task",
                "sdd-test",
            ]
        );
    }

    #[test]
    fn extract_writes_all_skills_and_marker() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        extract(home, "1.2.3");
        for &(name, body) in SKILLS {
            let got = std::fs::read_to_string(home.join("skills").join(name).join("SKILL.md"))
                .unwrap_or_else(|_| panic!("{name} not extracted"));
            assert_eq!(got, body);
        }
        assert_eq!(
            std::fs::read_to_string(home.join(VERSION_MARKER))
                .unwrap()
                .trim(),
            "1.2.3"
        );
    }

    #[test]
    fn same_version_does_not_overwrite_user_edits() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        extract(home, "1.0.0");
        let edited = home.join("skills/sdd-implement/SKILL.md");
        std::fs::write(&edited, "my local edit").unwrap();
        // Same version → no-op, edit survives.
        extract(home, "1.0.0");
        assert_eq!(std::fs::read_to_string(&edited).unwrap(), "my local edit");
    }

    #[test]
    fn refresh_project_rules_is_scoped_and_version_gated() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // No sdd/ dir → no-op even if a rules file somehow exists.
        refresh_project_rules(root, "1.0.0");
        assert!(!root.join(".grok-sdd/.sdd_rules_version").exists());

        // An SDD project without a rules file → init owns creation, still no-op.
        std::fs::create_dir_all(root.join("sdd")).unwrap();
        refresh_project_rules(root, "1.0.0");
        assert!(!root.join(".grok/rules/sdd.md").exists());

        // With a stale rules file → refreshed to canonical, marker stamped.
        let rules = root.join(".grok/rules/sdd.md");
        std::fs::create_dir_all(rules.parent().unwrap()).unwrap();
        std::fs::write(&rules, "stale rules").unwrap();
        refresh_project_rules(root, "1.0.0");
        assert_eq!(
            std::fs::read_to_string(&rules).unwrap(),
            crate::cli::RULES_TEMPLATE
        );

        // Same version → no rewrite (a local edit survives).
        std::fs::write(&rules, "user edit").unwrap();
        refresh_project_rules(root, "1.0.0");
        assert_eq!(std::fs::read_to_string(&rules).unwrap(), "user edit");

        // Version bump → canonical restored.
        refresh_project_rules(root, "2.0.0");
        assert_eq!(
            std::fs::read_to_string(&rules).unwrap(),
            crate::cli::RULES_TEMPLATE
        );
    }

    #[test]
    fn version_bump_reextracts_canonical_bodies() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        extract(home, "1.0.0");
        let edited = home.join("skills/sdd-implement/SKILL.md");
        std::fs::write(&edited, "stale").unwrap();
        // New version → canonical body restored.
        extract(home, "2.0.0");
        let canonical = SKILLS
            .iter()
            .find(|&&(n, _)| n == "sdd-implement")
            .unwrap()
            .1;
        assert_eq!(std::fs::read_to_string(&edited).unwrap(), canonical);
    }
}
