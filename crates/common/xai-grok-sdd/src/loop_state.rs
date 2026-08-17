//! The resumable position of the SDD loop and its single-next-step advisor.
//! `LoopState` is derived purely from files on disk plus the current git branch,
//! so it is cheap to recompute every turn. Ported from `internal/sdd/loop.go`.

use crate::scaffold::{DIR_NAME, TaskInfo, stem};
use crate::tier::resolve_tier;
use crate::util::{list_artifacts, read_frontmatter};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

/// The phase skill for the implement step — the long TDD + review cycle. Exported
/// so callers can detect this phase (e.g. to widen the turn budget).
pub const SKILL_IMPLEMENT: &str = "sdd-implement";

/// Reports whether `branch` is one the feature-branch policy protects.
pub fn is_protected_branch(branch: &str) -> bool {
    matches!(branch.trim(), "main" | "master")
}

/// The resumable position of the SDD loop.
#[derive(Debug, Clone, Default)]
pub struct LoopState {
    pub present: bool,
    pub proposal_active: bool,
    pub proposal_title: String,
    pub decisions: usize,
    /// e.g. `"decisions/002-architecture.md"`, or `""` if none.
    pub latest_decision: String,
    /// Some decision carries an architecture/stack tag.
    pub stack_decided: bool,
    pub pending_tasks: Vec<TaskInfo>,
    pub branch: String,

    /// A design artifact awaiting the gate (`"designs/NNN-slug"`), or `""`.
    pub design_in_review: String,
    /// Decision ref `pending_tasks[0]` links to (verbatim), or `""`.
    pub first_task_decision: String,
    /// That decision is tagged UI, so a design is required first.
    pub first_task_needs_ui: bool,
    /// An approved design already exists for that decision.
    pub first_task_has_design: bool,

    /// Ref of the decision tagged design-system, or `""`.
    pub design_system_decision: String,
    /// That decision has an approved workbench design.
    pub design_system_ready: bool,
}

/// The single recommended step given a `LoopState`. `gate` marks a step that
/// hands control to the human rather than doing work. `skill` names the built-in
/// phase skill to load first (`""` for mechanical steps and human gates).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NextAction {
    pub summary: String,
    pub command: String,
    pub gate: bool,
    pub skill: String,
    /// A short, human-readable horizon: the arc that typically follows this step.
    pub then: String,
}

/// Inspects `<root>/sdd` plus the given git `branch` (pass `""` if unknown or
/// outside a repo) and reports the loop position.
pub fn read_loop_state(root: &Path, branch: &str) -> anyhow::Result<LoopState> {
    let mut st = LoopState {
        branch: branch.trim().to_string(),
        ..Default::default()
    };
    let base = root.join(DIR_NAME);
    match fs::metadata(&base) {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(st),
        Err(e) => return Err(e.into()),
    }
    st.present = true;

    if let Ok(raw) = fs::read_to_string(base.join("proposal.md")) {
        let (fm, _) = crate::util::split_frontmatter(&raw);
        let status = fm.get("status").map(|s| s.trim()).unwrap_or("");
        if !status.is_empty() && status != "empty" {
            st.proposal_active = true;
            st.proposal_title = fm
                .get("title")
                .map(|s| s.trim().to_string())
                .unwrap_or_default();
        }
    }

    let decisions = list_artifacts(&base.join("decisions"))?;
    st.decisions = decisions.len();
    if let Some(last) = decisions.last() {
        st.latest_decision = format!("decisions/{}", file_name(last));
    }
    for path in &decisions {
        let fm = read_frontmatter(path);
        let tags = fm.get("tags").map(String::as_str).unwrap_or("");
        if has_tag(tags, "architecture") || has_tag(tags, "stack") {
            st.stack_decided = true;
            break;
        }
    }

    for path in list_artifacts(&base.join("tasks"))? {
        let fm = read_frontmatter(&path);
        let status = fm
            .get("status")
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        if status == "done" || status == "completed" {
            continue;
        }
        st.pending_tasks.push(TaskInfo {
            name: stem(&path),
            title: fm.get("title").cloned().unwrap_or_default(),
            status,
            decision: fm
                .get("decision")
                .map(|s| s.trim().to_string())
                .unwrap_or_default(),
            tier: resolve_tier(&fm),
        });
    }

    let (design_in_review, approved_by_decision) = scan_designs(&base)?;
    st.design_in_review = design_in_review;
    if let Some(first) = st.pending_tasks.first() {
        st.first_task_decision = first.decision.clone();
        if !st.first_task_decision.is_empty() {
            st.first_task_needs_ui = decision_is_ui(&base, &st.first_task_decision);
            st.first_task_has_design =
                approved_by_decision.contains(&normalize_ref(&st.first_task_decision));
        }
    }

    for path in &decisions {
        let fm = read_frontmatter(path);
        if has_tag(
            fm.get("tags").map(String::as_str).unwrap_or(""),
            "design-system",
        ) {
            let refr = format!("decisions/{}", file_name(path));
            st.design_system_ready = approved_by_decision.contains(&normalize_ref(&refr));
            st.design_system_decision = refr;
            break;
        }
    }
    Ok(st)
}

impl LoopState {
    /// Returns the single recommended action for the current loop position. The
    /// decision tree is priority-ordered.
    pub fn next(&self) -> NextAction {
        if !self.present {
            return NextAction {
                summary: "No SDD knowledge base yet. Seed it, then start the loop.".into(),
                command: "grok-sdd init".into(),
                ..Default::default()
            };
        }
        if self.proposal_active {
            let title = if self.proposal_title.is_empty() {
                "the draft"
            } else {
                &self.proposal_title
            };
            return NextAction {
                summary: format!(
                    "A proposal is in review: {title}. Review sdd/proposal.md, then approve it."
                ),
                command: format!("grok-sdd approve --title \"{title}\""),
                gate: true,
                then: "approve → stack (if first) or design/tasks → implement".into(),
                ..Default::default()
            };
        }
        if !self.design_in_review.is_empty() {
            return NextAction {
                summary: format!(
                    "A design is in review: {}. Review it before any UI code — the base components live in the /design-system workbench, or the wireframe + references for a product screen — then approve it.",
                    self.design_in_review
                ),
                command: format!("grok-sdd approve-design {}", self.design_in_review),
                gate: true,
                ..Default::default()
            };
        }
        if self.decisions == 0 {
            return NextAction {
                summary: "Nothing recorded yet. Draft the first proposal (start with discovery — the what & why).".into(),
                command: "grok-sdd propose \"Discovery: <who uses it, the one flow that must not fail, constraints>\"".into(),
                skill: "sdd-discovery".into(),
                ..Default::default()
            };
        }
        if let Some(task) = self.pending_tasks.first() {
            if self.first_task_needs_ui && !self.first_task_has_design {
                return NextAction {
                    summary: format!(
                        "Task {} is UI work with no approved design. Ask the user for visual references, sketch the layout as an ASCII wireframe in the design doc, get it approved, then code the screen hi-fi directly.",
                        task.name
                    ),
                    command: format!(
                        "grok-sdd design {} \"<screen or flow>\"",
                        self.first_task_decision
                    ),
                    skill: "sdd-design".into(),
                    then: "references → wireframe → approve (gate) → code hi-fi → review → ship"
                        .into(),
                    ..Default::default()
                };
            }
            if is_protected_branch(&self.branch) {
                return NextAction {
                    summary: format!(
                        "Pending task {} but HEAD is on {}. Branch once per proposal before writing code.",
                        task.name, self.branch
                    ),
                    command: format!(
                        "git checkout -b {}",
                        proposal_branch(&self.first_task_decision, &task.name)
                    ),
                    ..Default::default()
                };
            }
            let mut label = task.name.clone();
            if !task.title.is_empty() {
                label = format!("{label} — {}", task.title);
            }
            let tier = task.tier;
            return NextAction {
                summary: format!(
                    "Implement pending task {label} [tier: {tier}] (TDD: red → green), review at that tier, then ship it with `grok-sdd ship {}`. One PR per proposal.",
                    task.name
                ),
                skill: SKILL_IMPLEMENT.into(),
                then: format!(
                    "TDD → review (tier {tier}) → grok-sdd ship → next task, or mark PR ready when the proposal is done"
                ),
                ..Default::default()
            };
        }
        if !self.stack_decided {
            return NextAction {
                summary: "Decision recorded, but the stack isn't chosen yet. Ask the user which technologies they want (framework, UI, tests), research the open pieces, then record it as an architecture decision.".into(),
                command: "grok-sdd propose \"Architecture: <stack chosen with the user>\"".into(),
                skill: "sdd-stack".into(),
                ..Default::default()
            };
        }
        if !self.design_system_ready {
            if self.design_system_decision.is_empty() {
                return NextAction {
                    summary: "Stack chosen, but there's no design system yet. Propose one: the base components (tokens, button, input, layout, states) built in an isolated /design-system workbench route, reviewed live before any feature UI.".into(),
                    command: "grok-sdd propose \"Design system: base components in an isolated /design-system workbench route\"".into(),
                    skill: "sdd-design-system".into(),
                    ..Default::default()
                };
            }
            if is_protected_branch(&self.branch) {
                return NextAction {
                    summary: format!(
                        "Design-system decision approved but HEAD is on {}. Branch before building the workbench.",
                        self.branch
                    ),
                    command: format!(
                        "git checkout -b {}",
                        proposal_branch(&self.design_system_decision, "")
                    ),
                    ..Default::default()
                };
            }
            return NextAction {
                summary: "Build the /design-system workbench for the base components — each in every state (empty, loading, error, hover, variants) — then record it with `grok-sdd design` and stop at the gate so the human reviews them live.".into(),
                command: format!("grok-sdd design {} \"Design system workbench\"", self.design_system_decision),
                skill: "sdd-design-system".into(),
                ..Default::default()
            };
        }
        let refr = if self.latest_decision.is_empty() {
            "decisions/NNN-name.md".to_string()
        } else {
            self.latest_decision.clone()
        };
        NextAction {
            summary: "No open work. Add a task to the latest decision, or propose the next thing."
                .into(),
            command: format!("grok-sdd task {refr} \"<task title>\""),
            skill: "sdd-task".into(),
            then:
                "new task → implement, or `grok-sdd propose` a new feature; merge any open PR first"
                    .into(),
            ..Default::default()
        }
    }
}

/// Reports the first design artifact awaiting the gate (status `in-review`) and
/// the set of decisions (normalized refs) that already have an approved design.
fn scan_designs(base: &Path) -> anyhow::Result<(String, HashSet<String>)> {
    let mut approved = HashSet::new();
    let mut in_review = String::new();
    for p in list_artifacts(&base.join("designs"))? {
        let fm = read_frontmatter(&p);
        let name = stem(&p);
        match fm.get("status").map(|s| s.trim()).unwrap_or("") {
            "approved" | "done" | "completed" => {
                let dec = normalize_ref(fm.get("decision").map(String::as_str).unwrap_or(""));
                if !dec.is_empty() {
                    approved.insert(dec);
                }
            }
            "in-review" => {
                if in_review.is_empty() {
                    in_review = format!("designs/{name}");
                }
            }
            _ => {}
        }
    }
    Ok((in_review, approved))
}

/// Reports whether the decision named by `decision_ref` is UI-bearing — either
/// `ui: true` in its frontmatter or a `ui` tag.
fn decision_is_ui(base: &Path, decision_ref: &str) -> bool {
    let name = normalize_ref(decision_ref);
    if name.is_empty() {
        return false;
    }
    let fm = read_frontmatter(&base.join("decisions").join(format!("{name}.md")));
    if is_truthy(fm.get("ui").map(String::as_str).unwrap_or("")) {
        return true;
    }
    has_tag(fm.get("tags").map(String::as_str).unwrap_or(""), "ui")
}

/// Reduces an artifact reference to its bare stem: `"decisions/002-x.md"`,
/// `"002-x.md"`, and `"002-x"` all normalize to `"002-x"`.
pub fn normalize_ref(refr: &str) -> String {
    let refr = refr.trim().replace('\\', "/");
    let refr = refr.trim();
    if refr.is_empty() {
        return String::new();
    }
    let tail = refr.rsplit_once('/').map(|(_, t)| t).unwrap_or(refr);
    tail.strip_suffix(".md").unwrap_or(tail).to_string()
}

/// Reports whether a frontmatter tags value like `"[ui, caja]"` contains `want`.
fn has_tag(tags: &str, want: &str) -> bool {
    tags.trim()
        .trim_matches(|c| c == '[' || c == ']')
        .split(',')
        .any(|t| t.trim().eq_ignore_ascii_case(want))
}

/// Reports whether a scalar frontmatter value reads as boolean true.
fn is_truthy(s: &str) -> bool {
    matches!(
        s.trim().to_lowercase().as_str(),
        "true" | "yes" | "1" | "on"
    )
}

/// Names the single feature branch a proposal's work lands on. When the task
/// links to a decision, the branch carries the decision's stem so every task of
/// that decision shares it (`feat/002-owner-auth`).
fn proposal_branch(decision_ref: &str, task_name: &str) -> String {
    let slug = normalize_ref(decision_ref);
    if !slug.is_empty() {
        return format!("feat/{slug}");
    }
    format!("feat/{}", feature_slug(task_name))
}

/// Turns a task file name (e.g. `"002-owner-auth"`) into a branch slug, dropping
/// the leading `NNN-` sequence prefix so branches read `feat/owner-auth`.
fn feature_slug(task_name: &str) -> String {
    let mut name = task_name;
    if let Some(idx) = task_name.find('-')
        && idx > 0
        && task_name[..idx].chars().all(|c| c.is_ascii_digit())
    {
        name = &task_name[idx + 1..];
    }
    if name.is_empty() {
        return crate::util::slugify(task_name);
    }
    crate::util::slugify(name)
}

/// The file name (with extension) of a path, as a `String`.
fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scaffold::scaffold;

    fn write_raw(root: &Path, rel: &str, content: &str) {
        let path = root.join("sdd").join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    fn write_artifact(root: &Path, rel: &str, typ: &str, title: &str, status: &str) {
        let body = format!("---\ntype: {typ}\ntitle: {title}\nstatus: {status}\n---\n\n# {typ}\n");
        write_raw(root, rel, &body);
    }

    fn must_state(root: &Path, branch: &str) -> LoopState {
        read_loop_state(root, branch).unwrap()
    }

    fn set_active_proposal(root: &Path, title: &str) {
        let doc = format!(
            "---\ntype: Proposal\ntitle: {title}\nstatus: in-review\n---\n\n# Proposal\n\nx\n"
        );
        fs::write(root.join("sdd/proposal.md"), doc).unwrap();
    }

    #[test]
    fn next_missing_knowledge_base() {
        let tmp = tempfile::tempdir().unwrap();
        let st = read_loop_state(tmp.path(), "main").unwrap();
        assert!(!st.present);
        assert_eq!(st.next().command, "grok-sdd init");
    }

    #[test]
    fn next_active_proposal_is_approve_gate() {
        let tmp = tempfile::tempdir().unwrap();
        scaffold(tmp.path()).unwrap();
        set_active_proposal(tmp.path(), "Architecture");
        let action = must_state(tmp.path(), "feat/x").next();
        assert!(action.gate, "active proposal must be a human gate");
        assert!(
            action.command.contains(r#"--title "Architecture""#),
            "approve command = {:?}",
            action.command
        );
    }

    #[test]
    fn next_no_decisions_proposes_discovery() {
        let tmp = tempfile::tempdir().unwrap();
        scaffold(tmp.path()).unwrap();
        let action = must_state(tmp.path(), "main").next();
        assert!(
            action.command.starts_with("grok-sdd propose"),
            "got {:?}",
            action.command
        );
        assert_eq!(action.skill, "sdd-discovery");
    }

    #[test]
    fn next_pending_task_on_protected_branch_wants_feature_branch() {
        let tmp = tempfile::tempdir().unwrap();
        scaffold(tmp.path()).unwrap();
        write_artifact(
            tmp.path(),
            "decisions/001-architecture.md",
            "Decision",
            "Architecture",
            "approved",
        );
        write_artifact(
            tmp.path(),
            "tasks/002-owner-auth.md",
            "Task",
            "Owner auth",
            "pending",
        );
        let action = must_state(tmp.path(), "main").next();
        assert_eq!(action.command, "git checkout -b feat/owner-auth");
    }

    #[test]
    fn next_pending_task_on_feature_branch_implements() {
        let tmp = tempfile::tempdir().unwrap();
        scaffold(tmp.path()).unwrap();
        write_artifact(
            tmp.path(),
            "decisions/001-architecture.md",
            "Decision",
            "Architecture",
            "approved",
        );
        write_artifact(
            tmp.path(),
            "tasks/002-owner-auth.md",
            "Task",
            "Owner auth",
            "pending",
        );
        let action = must_state(tmp.path(), "feat/owner-auth").next();
        assert_eq!(action.command, "");
        assert!(action.summary.contains("002-owner-auth"));
        assert_eq!(action.skill, "sdd-implement");
    }

    #[test]
    fn next_mechanical_steps_name_no_skill() {
        let tmp = tempfile::tempdir().unwrap();
        scaffold(tmp.path()).unwrap();
        write_artifact(
            tmp.path(),
            "decisions/001-architecture.md",
            "Decision",
            "Architecture",
            "approved",
        );
        write_artifact(
            tmp.path(),
            "tasks/002-owner-auth.md",
            "Task",
            "Owner auth",
            "pending",
        );
        assert_eq!(must_state(tmp.path(), "main").next().skill, "");
    }

    #[test]
    fn next_all_tasks_done_proposes_task_or_next() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        scaffold(root).unwrap();
        write_raw(
            root,
            "decisions/001-architecture.md",
            "---\ntype: Decision\ntitle: Architecture\ntags: [architecture]\nstatus: approved\n---\n\n# Decision\n",
        );
        write_raw(
            root,
            "decisions/002-design-system.md",
            "---\ntype: Decision\ntitle: Design system\ntags: [design-system]\nstatus: approved\n---\n",
        );
        write_raw(
            root,
            "designs/001-ds.md",
            "---\ntype: Design\ntitle: DS\ndecision: decisions/002-design-system.md\nstatus: approved\n---\n",
        );
        write_artifact(
            root,
            "decisions/003-catalog.md",
            "Decision",
            "Catalog",
            "approved",
        );
        write_artifact(
            root,
            "tasks/001-foundations.md",
            "Task",
            "Foundations",
            "done",
        );
        let action = must_state(root, "feat/x").next();
        assert!(
            action.command.contains("decisions/003-catalog.md"),
            "got {:?}",
            action.command
        );
    }

    #[test]
    fn next_routes_to_stack_after_first_decision() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        scaffold(root).unwrap();
        write_artifact(
            root,
            "decisions/001-product.md",
            "Decision",
            "Product",
            "approved",
        );
        assert_eq!(must_state(root, "feat/x").next().skill, "sdd-stack");
        write_raw(
            root,
            "decisions/002-architecture.md",
            "---\ntype: Decision\ntitle: Architecture\ntags: [architecture]\nstatus: approved\n---\n\n# Decision\n",
        );
        assert_ne!(must_state(root, "feat/x").next().skill, "sdd-stack");
    }

    #[test]
    fn next_after_stack_proposes_design_system() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        scaffold(root).unwrap();
        write_raw(
            root,
            "decisions/001-architecture.md",
            "---\ntype: Decision\ntitle: Architecture\ntags: [architecture]\nstatus: approved\n---\n",
        );
        let action = must_state(root, "feat/x").next();
        assert_eq!(action.skill, "sdd-design-system");
        assert!(
            action
                .command
                .starts_with(r#"grok-sdd propose "Design system"#),
            "got {:?}",
            action.command
        );
    }

    #[test]
    fn next_design_system_decision_wants_workbench() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        scaffold(root).unwrap();
        write_raw(
            root,
            "decisions/001-architecture.md",
            "---\ntype: Decision\ntitle: Architecture\ntags: [architecture]\nstatus: approved\n---\n",
        );
        write_raw(
            root,
            "decisions/002-design-system.md",
            "---\ntype: Decision\ntitle: Design system\ntags: [design-system]\nstatus: approved\n---\n",
        );
        let action = must_state(root, "feat/002-design-system").next();
        assert_eq!(action.skill, "sdd-design-system");
        assert!(
            action
                .command
                .starts_with("grok-sdd design decisions/002-design-system.md"),
            "got {:?}",
            action.command
        );
    }

    #[test]
    fn next_design_system_ready_proceeds_to_features() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        scaffold(root).unwrap();
        write_raw(
            root,
            "decisions/001-architecture.md",
            "---\ntype: Decision\ntitle: Architecture\ntags: [architecture]\nstatus: approved\n---\n",
        );
        write_raw(
            root,
            "decisions/002-design-system.md",
            "---\ntype: Decision\ntitle: Design system\ntags: [design-system]\nstatus: approved\n---\n",
        );
        write_raw(
            root,
            "designs/001-ds.md",
            "---\ntype: Design\ntitle: DS\ndecision: decisions/002-design-system.md\nstatus: approved\n---\n",
        );
        assert_ne!(must_state(root, "feat/x").next().skill, "sdd-design-system");
    }

    #[test]
    fn feature_slug_drops_sequence_prefix() {
        assert_eq!(feature_slug("002-owner-auth"), "owner-auth");
        assert_eq!(feature_slug("010-order-flow"), "order-flow");
        assert_eq!(feature_slug("no-prefix"), "no-prefix");
    }

    #[test]
    fn next_branch_is_per_proposal() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        scaffold(root).unwrap();
        write_raw(
            root,
            "decisions/002-catalog.md",
            "---\ntype: Decision\ntitle: Catalog\ntags: []\nstatus: approved\n---\n",
        );
        write_raw(
            root,
            "tasks/005-add-item.md",
            "---\ntype: Task\ntitle: Add item\ndecision: decisions/002-catalog.md\nstatus: pending\n---\n",
        );
        assert_eq!(
            must_state(root, "main").next().command,
            "git checkout -b feat/002-catalog"
        );
    }

    #[test]
    fn next_ui_decision_without_design_wants_design_gate() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        scaffold(root).unwrap();
        write_raw(
            root,
            "decisions/002-caja-ui.md",
            "---\ntype: Decision\ntitle: Caja UI\ntags: [ui]\nstatus: approved\n---\n",
        );
        write_raw(
            root,
            "tasks/003-caja-screen.md",
            "---\ntype: Task\ntitle: Caja screen\ndecision: decisions/002-caja-ui.md\nstatus: pending\n---\n",
        );
        let action = must_state(root, "feat/002-caja-ui").next();
        assert!(
            action
                .command
                .starts_with("grok-sdd design decisions/002-caja-ui.md"),
            "got {:?}",
            action.command
        );
    }

    #[test]
    fn next_design_in_review_is_gate() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        scaffold(root).unwrap();
        write_raw(
            root,
            "decisions/002-caja-ui.md",
            "---\ntype: Decision\ntitle: Caja UI\ntags: [ui]\nstatus: approved\n---\n",
        );
        write_raw(
            root,
            "tasks/003-caja-screen.md",
            "---\ntype: Task\ntitle: Caja screen\ndecision: decisions/002-caja-ui.md\nstatus: pending\n---\n",
        );
        write_raw(
            root,
            "designs/001-caja.md",
            "---\ntype: Design\ntitle: Caja\ndecision: decisions/002-caja-ui.md\nstatus: in-review\n---\n",
        );
        let action = must_state(root, "feat/002-caja-ui").next();
        assert!(
            action.gate
                && action
                    .command
                    .starts_with("grok-sdd approve-design designs/001-caja"),
            "got {action:?}"
        );
    }

    #[test]
    fn next_approved_design_unblocks_ui_task() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        scaffold(root).unwrap();
        write_raw(
            root,
            "decisions/002-caja-ui.md",
            "---\ntype: Decision\ntitle: Caja UI\ntags: [ui]\nstatus: approved\n---\n",
        );
        write_raw(
            root,
            "tasks/003-caja-screen.md",
            "---\ntype: Task\ntitle: Caja screen\ndecision: decisions/002-caja-ui.md\nstatus: pending\n---\n",
        );
        write_raw(
            root,
            "designs/001-caja.md",
            "---\ntype: Design\ntitle: Caja\ndecision: decisions/002-caja-ui.md\nstatus: approved\n---\n",
        );
        assert_eq!(
            must_state(root, "main").next().command,
            "git checkout -b feat/002-caja-ui"
        );
        assert_eq!(must_state(root, "feat/002-caja-ui").next().command, "");
    }
}
