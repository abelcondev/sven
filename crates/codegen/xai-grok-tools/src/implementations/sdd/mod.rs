//! The native `sdd` tool — Spec-Driven Development loop control.
//!
//! Replaces the external `grok-sdd` Go binary (invoked via the terminal + a Stop
//! hook) with an in-process tool backed by the `xai-grok-sdd` engine crate. The
//! agent drives the loop by calling this tool with an `action`; there is no
//! subprocess per turn. Phase 2 of `docs/NATIVE_SDD_INTEGRATION.md`.

use crate::types::output::ToolOutput;
use crate::types::tool::{ToolKind, ToolNamespace};
use crate::types::tool_metadata::{ToolMetadata, resolve_cwd, shared_resources};

/// Registered wire name of the tool. Single source of truth for the id.
pub const SDD_TOOL_NAME: &str = "sdd";

const SDD_DESCRIPTION: &str = "Drive this project's Spec-Driven Development (SDD) loop. \
The durable state lives on disk under `sdd/`; this tool reads and mutates it.\n\n\
Pass an `action`:\n\
- `next` — the single recommended next step (cheap, resumable; prefer it over guessing where you are).\n\
- `status` — the fuller picture (decisions, tasks, current branch, next step).\n\
- `init` — scaffold the `sdd/` knowledge base in this repo.\n\
- `propose` — seed `sdd/proposal.md` from a description in `args` (then expand it in place with your edit tools).\n\
- `approve` — promote the in-review proposal to a numbered decision (`title` overrides the proposal title).\n\
- `task` — add a task: `args: [<decision-ref>, <title...>]`, optional `tier` (trivial|standard|critical).\n\
- `design` — add a UI design (the gate before UI code): `args: [<decision-ref>, <title...>]`.\n\
- `approve-design` — approve an in-review design: `args: [<design-ref>]`.\n\
- `done` — close a task: `args: [<task-ref>]`, optional `residual` follow-up notes.\n\n\
Do one step, then stop at human gates (proposal approval, design review). \
(`ship`/`preflight`/`cleanup` return the manual git/gh steps to run for now.)";

/// Structured input for the `sdd` tool. `args` are the positional arguments;
/// `title`/`tier`/`residual` mirror the old CLI flags.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct SddInput {
    #[schemars(
        description = "The SDD action: next, status, init, propose, approve, task, design, approve-design, done. (ship/preflight/cleanup return manual git/gh steps for now.)"
    )]
    pub action: String,
    #[serde(default)]
    #[schemars(
        description = "Positional arguments. propose: [description]. task/design: [decision-ref, title...]. approve-design/done: [ref]."
    )]
    pub args: Vec<String>,
    #[serde(default)]
    #[schemars(
        description = "Decision title for `approve` (overrides the proposal's frontmatter title)."
    )]
    pub title: Option<String>,
    #[serde(default)]
    #[schemars(description = "Tier override for `task`: trivial | standard | critical.")]
    pub tier: Option<String>,
    #[serde(default)]
    #[schemars(
        description = "Residual follow-up notes for `done`; each is recorded as a follow-up task."
    )]
    pub residual: Vec<String>,
}

/// The native SDD loop tool.
#[derive(Debug, Default)]
pub struct SddTool;

impl ToolMetadata for SddTool {
    fn kind(&self) -> ToolKind {
        // `Other` is the generic (non-read-only) bucket; the tool mutates `sdd/`.
        ToolKind::Other
    }

    fn tool_namespace(&self) -> ToolNamespace {
        ToolNamespace::GrokBuild
    }

    fn description_template(&self) -> &str {
        SDD_DESCRIPTION
    }
}

impl xai_tool_runtime::Tool for SddTool {
    type Args = SddInput;
    type Output = ToolOutput;

    fn id(&self) -> xai_tool_protocol::ToolId {
        xai_tool_protocol::ToolId::new(SDD_TOOL_NAME).expect("valid tool id")
    }

    fn description(
        &self,
        _ctx: &xai_tool_runtime::ListToolsContext,
    ) -> xai_tool_types::ToolDescription {
        xai_tool_types::ToolDescription::new(
            SDD_TOOL_NAME,
            ToolMetadata::sanitized_description_template(self),
        )
    }

    async fn run(
        &self,
        ctx: xai_tool_runtime::ToolCallContext,
        input: SddInput,
    ) -> Result<ToolOutput, xai_tool_runtime::ToolError> {
        let resources = shared_resources(&ctx)?;
        let cwd = resolve_cwd(&ctx, &resources).await?;
        let text = xai_grok_sdd::cli::dispatch(
            &cwd,
            &input.action,
            &input.args,
            input.title.as_deref(),
            input.tier.as_deref(),
            &input.residual,
            chrono::Utc::now(),
        )
        .map_err(|e| {
            xai_tool_runtime::ToolError::execution(
                xai_tool_protocol::ToolId::new(SDD_TOOL_NAME).expect("valid"),
                e.to_string(),
            )
        })?;
        Ok(ToolOutput::Text(text.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_id_matches_name() {
        assert_eq!(
            xai_tool_runtime::Tool::id(&SddTool).to_string(),
            SDD_TOOL_NAME
        );
    }
}
