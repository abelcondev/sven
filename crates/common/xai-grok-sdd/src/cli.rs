//! The action dispatcher: a thin, model-facing façade over the engine, ported
//! from the Go `grok-sdd` CLI (`main.go`). It maps a native `sdd` tool action to
//! an engine call and returns the model-facing text. Unlike the Go binary this is
//! a library function returning a `String` (no process, no stdout/stderr), so the
//! native tool adapter in `xai-grok-tools` stays a thin bridge.
//!
//! Git/gh-orchestration actions (`ship`, `preflight`, `cleanup`) are deferred to
//! Phase 2b of `docs/NATIVE_SDD_INTEGRATION.md`; this dispatcher recognizes them
//! and returns the manual steps to run instead.

use crate::lifecycle::{add_design, add_task_with_tier, approve_design, complete_task, promote};
use crate::loop_state::{NextAction, read_loop_state};
use crate::propose::seed_proposal;
use crate::scaffold::{DIR_NAME, read_status, scaffold};
use crate::tier::parse_tier;
use anyhow::bail;
use chrono::{DateTime, Utc};
use std::fs;
use std::path::Path;

/// The standing SDD policy `init` drops at `.grok/rules/sdd.md`. Phase 3 slims
/// this and moves the per-turn nudge into native, conditional injection.
pub const RULES_TEMPLATE: &str = include_str!("../assets/rules_sdd.md");

/// Dispatches a native `sdd` tool action. `args` are the positional arguments
/// (e.g. a decision ref then title words); `title`/`tier`/`residual` mirror the
/// old CLI flags. Returns the model-facing result text, or an error whose message
/// is model-facing.
pub fn dispatch(
    root: &Path,
    action: &str,
    args: &[String],
    title: Option<&str>,
    tier: Option<&str>,
    residual: &[String],
    now: DateTime<Utc>,
) -> anyhow::Result<String> {
    match action {
        "init" => cmd_init(root),
        "status" => cmd_status(root),
        "next" => Ok(render_next(&loop_state(root)?.next())),
        "propose" => cmd_propose(root, &args.join(" "), now),
        "approve" => cmd_approve(root, title.unwrap_or(""), now),
        "task" => cmd_task(root, args, tier, now),
        "design" => cmd_design(root, args, now),
        "approve-design" => cmd_approve_design(root, args, now),
        "done" => cmd_done(root, args, residual, now),
        "ship" | "preflight" | "cleanup" => Ok(deferred_git_action(action, args)),
        other => bail!(
            "unknown sdd action {other:?}. Valid: init, next, status, propose, approve, task, design, approve-design, done (ship/preflight/cleanup are run via git/gh for now)."
        ),
    }
}

fn loop_state(root: &Path) -> anyhow::Result<crate::loop_state::LoopState> {
    read_loop_state(root, &current_branch(root))
}

fn cmd_init(root: &Path) -> anyhow::Result<String> {
    let (created, skipped) = scaffold(root)?;
    let mut out = String::new();
    if created.is_empty() {
        out.push_str(&format!(
            "SDD knowledge base already present at {DIR_NAME}/ (nothing to create).\n"
        ));
    } else {
        out.push_str(&format!(
            "Scaffolded the OKF SDD knowledge base at {DIR_NAME}/:\n"
        ));
        for c in &created {
            out.push_str(&format!("  + {c}\n"));
        }
    }
    for s in &skipped {
        out.push_str(&format!("  = {s} (kept)\n"));
    }
    if write_require_branch_marker(root)? {
        out.push_str(
            "  + .grok-sdd/require-branch (branch guard on: feature branch + PR required)\n",
        );
    }
    if write_rules_file(root)? {
        out.push_str("  + .grok/rules/sdd.md (SDD policy injected every turn)\n");
    }
    if !created.is_empty() {
        out.push_str("\nNext: draft a proposal with the `sdd` tool (action \"propose\"), then approve it into a decision.\n");
    }
    Ok(out)
}

fn cmd_status(root: &Path) -> anyhow::Result<String> {
    let st = read_status(root)?;
    if !st.present {
        return Ok(format!(
            "No SDD knowledge base found. Use the `sdd` tool (action \"init\") to scaffold {DIR_NAME}/.\n"
        ));
    }
    let (mut done, mut pending, mut other) = (0, 0, 0);
    for t in &st.tasks {
        match t.status.as_str() {
            "done" | "completed" => done += 1,
            "pending" | "todo" | "" => pending += 1,
            _ => other += 1,
        }
    }
    let mut out = format!("SDD (OKF) — {DIR_NAME}/\n");
    out.push_str(&format!("  decisions: {}\n", st.decisions));
    out.push_str(&format!(
        "  tasks:     {} ({done} done, {pending} pending, {other} other)\n",
        st.tasks.len()
    ));
    for t in &st.tasks {
        let title = if t.title.is_empty() {
            String::new()
        } else {
            format!(" — {}", t.title)
        };
        let tier = if !is_done_status(&t.status) {
            format!("  {{tier: {}}}", t.tier)
        } else {
            String::new()
        };
        let status = if t.status.is_empty() { "-" } else { &t.status };
        out.push_str(&format!("    [{status}] {}{title}{tier}\n", t.name));
    }
    let state = loop_state(root)?;
    if !state.branch.is_empty() {
        out.push_str(&format!("  branch:    {}\n", state.branch));
    }
    out.push('\n');
    out.push_str(&render_next(&state.next()));
    Ok(out)
}

fn cmd_propose(root: &Path, description: &str, now: DateTime<Utc>) -> anyhow::Result<String> {
    if description.trim().is_empty() {
        bail!("a proposal description is required (pass it in `args`)");
    }
    // Branch onto the proposal's feature branch first (when in a git repo on a
    // protected or another proposal branch), so the doc, approval, and code land
    // in one PR — mirrors the Go binary's `ensureProposalBranch`.
    let mut out = ensure_proposal_branch(root, description);
    let rel = seed_proposal(root, description, now)?;
    out.push_str(&format!(
        "Seeded a proposal skeleton at {rel}.\nExpand it in place with your edit tools (do not overwrite) — fill the # Proposal, # Context and # Acceptance sections, set `tags: [ui]` if it involves screens. Describe WHAT and WHY, not HOW, and write no code. Then use the `sdd` tool (action \"approve\", title \"…\").\n"
    ));
    Ok(out)
}

/// Branches onto the proposal's feature branch (`sdd/prop-<slug>`) when HEAD is on
/// a protected branch or another proposal branch, so a proposal's doc, approval,
/// and code share one branch/PR. Returns a model-facing note (possibly empty).
/// No-op outside a git repo or when already on the target branch.
fn ensure_proposal_branch(root: &Path, description: &str) -> String {
    let branch = current_branch(root);
    if branch.is_empty() {
        return String::new();
    }
    let target = crate::propose::proposal_branch_name(description);
    if branch == target {
        return String::new();
    }
    if !crate::loop_state::is_protected_branch(&branch) && !branch.starts_with("sdd/prop-") {
        return String::new();
    }
    match checkout_branch(root, &target) {
        Ok(()) => format!(
            "Branched to {target} — this proposal's doc, approval, and code land in one PR.\n"
        ),
        Err(e) => format!(
            "Note: could not open branch {target} ({e}); writing the proposal on {branch}.\n"
        ),
    }
}

/// Checks out `branch`, creating it if it doesn't exist yet.
fn checkout_branch(root: &Path, branch: &str) -> anyhow::Result<()> {
    let exists = git_ok(
        root,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ],
    );
    if exists {
        git_run(root, &["checkout", branch])
    } else {
        git_run(root, &["checkout", "-b", branch])
    }
}

/// Runs `git -C <root> <args>` for its exit status only, silencing output.
fn git_ok(root: &Path, args: &[&str]) -> bool {
    std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Runs `git -C <root> <args>`, erroring with the first stderr line on failure.
fn git_run(root: &Path, args: &[&str]) -> anyhow::Result<()> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()?;
    if out.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    let msg = stderr
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("git command failed");
    bail!("{msg}")
}

fn cmd_approve(root: &Path, title: &str, now: DateTime<Utc>) -> anyhow::Result<String> {
    let rel = promote(root, title, now)?;
    let base = rel.rsplit('/').next().unwrap_or(&rel);
    Ok(format!(
        "Approved. Wrote decision {rel}, appended {DIR_NAME}/log.md, updated {DIR_NAME}/index.md, reset {DIR_NAME}/proposal.md.\n\
         Commit the decision and open its PR as a draft (no `-u`: it writes .git/config, which the sandbox refuses):\n\
           git add {DIR_NAME}/ && git commit -m \"docs(sdd): {base}\"\n\
           git push origin HEAD && gh pr create --fill --draft\n\
         Then use the `sdd` tool (action \"next\"). All of this decision's tasks stay on the same branch and draft PR — one PR per proposal, marked ready only when every task is done.\n"
    ))
}

fn cmd_task(
    root: &Path,
    args: &[String],
    tier: Option<&str>,
    now: DateTime<Utc>,
) -> anyhow::Result<String> {
    if args.len() < 2 {
        bail!(
            "usage: action \"task\", args: [<decision-ref>, <title...>], optional tier: trivial|standard|critical"
        );
    }
    let decision_ref = &args[0];
    let title = args[1..].join(" ");
    let tier_val = tier.and_then(parse_tier);
    let rel = add_task_with_tier(root, decision_ref, &title, tier_val, now)?;
    let tier_note = match tier_val {
        Some(t) => format!("tier {t}"),
        None => "tier inferred".to_string(),
    };
    Ok(format!(
        "Created task {rel} (pending, {tier_note}, linked to {decision_ref}).\n"
    ))
}

fn cmd_design(root: &Path, args: &[String], now: DateTime<Utc>) -> anyhow::Result<String> {
    if args.len() < 2 {
        bail!("usage: action \"design\", args: [<decision-ref>, <title...>]");
    }
    let decision_ref = &args[0];
    let title = args[1..].join(" ");
    let rel = add_design(root, decision_ref, &title, now)?;
    let stem = rel
        .rsplit('/')
        .next()
        .and_then(|s| s.strip_suffix(".md"))
        .unwrap_or(&rel);
    Ok(format!(
        "Created design {rel} (in-review, linked to {decision_ref}). Gather references, write the ASCII wireframe + composition into the file, then — once the human approves it — use the `sdd` tool (action \"approve-design\", args: [\"{stem}\"]).\n"
    ))
}

fn cmd_approve_design(root: &Path, args: &[String], now: DateTime<Utc>) -> anyhow::Result<String> {
    if args.is_empty() {
        bail!("usage: action \"approve-design\", args: [<design-ref>]");
    }
    let rel = approve_design(root, &args[0], now)?;
    Ok(format!(
        "Approved design {rel}, appended {DIR_NAME}/log.md. UI tasks for its decision are unblocked.\n"
    ))
}

fn cmd_done(
    root: &Path,
    args: &[String],
    residual: &[String],
    now: DateTime<Utc>,
) -> anyhow::Result<String> {
    if args.is_empty() {
        bail!("usage: action \"done\", args: [<task-ref>], optional residual: [\"...\"]");
    }
    let rel = complete_task(root, &args[0], now)?;
    let mut out = format!("Marked {rel} done, appended {DIR_NAME}/log.md.\n");
    if !residual.is_empty() {
        out.push_str(&residual_tasks(root, &rel, residual, now));
    }
    out.push_str(&proposal_progress(root, &rel));
    out.push_str("\n▶ Next step:\n");
    out.push_str(&render_next(&loop_state(root)?.next()));
    Ok(out)
}

fn deferred_git_action(action: &str, args: &[String]) -> String {
    let task = args.first().map(String::as_str).unwrap_or("<task-ref>");
    match action {
        "preflight" => "Pre-flight (git/gh) is not native yet (Phase 2b). Verify before pushing:\n  - on the proposal's feature branch (not main/master)\n  - `git remote get-url origin` is set and reachable (`git ls-remote origin HEAD`)\n  - `gh auth status` is authenticated\n".to_string(),
        "ship" => format!(
            "`ship` (pre-flight + close) is not native yet (Phase 2b). Do the pre-flight above, then close the task with the `sdd` tool (action \"done\", args: [\"{task}\"]), then push and update the draft PR:\n  git push origin HEAD\n  gh pr edit --body \"<task checklist + manual QA>\"\n  gh pr ready   # only once every task in the proposal is done\n"
        ),
        _ => "`cleanup` (delete merged proposal branches) is not native yet (Phase 2b). After the PR merges, delete the branch manually:\n  git branch --merged <default> | grep -E '^(sdd/prop-|feat/)' | xargs -r git branch -D\n".to_string(),
    }
}

// ---- rendering + helpers (ported from main.go) ----

/// Renders a [`NextAction`] as model-facing next-step text, translating the
/// engine's `grok-sdd <sub>` command hints into native `sdd` tool actions while
/// leaving real git commands verbatim.
fn render_next(a: &NextAction) -> String {
    let label = if a.gate {
        "Next (human gate — your call)"
    } else {
        "Next"
    };
    let mut s = format!("{label}: {}\n", a.summary);
    if !a.command.is_empty() {
        if let Some(sub) = a.command.strip_prefix("grok-sdd ") {
            let (action, rest) = sub.split_once(' ').unwrap_or((sub, ""));
            if rest.is_empty() {
                s.push_str(&format!("  → use the `sdd` tool, action \"{action}\"\n"));
            } else {
                s.push_str(&format!(
                    "  → use the `sdd` tool, action \"{action}\": {rest}\n"
                ));
            }
        } else {
            s.push_str(&format!("  → {}\n", a.command));
        }
    }
    // For the implement step the engine prescribes the loop deterministically by
    // tier, so the model doesn't have to derive the ceremony from dense skill prose
    // (a slow model reads it loosely and defaults to full ceremony every time).
    if let Some(tier) = a.tier {
        s.push_str(&render_tier_plan(tier));
        if !a.skill.is_empty() {
            let hint = if tier == crate::tier::Tier::Trivial {
                "load only if you need the detail"
            } else {
                "load for the phase detail"
            };
            s.push_str(&format!("  skill: `{}` — {hint}\n", a.skill));
        }
    } else if !a.skill.is_empty() {
        s.push_str(&format!("  skill: load `{}` first, then act\n", a.skill));
    }
    if !a.then.is_empty() {
        s.push_str(&format!("  then: {}\n", a.then));
    }
    s
}

/// Renders the tier's concrete implement→review→ship loop as a single `plan` line.
/// This is the deterministic ceremony scaling: the model follows these knobs rather
/// than re-deriving "how much review does this deserve?" from prose each turn.
fn render_tier_plan(tier: crate::tier::Tier) -> String {
    let p = tier.plan();
    let test = if p.tdd {
        "TDD (failing test first) → green → refactor"
    } else {
        "compose from existing base components — no test-first (cover any real logic with a plain test)"
    };
    let build = if tier == crate::tier::Tier::Trivial {
        "typecheck + lint only (no full build until ship)"
    } else {
        "typecheck + lint + scoped tests (full build runs once at ship, not per fix)"
    };
    let review = match (p.craft_lens, p.security_lens, p.review_rounds) {
        (false, _, _) => {
            "one inline correctness review (no craft subagent, no round 2)".to_string()
        }
        (true, false, _) => {
            "review both lenses (correctness + craft), round 2 only if round 1 changed code"
                .to_string()
        }
        (true, true, _) => {
            "review both lenses, security lens mandatory, both rounds always".to_string()
        }
    };
    let close = if p.batch {
        "close + ship — all in this one turn"
    } else {
        "checkpoint, then ship"
    };
    format!("  plan ({tier}): {test} · {build} · {review} · {close}\n")
}

/// Reads `<root>/.git/HEAD`, returning `""` when detached, outside a repo, or a
/// worktree gitdir pointer. Dependency-free by design (matches Go's
/// `currentGitBranch`, which reads HEAD directly rather than resolving gitdir).
pub fn current_branch(root: &Path) -> String {
    let Ok(data) = fs::read_to_string(root.join(".git").join("HEAD")) else {
        return String::new();
    };
    data.trim()
        .strip_prefix("ref: refs/heads/")
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

fn ref_stem(refr: &str) -> String {
    let refr = refr.trim();
    let tail = refr.rsplit_once('/').map(|(_, t)| t).unwrap_or(refr);
    tail.strip_suffix(".md").unwrap_or(tail).to_string()
}

fn is_done_status(status: &str) -> bool {
    matches!(status.trim(), "done" | "completed")
}

fn decision_ref_for_task(root: &Path, task_rel: &str) -> String {
    let Ok(st) = read_status(root) else {
        return String::new();
    };
    let want = ref_stem(task_rel);
    for t in &st.tasks {
        if ref_stem(&t.name) == want {
            return t.decision.trim().to_string();
        }
    }
    String::new()
}

fn residual_tasks(root: &Path, task_rel: &str, residuals: &[String], now: DateTime<Utc>) -> String {
    let decision = decision_ref_for_task(root, task_rel);
    if decision.is_empty() {
        return format!(
            "\nwarning: {task_rel} has no decision link; cannot record residuals as follow-up tasks.\n"
        );
    }
    let mut out = format!("\nRecorded residual(s) as follow-up tasks on {decision}:\n");
    for r in residuals {
        let text = r.trim();
        if text.is_empty() {
            continue;
        }
        let title = if text.to_lowercase().starts_with("residual") {
            text.to_string()
        } else {
            format!("Residual: {text}")
        };
        match add_task_with_tier(root, &decision, &title, None, now) {
            Ok(rel) => out.push_str(&format!("  + {rel}\n")),
            Err(e) => out.push_str(&format!(
                "  ! could not create residual task {text:?}: {e}\n"
            )),
        }
    }
    out
}

fn proposal_progress(root: &Path, completed_rel: &str) -> String {
    let Ok(st) = read_status(root) else {
        return String::new();
    };
    if st.tasks.is_empty() {
        return String::new();
    }
    let completed = ref_stem(completed_rel);
    let decision = st
        .tasks
        .iter()
        .find(|t| ref_stem(&t.name) == completed)
        .map(|t| ref_stem(&t.decision))
        .unwrap_or_default();
    if decision.is_empty() {
        return String::new();
    }
    let mut checklist = String::new();
    let mut pending = 0;
    for t in &st.tasks {
        if ref_stem(&t.decision) != decision {
            continue;
        }
        let box_ = if is_done_status(&t.status) {
            "[x]"
        } else {
            pending += 1;
            "[ ]"
        };
        checklist.push_str(&format!("  - {box_} {}", t.name));
        if !t.title.is_empty() {
            checklist.push_str(&format!(" — {}", t.title));
        }
        checklist.push('\n');
    }
    let mut out = format!("\nProposal tasks (this PR):\n{checklist}");
    if pending == 0 {
        out.push_str("\nEvery task in this proposal is done. Refresh the PR body with the checklist above, then take it out of draft:\n  gh pr edit --body \"<checklist + manual QA>\"\n  gh pr ready\n");
    } else {
        out.push_str(&format!("\n{pending} task(s) still pending — keep them on this branch (one PR per proposal) and leave the PR a draft. Push and refresh the PR body:\n  git push origin HEAD\n  gh pr edit --body \"<checklist above + manual QA>\"\n"));
    }
    out
}

/// Creates `<root>/.grok-sdd/require-branch` (empty) to opt the repo into the
/// feature-branch guard. Reports whether it created the marker.
fn write_require_branch_marker(root: &Path) -> anyhow::Result<bool> {
    let marker = root.join(".grok-sdd").join("require-branch");
    if marker.exists() {
        return Ok(false);
    }
    fs::create_dir_all(marker.parent().unwrap())?;
    fs::write(&marker, "")?;
    Ok(true)
}

/// Writes `.grok/rules/sdd.md` — the standing SDD policy. Skips an existing file
/// so a user's edits are never clobbered. Reports whether it wrote the file.
fn write_rules_file(root: &Path) -> anyhow::Result<bool> {
    let path = root.join(".grok").join("rules").join("sdd.md");
    if path.exists() {
        return Ok(false);
    }
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(&path, RULES_TEMPLATE)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-07-25T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn run(root: &Path, action: &str, args: &[&str]) -> anyhow::Result<String> {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        dispatch(root, action, &args, None, None, &[], now())
    }

    #[test]
    fn init_then_next_points_to_discovery() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let init = run(root, "init", &[]).unwrap();
        assert!(init.contains("Scaffolded"));
        assert!(root.join(".grok/rules/sdd.md").exists());
        assert!(root.join(".grok-sdd/require-branch").exists());
        let next = run(root, "next", &[]).unwrap();
        assert!(
            next.contains("sdd") && next.contains("skill: load `sdd-discovery`"),
            "next:\n{next}"
        );
    }

    #[test]
    fn propose_then_status_shows_in_review_gate() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        run(root, "propose", &["Build a POS for a pollería"]).unwrap();
        let status = run(root, "status", &[]).unwrap();
        assert!(status.contains("SDD (OKF)"), "status:\n{status}");
        // A seeded proposal is in review → next is the approve gate.
        let next = run(root, "next", &[]).unwrap();
        assert!(
            next.contains("human gate") && next.contains("action \"approve\""),
            "next:\n{next}"
        );
    }

    #[test]
    fn full_loop_smoke() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        run(root, "init", &[]).unwrap();
        run(root, "propose", &["Architecture: TS + InstantDB"]).unwrap();
        let approved =
            dispatch(root, "approve", &[], Some("Architecture"), None, &[], now()).unwrap();
        assert!(approved.contains("Approved"));
        let task = run(
            root,
            "task",
            &["decisions/001-architecture.md", "Wire", "auth"],
        )
        .unwrap();
        assert!(task.contains("Created task"));
        let done = run(root, "done", &["001-wire-auth"]).unwrap();
        assert!(
            done.contains("done") && done.contains("Next step"),
            "done:\n{done}"
        );
    }

    #[test]
    fn render_next_scales_plan_by_tier() {
        use crate::loop_state::NextAction;
        use crate::tier::Tier;
        let base = NextAction {
            summary: "Implement 006-caja".into(),
            skill: "sdd-implement".into(),
            ..Default::default()
        };
        let trivial = render_next(&NextAction {
            tier: Some(Tier::Trivial),
            ..base.clone()
        });
        assert!(trivial.contains("plan (trivial)"), "{trivial}");
        assert!(trivial.contains("no test-first"), "{trivial}");
        assert!(trivial.contains("all in this one turn"), "{trivial}");
        assert!(trivial.contains("load only if you need"), "{trivial}");

        let critical = render_next(&NextAction {
            tier: Some(Tier::Critical),
            ..base
        });
        assert!(critical.contains("security lens mandatory"), "{critical}");
        assert!(critical.contains("both rounds always"), "{critical}");
        assert!(critical.contains("checkpoint, then ship"), "{critical}");
        // The "dense phase rules" phrasing that invited heavy loading is gone.
        assert!(!critical.contains("dense"), "{critical}");
    }

    #[test]
    fn deferred_actions_explain_manual_steps() {
        let tmp = tempfile::tempdir().unwrap();
        let out = run(tmp.path(), "ship", &["001-x"]).unwrap();
        assert!(out.contains("not native yet") && out.contains("done"));
    }

    #[test]
    fn unknown_action_errors() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(run(tmp.path(), "frobnicate", &[]).is_err());
    }
}
