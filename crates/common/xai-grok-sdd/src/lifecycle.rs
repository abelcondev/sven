//! The SDD lifecycle mutations: promote a proposal to a decision, add tasks and
//! designs, and close them out — each atomically updating the artifact, `log.md`,
//! and `index.md` so callers never hand-edit frontmatter. Ported from
//! `internal/sdd/lifecycle.go`.

use crate::scaffold::{DIR_NAME, stem, template};
use crate::tier::{Tier, parse_tier};
use crate::util::{
    append_line, next_number, read_frontmatter, set_frontmatter_status, slugify, split_frontmatter,
};
use anyhow::{Context, bail};
use chrono::{DateTime, Utc};
use std::fs;
use std::path::Path;

fn rfc3339(now: DateTime<Utc>) -> String {
    now.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn date(now: DateTime<Utc>) -> String {
    now.format("%Y-%m-%d").to_string()
}

/// Turns the in-review `proposal.md` into a numbered, approved decision: writes
/// `decisions/NNN-slug.md`, appends a line to `log.md`, adds the decision to
/// `index.md`, and resets `proposal.md`. `title_override` wins over the proposal's
/// frontmatter title when non-empty. Returns the created decision's path relative
/// to root.
pub fn promote(root: &Path, title_override: &str, now: DateTime<Utc>) -> anyhow::Result<String> {
    let base = root.join(DIR_NAME);
    let proposal_path = base.join("proposal.md");
    let raw = fs::read_to_string(&proposal_path).context("read proposal")?;
    let (fm, body) = split_frontmatter(&raw);

    let mut title = title_override.trim().to_string();
    if title.is_empty() {
        title = fm
            .get("title")
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
    }
    if title.is_empty() || title == "(no active proposal)" {
        bail!("no active proposal to approve; draft sdd/proposal.md first (or pass --title)");
    }
    let description = fm
        .get("description")
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    let decisions_dir = base.join("decisions");
    fs::create_dir_all(&decisions_dir)?;
    let num = next_number(&decisions_dir)?;
    let file_name = format!("{num}-{}.md", slugify(&title));
    let rel_path = format!("{DIR_NAME}/decisions/{file_name}");

    let tags = {
        let t = fm.get("tags").map(|s| s.trim()).unwrap_or("");
        if t.is_empty() { "[]" } else { t }
    };
    let doc = format!(
        "---\ntype: Decision\ntitle: {title}\ndescription: {description}\ntags: {tags}\nstatus: approved\ntimestamp: {ts}\nsupersedes: []\n---\n\n{body}\n",
        ts = rfc3339(now),
        body = relabel_first_heading(&body, "Proposal", "Decision").trim(),
    );
    fs::write(root.join(&rel_path), doc)?;

    let mut log_line = format!("- {} — Decision {num}: {title}.", date(now));
    if !description.is_empty() {
        log_line.push_str(&format!(" {description}"));
    }
    append_line(&base.join("log.md"), &log_line)?;

    add_decision_to_index(
        &base.join("index.md"),
        &num,
        &title,
        &file_name,
        &description,
    )?;

    // Reset proposal.md to the empty template so the next proposal starts clean.
    if let Some(tmpl) = template("proposal.md") {
        let _ = fs::write(&proposal_path, tmpl);
    }
    Ok(rel_path)
}

/// Scaffolds `tasks/NNN-slug.md`, a pending task linked to `decision_ref`. The
/// tier is left to inference. Returns the new task's path relative to root.
pub fn add_task(
    root: &Path,
    decision_ref: &str,
    title: &str,
    now: DateTime<Utc>,
) -> anyhow::Result<String> {
    add_task_with_tier(root, decision_ref, title, None, now)
}

/// [`add_task`] with an explicit tier override written to the task's frontmatter.
/// `None` writes no `tier:` line, leaving it to inference.
pub fn add_task_with_tier(
    root: &Path,
    decision_ref: &str,
    title: &str,
    tier: Option<Tier>,
    now: DateTime<Utc>,
) -> anyhow::Result<String> {
    let title = title.trim();
    if title.is_empty() {
        bail!("task title is required");
    }
    let base = root.join(DIR_NAME);
    let tasks_dir = base.join("tasks");
    fs::create_dir_all(&tasks_dir)?;
    let num = next_number(&tasks_dir)?;
    let file_name = format!("{num}-{}.md", slugify(title));
    let rel_path = format!("{DIR_NAME}/tasks/{file_name}");

    let refr = {
        let r = decision_ref.trim();
        if r.is_empty() {
            "decisions/NNN-name.md"
        } else {
            r
        }
    };
    // A tier override is honored only when it is a valid value (mirrors parseTier).
    let tier_line = tier
        .and_then(|t| parse_tier(t.as_str()))
        .map(|t| format!("tier: {t}\n"))
        .unwrap_or_default();

    let doc = format!(
        "---\ntype: Task\ntitle: {title}\ndescription: \ndecision: {refr}\ntags: []\nstatus: pending\n{tier_line}timestamp: {ts}\n---\n\n\
         # Acceptance criteria\n\n\
         ```gherkin\n\
         Scenario: <describe the behavior>\n  Given <precondition>\n  When <action>\n  Then <expected outcome>\n\
         ```\n\n\
         # Dependencies\n\nList any tasks or decisions this depends on.\n",
        ts = rfc3339(now),
    );
    fs::write(root.join(&rel_path), doc)?;
    Ok(rel_path)
}

/// Marks a task done and appends a line to `log.md`, atomically closing it.
/// `task_ref` accepts `"tasks/NNN-slug.md"`, `"NNN-slug.md"`, or `"NNN-slug"`.
/// Returns the task path relative to root.
pub fn complete_task(root: &Path, task_ref: &str, now: DateTime<Utc>) -> anyhow::Result<String> {
    let base = root.join(DIR_NAME);
    let path = resolve_artifact_path(&base, "tasks", task_ref)?;
    let fm = read_frontmatter(&path);
    set_frontmatter_status(&path, "done")?;
    let name = stem(&path);
    let title = fm.get("title").map(|s| s.trim()).unwrap_or("");
    let log_line = if title.is_empty() {
        format!("- {} — Task {name} done.", date(now))
    } else {
        format!("- {} — Task {name} ({title}) done.", date(now))
    };
    append_line(&base.join("log.md"), &log_line)?;
    Ok(format!("{DIR_NAME}/tasks/{name}.md"))
}

/// Scaffolds `designs/NNN-slug.md`, an in-review design linked to `decision_ref` —
/// the UI gate's artifact. Returns the path relative to root.
pub fn add_design(
    root: &Path,
    decision_ref: &str,
    title: &str,
    now: DateTime<Utc>,
) -> anyhow::Result<String> {
    let title = title.trim();
    if title.is_empty() {
        bail!("a design title is required");
    }
    let base = root.join(DIR_NAME);
    let designs_dir = base.join("designs");
    fs::create_dir_all(&designs_dir)?;
    let num = next_number(&designs_dir)?;
    let file_name = format!("{num}-{}.md", slugify(title));
    let rel_path = format!("{DIR_NAME}/designs/{file_name}");

    let refr = {
        let r = decision_ref.trim();
        if r.is_empty() {
            "decisions/NNN-name.md"
        } else {
            r
        }
    };
    let doc = format!(
        "---\ntype: Design\ntitle: {title}\ndescription: \ndecision: {refr}\ntags: [ui]\nstatus: in-review\ntimestamp: {ts}\n---\n\n{DESIGN_BODY}",
        ts = rfc3339(now),
    );
    fs::write(root.join(&rel_path), doc)?;
    Ok(rel_path)
}

/// Flips a design artifact from in-review to approved and logs it, clearing the UI
/// gate for the tasks of its decision. Returns the design path relative to root.
pub fn approve_design(root: &Path, design_ref: &str, now: DateTime<Utc>) -> anyhow::Result<String> {
    let base = root.join(DIR_NAME);
    let path = resolve_artifact_path(&base, "designs", design_ref)?;
    let fm = read_frontmatter(&path);
    set_frontmatter_status(&path, "approved")?;
    let name = stem(&path);
    let title = fm.get("title").map(|s| s.trim()).unwrap_or("");
    let log_line = if title.is_empty() {
        format!("- {} — Design {name} approved.", date(now))
    } else {
        format!("- {} — Design {name} ({title}) approved.", date(now))
    };
    append_line(&base.join("log.md"), &log_line)?;
    Ok(format!("{DIR_NAME}/designs/{name}.md"))
}

/// Resolves a user-supplied artifact reference to an existing file under
/// `<base>/<subdir>`, accepting `"<subdir>/NNN-slug.md"`, `"NNN-slug.md"`, or
/// `"NNN-slug"`. Errors if the reference is empty or the file is absent.
fn resolve_artifact_path(
    base: &Path,
    subdir: &str,
    refr: &str,
) -> anyhow::Result<std::path::PathBuf> {
    let singular = subdir.strip_suffix('s').unwrap_or(subdir);
    let name = crate::loop_state::normalize_ref(refr);
    if name.is_empty() {
        bail!("a {singular} reference is required");
    }
    let path = base.join(subdir).join(format!("{name}.md"));
    if !path.exists() {
        bail!("{singular} not found: {subdir}/{name}.md");
    }
    Ok(path)
}

/// Rewrites the first `"# from"` heading line to `"# to"`.
fn relabel_first_heading(body: &str, from: &str, to: &str) -> String {
    let mut lines: Vec<String> = body.split('\n').map(String::from).collect();
    for line in &mut lines {
        if line.trim() == format!("# {from}") {
            *line = format!("# {to}");
            break;
        }
    }
    lines.join("\n")
}

/// Inserts a decision bullet at the end of `index.md`'s "## Decisions" section
/// (newest last). If the section is absent it is appended.
fn add_decision_to_index(
    path: &Path,
    num: &str,
    title: &str,
    file_name: &str,
    description: &str,
) -> anyhow::Result<()> {
    let raw = fs::read_to_string(path)?;
    let mut bullet = format!("- [{num} — {title}](decisions/{file_name})");
    if !description.is_empty() {
        bullet.push_str(&format!(" — {description}"));
    }

    let normalized = raw.replace("\r\n", "\n");
    let lines: Vec<String> = normalized.split('\n').map(String::from).collect();
    let start = lines.iter().position(|l| l.trim() == "## Decisions");
    let Some(start) = start else {
        let out = format!(
            "{}\n\n## Decisions\n\n{bullet}\n",
            raw.trim_end_matches('\n')
        );
        fs::write(path, out)?;
        return Ok(());
    };

    // End of the section: the next "## " heading, the English-footer line, or EOF.
    let mut end = lines.len();
    for (i, line) in lines.iter().enumerate().skip(start + 1) {
        let t = line.trim();
        if t.starts_with("## ") || t.starts_with("Everything here is written") {
            end = i;
            break;
        }
    }
    // Insert after the last non-blank line of the section (keeps trailing blanks).
    let mut ins = end;
    while ins - 1 > start && lines[ins - 1].trim().is_empty() {
        ins -= 1;
    }
    let mut out: Vec<String> = Vec::with_capacity(lines.len() + 1);
    out.extend_from_slice(&lines[..ins]);
    out.push(bullet);
    out.extend_from_slice(&lines[ins..]);
    fs::write(path, out.join("\n"))?;
    Ok(())
}

/// The starting checklist for a design artifact — a lightweight spec the human
/// approves before any UI code is written.
const DESIGN_BODY: &str = r#"# Design

The spec for a product screen: gather references, sketch the layout as a
wireframe, and record which base components compose it. The screen is then coded
hi-fi **directly** — no placeholder build. The human approves this doc first.

## References

What the UI should look like — ask the user before sketching and record whatever
they provide (save images under this design's folder `sdd/designs/<NNN>/`):

- Figma: <url> (if given, pull frames/tokens with the figma MCP)
- Image / Pencil / screenshot: <path>
- Site or product to emulate: <url>
- Or a written description of the intended look & feel.

If the user has no reference, note that and describe the intended look here.

## Wireframe

An ASCII/line sketch of each screen and key state — layout, regions, and where
each component sits. One block per state (default, empty, loading, error):

```
┌───────────────────────────┐
│  <sketch the layout>       │
└───────────────────────────┘
```

## Composition

- Base components used (from the `/design-system` workbench): <Button, Card, …>.
- Product components coded directly for this screen: <name each>.
- Missing base primitive to add to the workbench first (if any): <name>.

## Behavior & states

Interactions, empty/loading/error handling, and edge cases the reviewer must
check. Behavior itself is covered by code tests in `sdd-test`; the human
verifies the live screen on localhost.
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scaffold::{read_status, scaffold};

    fn fixed() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-07-25T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn read(path: &Path) -> String {
        fs::read_to_string(path).unwrap()
    }

    #[test]
    fn promote_writes_decision_log_index_and_resets_proposal() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        scaffold(root).unwrap();
        let proposal = "---\ntype: Proposal\ntitle: Magic-link auth\ndescription: Passwordless sign-in via email.\ntags: []\nstatus: in-review\n---\n\n# Proposal\n\nAdd magic-link authentication.\n\n# Context\n\nUsers dislike passwords.\n";
        fs::write(root.join("sdd/proposal.md"), proposal).unwrap();

        let rel = promote(root, "", fixed()).unwrap();
        assert_eq!(rel, "sdd/decisions/001-magic-link-auth.md");

        let decision = read(&root.join(&rel));
        for want in [
            "type: Decision",
            "status: approved",
            "title: Magic-link auth",
            "timestamp: 2026-07-25T10:00:00Z",
            "# Decision",
            "Add magic-link authentication.",
        ] {
            assert!(
                decision.contains(want),
                "decision missing {want:?}\n{decision}"
            );
        }
        assert!(
            !decision.contains("# Proposal"),
            "heading not relabeled:\n{decision}"
        );

        let log = read(&root.join("sdd/log.md"));
        assert!(
            log.contains(
                "- 2026-07-25 — Decision 001: Magic-link auth. Passwordless sign-in via email."
            ),
            "log:\n{log}"
        );

        let index = read(&root.join("sdd/index.md"));
        assert!(index.contains("- [001 — Magic-link auth](decisions/001-magic-link-auth.md) — Passwordless sign-in via email."), "index:\n{index}");
        assert!(
            index.contains("Everything here is written in English"),
            "index footer clobbered:\n{index}"
        );

        let reset = read(&root.join("sdd/proposal.md"));
        assert!(
            reset.contains("status: empty"),
            "proposal not reset:\n{reset}"
        );
    }

    #[test]
    fn promote_refuses_empty_proposal() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        scaffold(root).unwrap();
        assert!(promote(root, "", fixed()).is_err());
        let rel = promote(root, "Explicit decision", fixed()).unwrap();
        assert_eq!(rel, "sdd/decisions/001-explicit-decision.md");
    }

    #[test]
    fn promote_numbers_sequentially() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        scaffold(root).unwrap();
        for title in ["First", "Second", "Third"] {
            promote(root, title, fixed()).unwrap();
        }
        for want in ["001-first.md", "002-second.md", "003-third.md"] {
            assert!(
                root.join("sdd/decisions").join(want).exists(),
                "expected {want}"
            );
        }
    }

    #[test]
    fn add_task_links_decision_and_is_pending() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        scaffold(root).unwrap();
        let rel = add_task(root, "decisions/003-auth.md", "Signup UI", fixed()).unwrap();
        assert_eq!(rel, "sdd/tasks/001-signup-ui.md");
        let task = read(&root.join(&rel));
        for want in [
            "type: Task",
            "status: pending",
            "decision: decisions/003-auth.md",
            "title: Signup UI",
            "```gherkin",
        ] {
            assert!(task.contains(want), "task missing {want:?}\n{task}");
        }
        let st = read_status(root).unwrap();
        assert_eq!(st.tasks.len(), 1);
        assert_eq!(st.tasks[0].status, "pending");
    }

    #[test]
    fn promote_preserves_ui_tag() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        scaffold(root).unwrap();
        let proposal = "---\ntype: Proposal\ntitle: Caja screen\ndescription: The cashier POS screen.\ntags: [ui]\nstatus: in-review\n---\n\n# Proposal\n\nBuild the caja screen.\n";
        fs::write(root.join("sdd/proposal.md"), proposal).unwrap();
        let rel = promote(root, "", fixed()).unwrap();
        assert!(
            read(&root.join(&rel)).contains("tags: [ui]"),
            "decision must carry the ui tag"
        );
    }

    #[test]
    fn complete_task_marks_done_and_logs() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        scaffold(root).unwrap();
        add_task(root, "decisions/001-arch.md", "Route guard", fixed()).unwrap();
        let rel = complete_task(root, "001-route-guard", fixed()).unwrap();
        assert!(read(&root.join(&rel)).contains("status: done"));
        assert!(
            read(&root.join("sdd/log.md")).contains("Task 001-route-guard (Route guard) done.")
        );
        let st = read_status(root).unwrap();
        assert_eq!(st.tasks.len(), 1);
        assert_eq!(st.tasks[0].status, "done");
    }

    #[test]
    fn add_and_approve_design() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        scaffold(root).unwrap();
        let rel = add_design(root, "decisions/002-caja-ui.md", "Caja screen", fixed()).unwrap();
        assert_eq!(rel, "sdd/designs/001-caja-screen.md");
        let design = read(&root.join(&rel));
        for want in [
            "type: Design",
            "status: in-review",
            "decision: decisions/002-caja-ui.md",
            "## References",
            "## Wireframe",
            "## Composition",
        ] {
            assert!(design.contains(want), "design missing {want:?}\n{design}");
        }
        approve_design(root, "001-caja-screen", fixed()).unwrap();
        assert!(read(&root.join(&rel)).contains("status: approved"));
        assert!(
            read(&root.join("sdd/log.md"))
                .contains("Design 001-caja-screen (Caja screen) approved.")
        );
    }

    #[test]
    fn complete_task_missing_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        scaffold(root).unwrap();
        assert!(complete_task(root, "999-nope", fixed()).is_err());
    }
}
