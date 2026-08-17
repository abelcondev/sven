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
