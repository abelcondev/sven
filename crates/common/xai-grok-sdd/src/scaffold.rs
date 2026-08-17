//! The OKF SDD knowledge base scaffold and status reader. `Scaffold` writes the
//! durable markdown artifacts under `<root>/sdd`; `read_status` reports decision
//! and task counts. Ported from the Go `internal/sdd/sdd.go`.

use crate::tier::{Tier, resolve_tier};
use crate::util::{list_artifacts, read_frontmatter};
use std::fs;
use std::path::Path;

/// The SDD knowledge-base directory, relative to the workspace root.
pub const DIR_NAME: &str = "sdd";

/// The embedded knowledge-base templates, as `(relative path, content)` pairs,
/// sorted by path. Mirrors the Go `go:embed all:templates`.
const TEMPLATES: &[(&str, &str)] = &[
    ("context.md", include_str!("../templates/context.md")),
    (
        "decisions/_template.md",
        include_str!("../templates/decisions/_template.md"),
    ),
    (
        "designs/_template.md",
        include_str!("../templates/designs/_template.md"),
    ),
    ("index.md", include_str!("../templates/index.md")),
    ("log.md", include_str!("../templates/log.md")),
    ("proposal.md", include_str!("../templates/proposal.md")),
    (
        "tasks/_template.md",
        include_str!("../templates/tasks/_template.md"),
    ),
];

/// Returns the embedded content of a template by its path relative to
/// `templates/` (e.g. `"proposal.md"`), or `None` if there is no such template.
pub(crate) fn template(rel: &str) -> Option<&'static str> {
    TEMPLATES.iter().find(|(p, _)| *p == rel).map(|(_, c)| *c)
}

/// Writes the OKF SDD knowledge base under `<root>/sdd`. Existing files are never
/// overwritten, so re-running is safe and idempotent. Returns the paths (relative
/// to root, `/`-separated) it created and the ones it skipped.
pub fn scaffold(root: &Path) -> anyhow::Result<(Vec<String>, Vec<String>)> {
    let base = root.join(DIR_NAME);
    let mut created = Vec::new();
    let mut skipped = Vec::new();
    for (rel, content) in TEMPLATES {
        let dest = base.join(rel);
        let display = format!("{DIR_NAME}/{rel}");
        if dest.exists() {
            skipped.push(display);
            continue;
        }
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&dest, content)?;
        created.push(display);
    }
    created.sort();
    skipped.sort();
    Ok((created, skipped))
}

/// A single task artifact's summary, read from its frontmatter.
#[derive(Debug, Clone)]
pub struct TaskInfo {
    /// File name without extension, e.g. `"003-auth-signup-ui"`.
    pub name: String,
    pub title: String,
    pub status: String,
    /// Decision ref this task links to (frontmatter `decision:`), or `""`.
    pub decision: String,
    /// Resolved ceremony weight: frontmatter override or inferred.
    pub tier: Tier,
}

/// A snapshot of the SDD knowledge base for reporting.
#[derive(Debug, Clone)]
pub struct Status {
    pub present: bool,
    /// Approved decisions (excludes `_template.md`).
    pub decisions: usize,
    pub tasks: Vec<TaskInfo>,
}

/// Inspects `<root>/sdd` and reports decision and task counts. A missing knowledge
/// base is not an error: it returns `Status { present: false, .. }`.
pub fn read_status(root: &Path) -> anyhow::Result<Status> {
    let base = root.join(DIR_NAME);
    if !base.exists() {
        return Ok(Status {
            present: false,
            decisions: 0,
            tasks: Vec::new(),
        });
    }
    let decisions = list_artifacts(&base.join("decisions"))?.len();
    let mut tasks = Vec::new();
    for path in list_artifacts(&base.join("tasks"))? {
        let fm = read_frontmatter(&path);
        tasks.push(TaskInfo {
            name: stem(&path),
            title: fm.get("title").cloned().unwrap_or_default(),
            status: fm.get("status").cloned().unwrap_or_default(),
            decision: fm.get("decision").cloned().unwrap_or_default(),
            tier: resolve_tier(&fm),
        });
    }
    tasks.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(Status {
        present: true,
        decisions,
        tasks,
    })
}

/// The file name without its `.md` extension.
pub(crate) fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_artifact(root: &Path, rel: &str, typ: &str, title: &str, status: &str) {
        let body = format!("---\ntype: {typ}\ntitle: {title}\nstatus: {status}\n---\n\n# {typ}\n");
        let path = root.join("sdd").join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, body).unwrap();
    }

    #[test]
    fn scaffold_creates_okf_base_and_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        let (created, skipped) = scaffold(root).unwrap();
        assert!(skipped.is_empty(), "first scaffold skipped {skipped:?}");

        let want = [
            "sdd/context.md",
            "sdd/decisions/_template.md",
            "sdd/designs/_template.md",
            "sdd/index.md",
            "sdd/log.md",
            "sdd/proposal.md",
            "sdd/tasks/_template.md",
        ];
        assert_eq!(created, want);
        for w in want {
            assert!(root.join(w).exists(), "expected file {w} on disk");
        }

        let (created2, skipped2) = scaffold(root).unwrap();
        assert!(created2.is_empty(), "second scaffold created {created2:?}");
        assert_eq!(skipped2.len(), want.len());
    }

    #[test]
    fn scaffold_preserves_user_edits() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        scaffold(root).unwrap();
        let log_path = root.join("sdd/log.md");
        let custom = "# Log\n\n- 2026-07-25 — Decision 001 approved.\n";
        fs::write(&log_path, custom).unwrap();
        scaffold(root).unwrap();
        assert_eq!(
            fs::read_to_string(&log_path).unwrap(),
            custom,
            "log.md was overwritten"
        );
    }

    #[test]
    fn read_status_counts() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        let st = read_status(root).unwrap();
        assert!(!st.present, "Present should be false for missing sdd/");

        scaffold(root).unwrap();
        write_artifact(
            root,
            "decisions/001-auth.md",
            "Decision",
            "Auth",
            "approved",
        );
        write_artifact(
            root,
            "tasks/001-auth-signup-ui.md",
            "Task",
            "Signup UI",
            "done",
        );
        write_artifact(
            root,
            "tasks/001-auth-guard.md",
            "Task",
            "Route guard",
            "pending",
        );

        let st = read_status(root).unwrap();
        assert!(st.present);
        assert_eq!(st.decisions, 1, "templates must be excluded");
        assert_eq!(st.tasks.len(), 2, "templates must be excluded");
        assert_eq!(st.tasks[0].name, "001-auth-guard");
        assert_eq!(st.tasks[0].status, "pending");
        assert_eq!(st.tasks[1].title, "Signup UI");
        assert_eq!(st.tasks[1].status, "done");
    }
}
