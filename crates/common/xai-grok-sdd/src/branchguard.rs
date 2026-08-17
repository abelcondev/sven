//! The compiled "work on a branch, not on the protected branch" gate. It refuses
//! to write source code while HEAD is on a protected branch, in repos that opt in
//! via a `.grok-sdd/require-branch` marker or `GROK_SDD_REQUIRE_BRANCH=on`. Ported
//! from `internal/branchguard`.

use crate::qualitygate::is_gated_source_file;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// The resolved guard decision for a workspace at write time.
#[derive(Debug, Clone, Default)]
pub struct State {
    /// The repo opted in (marker or env).
    pub enabled: bool,
    /// Current branch name, `""` when detached or unreadable.
    pub branch: String,
    /// `branch` is a protected/default branch.
    pub protected: bool,
}

/// Returned when a write is blocked because the repo requires a feature branch.
/// Its message is model-facing: it names the branch and the fix.
#[derive(Debug, Clone)]
pub struct GuardError {
    pub branch: String,
    pub path: String,
}

impl std::fmt::Display for GuardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Error: write blocked — you are on protected branch {:?}, but this repo requires feature branches + PRs. \
             Create a feature branch and retry: `git checkout -b feat/<short-name>`, then implement, push, and open a PR. \
             {} was NOT written. (Override for this run with GROK_SDD_REQUIRE_BRANCH=off.)",
            self.branch, self.path
        )
    }
}

impl std::error::Error for GuardError {}

/// Enforces the branch guard for a write of `relative_path` under `workspace_root`,
/// reading its configuration and git state from the environment and filesystem.
/// Returns `Ok(())` when the write is allowed.
pub fn check(workspace_root: &str, relative_path: &str) -> Result<(), GuardError> {
    check_state(
        &resolve(workspace_root, |k| std::env::var(k).unwrap_or_default()),
        relative_path,
    )
}

/// The pure decision core: it needs only a resolved `State` and the path, so tests
/// can exercise every branch without touching git or the fs.
pub fn check_state(state: &State, relative_path: &str) -> Result<(), GuardError> {
    if !state.enabled || !state.protected {
        return Ok(());
    }
    // Only code is gated; SDD notes, docs, and config may be edited on any branch.
    if !is_gated_source_file(relative_path) {
        return Ok(());
    }
    Err(GuardError {
        branch: state.branch.clone(),
        path: relative_path.to_string(),
    })
}

/// Inspects the repo containing `workspace_root` and returns the guard `State`. A
/// non-git path, an opted-out repo, or an unreadable HEAD all resolve to "do not
/// block" (fail open).
pub fn resolve(workspace_root: &str, getenv: impl Fn(&str) -> String) -> State {
    let getenv: &dyn Fn(&str) -> String = &getenv;
    let start = workspace_root.trim();
    if start.is_empty() {
        return State::default();
    }
    let Some(root) = find_git_root(Path::new(start)) else {
        return State::default();
    };
    if !enabled(&root, getenv) {
        return State::default();
    }
    let Some(git_dir) = resolve_git_dir(&root) else {
        return State {
            enabled: true,
            ..Default::default()
        };
    };
    let branch = current_branch(&git_dir);
    let protected = !branch.is_empty() && protected_branches(&git_dir, getenv).contains(&branch);
    State {
        enabled: true,
        branch,
        protected,
    }
}

/// Resolves opt-in: `GROK_SDD_REQUIRE_BRANCH` forces on/off; otherwise the
/// presence of a `.grok-sdd/require-branch` marker at the git root enables it.
fn enabled(root: &Path, getenv: &dyn Fn(&str) -> String) -> bool {
    match getenv("GROK_SDD_REQUIRE_BRANCH")
        .trim()
        .to_lowercase()
        .as_str()
    {
        "on" | "1" | "true" | "yes" => return true,
        "off" | "0" | "false" | "no" => return false,
        _ => {}
    }
    root.join(".grok-sdd").join("require-branch").exists()
}

/// Walks up from `start` looking for a `.git` directory or file, returning the
/// repository (work-tree) root.
fn find_git_root(start: &Path) -> Option<PathBuf> {
    let mut dir = start;
    loop {
        if dir.join(".git").exists() {
            return Some(dir.to_path_buf());
        }
        match dir.parent() {
            Some(parent) => dir = parent,
            None => return None,
        }
    }
}

/// Returns the git directory holding HEAD for the repo at `root`, handling both a
/// normal `.git` directory and a `.git` file (linked worktrees / submodules).
fn resolve_git_dir(root: &Path) -> Option<PathBuf> {
    let git_path = root.join(".git");
    let meta = fs::metadata(&git_path).ok()?;
    if meta.is_dir() {
        return Some(git_path);
    }
    let data = fs::read_to_string(&git_path).ok()?;
    let line = data.trim();
    let git_dir = line.strip_prefix("gitdir:")?.trim();
    let git_dir = Path::new(git_dir);
    if git_dir.is_absolute() {
        Some(git_dir.to_path_buf())
    } else {
        Some(root.join(git_dir))
    }
}

/// Resolves the checked-out branch for the work-tree containing `start`, reading
/// only `.git/HEAD` (no `git` subprocess). Returns `""` for a detached HEAD, a
/// non-git path, or an unreadable HEAD. Walks up to the repo root, so a session
/// cwd nested inside the repo resolves correctly.
pub fn current_branch_at(start: &Path) -> String {
    find_git_root(start)
        .and_then(|root| resolve_git_dir(&root))
        .map(|git_dir| current_branch(&git_dir))
        .unwrap_or_default()
}

/// Reads the checked-out branch from HEAD, returning `""` for a detached HEAD or
/// an unreadable HEAD.
fn current_branch(git_dir: &Path) -> String {
    let Ok(data) = fs::read_to_string(git_dir.join("HEAD")) else {
        return String::new();
    };
    let head = data.trim();
    head.strip_prefix("ref: refs/heads/")
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// The set of branch names the guard treats as protected: main and master always,
/// plus the remote default when recorded, plus any from
/// `GROK_SDD_PROTECTED_BRANCHES`.
fn protected_branches(git_dir: &Path, getenv: &dyn Fn(&str) -> String) -> HashSet<String> {
    let mut set = HashSet::new();
    set.insert("main".to_string());
    set.insert("master".to_string());
    if let Ok(data) = fs::read_to_string(
        git_dir
            .join("refs")
            .join("remotes")
            .join("origin")
            .join("HEAD"),
    ) && let Some(name) = data.trim().strip_prefix("ref: refs/remotes/origin/")
    {
        set.insert(name.trim().to_string());
    }
    for name in getenv("GROK_SDD_PROTECTED_BRANCHES").split(',') {
        let name = name.trim();
        if !name.is_empty() {
            set.insert(name.to_string());
        }
    }
    set
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env_map(m: &[(&str, &str)]) -> impl Fn(&str) -> String {
        let m: HashMap<String, String> = m
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |k: &str| m.get(k).cloned().unwrap_or_default()
    }

    fn init_repo(branch: &str, with_marker: bool) -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join(".git");
        fs::create_dir_all(&git_dir).unwrap();
        fs::write(git_dir.join("HEAD"), format!("ref: refs/heads/{branch}\n")).unwrap();
        if with_marker {
            fs::create_dir_all(tmp.path().join(".grok-sdd")).unwrap();
            fs::write(tmp.path().join(".grok-sdd").join("require-branch"), "").unwrap();
        }
        tmp
    }

    fn root_str(t: &tempfile::TempDir) -> String {
        t.path().to_string_lossy().to_string()
    }

    #[test]
    fn check_state_blocks_code_on_protected_branch() {
        let blocked = State {
            enabled: true,
            branch: "main".into(),
            protected: true,
        };
        assert!(check_state(&blocked, "internal/foo.go").is_err());
        assert!(
            check_state(&blocked, "sdd/proposal.md").is_ok(),
            "markdown must be allowed"
        );
        assert!(
            check_state(&blocked, "README.md").is_ok(),
            "docs must be allowed"
        );
    }

    #[test]
    fn check_state_allows_when_not_protected_or_disabled() {
        let cases = [
            State {
                enabled: true,
                branch: "feat/x".into(),
                protected: false,
            },
            State {
                enabled: false,
                branch: "main".into(),
                protected: true,
            },
            State {
                enabled: true,
                branch: String::new(),
                protected: false,
            },
        ];
        for st in cases {
            assert!(
                check_state(&st, "internal/foo.go").is_ok(),
                "state {st:?} must allow"
            );
        }
    }

    #[test]
    fn guard_error_guides_the_model() {
        let err = check_state(
            &State {
                enabled: true,
                branch: "main".into(),
                protected: true,
            },
            "internal/big.go",
        )
        .unwrap_err();
        let msg = err.to_string();
        for want in [
            "main",
            "internal/big.go",
            "git checkout -b",
            "GROK_SDD_REQUIRE_BRANCH=off",
        ] {
            assert!(msg.contains(want), "guard message missing {want:?}: {msg}");
        }
    }

    #[test]
    fn resolve_marker_enables_guard_on_protected_branch() {
        let t = init_repo("main", true);
        let st = resolve(&root_str(&t), env_map(&[]));
        assert!(
            st.enabled && st.protected && st.branch == "main",
            "got {st:?}"
        );
    }

    #[test]
    fn resolve_no_marker_no_env_is_disabled() {
        let t = init_repo("main", false);
        assert!(!resolve(&root_str(&t), env_map(&[])).enabled);
    }

    #[test]
    fn resolve_env_on_forces_guard_without_marker() {
        let t = init_repo("master", false);
        let st = resolve(&root_str(&t), env_map(&[("GROK_SDD_REQUIRE_BRANCH", "on")]));
        assert!(st.enabled && st.protected, "got {st:?}");
    }

    #[test]
    fn resolve_env_off_beats_marker() {
        let t = init_repo("main", true);
        assert!(
            !resolve(
                &root_str(&t),
                env_map(&[("GROK_SDD_REQUIRE_BRANCH", "off")])
            )
            .enabled
        );
    }

    #[test]
    fn resolve_feature_branch_not_protected() {
        let t = init_repo("feat/login", true);
        let st = resolve(&root_str(&t), env_map(&[]));
        assert!(st.enabled);
        assert!(!st.protected, "got {st:?}");
        assert!(check_state(&st, "internal/foo.go").is_ok());
    }

    #[test]
    fn resolve_custom_protected_branch_via_env() {
        let t = init_repo("develop", true);
        let st = resolve(
            &root_str(&t),
            env_map(&[("GROK_SDD_PROTECTED_BRANCHES", "develop, release")]),
        );
        assert!(st.protected, "got {st:?}");
    }

    #[test]
    fn resolve_non_git_path_is_disabled() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(
            !resolve(
                &tmp.path().to_string_lossy(),
                env_map(&[("GROK_SDD_REQUIRE_BRANCH", "on")])
            )
            .enabled
        );
    }

    #[test]
    fn resolve_detached_head_is_not_protected() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join(".git");
        fs::create_dir_all(&git_dir).unwrap();
        fs::write(git_dir.join("HEAD"), "9f8e7d6c5b4a\n").unwrap();
        fs::create_dir_all(tmp.path().join(".grok-sdd")).unwrap();
        fs::write(tmp.path().join(".grok-sdd").join("require-branch"), "").unwrap();
        let st = resolve(&tmp.path().to_string_lossy(), env_map(&[]));
        assert!(!st.protected && st.branch.is_empty(), "got {st:?}");
    }
}
