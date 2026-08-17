//! # xai-grok-sdd
//!
//! The Spec-Driven Development (SDD) loop engine, ported from the vendored Go
//! `grok-sdd` engine to native Rust for integration into grok-build (Phase 1 of
//! `docs/NATIVE_SDD_INTEGRATION.md`).
//!
//! It is a pure library: it owns the OKF knowledge base on disk (`sdd/`), the
//! resumable loop advisor ([`loop_state`]), the ceremony-weight classifier
//! ([`tier`]), the deterministic route advisor ([`route`]), the branch guard
//! ([`branchguard`]), the code-length quality gate ([`qualitygate`]), and the
//! embedded phase skills self-extracted to `$GROK_HOME/skills/` ([`skills`]) so
//! the whole loop ships in one `grok` binary. It has no CLI; its only I/O is the
//! workspace filesystem, git metadata files, and that one-time skill extraction.

pub mod branchguard;
pub mod cli;
pub mod lifecycle;
pub mod loop_state;
pub mod propose;
pub mod qualitygate;
pub mod route;
pub mod scaffold;
pub mod skills;
pub mod tier;
mod util;

// Flat, ergonomic re-exports of the public surface.
pub use loop_state::{
    LoopState, NextAction, SKILL_IMPLEMENT, is_protected_branch, normalize_ref, read_loop_state,
};
pub use scaffold::{DIR_NAME, Status, TaskInfo, read_status, scaffold};
pub use tier::{Tier, infer_tier, parse_tier, resolve_tier};

pub use lifecycle::{
    add_design, add_task, add_task_with_tier, approve_design, complete_task, promote,
};
pub use propose::{proposal_branch_name, seed_proposal};
