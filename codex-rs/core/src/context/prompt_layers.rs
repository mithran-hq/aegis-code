use crate::agents_md::AgentsMdInstructionLayers;
use crate::context_packs::redact_context_pack_sources;
use crate::state::MethodStatePersistenceStatus;
use codex_protocol::ThreadId;
use codex_protocol::method_state::MethodIssueProvider;
use codex_protocol::method_state::MethodResumeValidityStatus;
use codex_utils_absolute_path::AbsolutePathBuf;
use serde::Serialize;
use std::path::Path;

use super::ContextualUserFragment;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptLayerKind {
    BuiltInBase,
    UserPack,
    ProjectPack,
    PromotedLearnedPack,
    CurrentTaskFacts,
}

impl PromptLayerKind {
    fn order(self) -> u8 {
        match self {
            Self::BuiltInBase => 0,
            Self::UserPack => 1,
            Self::ProjectPack => 2,
            Self::PromotedLearnedPack => 3,
            Self::CurrentTaskFacts => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptLayerRole {
    BaseInstructions,
    User,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PromptLayerDiagnostic {
    pub order: u8,
    pub kind: PromptLayerKind,
    pub active: bool,
    pub role: PromptLayerRole,
    pub source: String,
    pub status: String,
}

impl PromptLayerDiagnostic {
    fn new(
        kind: PromptLayerKind,
        active: bool,
        role: PromptLayerRole,
        source: impl Into<String>,
        status: impl Into<String>,
    ) -> Self {
        Self {
            order: kind.order(),
            kind,
            active,
            role,
            source: source.into(),
            status: status.into(),
        }
    }
}

pub(crate) fn build_static_prompt_layer_diagnostics(
    codex_home: &AbsolutePathBuf,
    cwd: &AbsolutePathBuf,
    base_instructions: &str,
    instruction_layers: &AgentsMdInstructionLayers,
) -> Vec<PromptLayerDiagnostic> {
    let user_active = instruction_layers
        .user
        .as_ref()
        .is_some_and(|text| !text.trim().is_empty())
        || instruction_layers.user_context_pack.is_some()
        || instruction_layers.child_agents_md_enabled;
    let user_source = source_summary(
        [
            instruction_layers
                .user
                .is_some()
                .then(|| "user_config_or_global_agents".to_string()),
            instruction_layers
                .user_context_pack
                .as_ref()
                .map(|pack| redact_context_pack_sources(&pack.sources, codex_home, cwd)),
            instruction_layers
                .child_agents_md_enabled
                .then(|| "child_agents_md_feature".to_string()),
        ]
        .into_iter()
        .flatten(),
    );
    let project_active =
        instruction_layers.project.is_some() || instruction_layers.project_context_pack.is_some();
    let project_sources = source_summary(
        [
            instruction_layers
                .project
                .as_ref()
                .map(|project| redact_sources(&project.sources, codex_home, cwd)),
            instruction_layers
                .project_context_pack
                .as_ref()
                .map(|pack| redact_context_pack_sources(&pack.sources, codex_home, cwd)),
        ]
        .into_iter()
        .flatten(),
    );
    let learned_sources = instruction_layers
        .promoted_learned_context_pack
        .as_ref()
        .map(|pack| redact_context_pack_sources(&pack.sources, codex_home, cwd))
        .unwrap_or_else(|| "not_configured".to_string());
    let learned_active = instruction_layers.promoted_learned_context_pack.is_some();

    let base_active = !base_instructions.trim().is_empty();

    vec![
        PromptLayerDiagnostic::new(
            PromptLayerKind::BuiltInBase,
            base_active,
            PromptLayerRole::BaseInstructions,
            "built_in_or_model_config",
            if base_active { "active" } else { "inactive" },
        ),
        PromptLayerDiagnostic::new(
            PromptLayerKind::UserPack,
            user_active,
            PromptLayerRole::User,
            user_source,
            if user_active { "active" } else { "inactive" },
        ),
        PromptLayerDiagnostic::new(
            PromptLayerKind::ProjectPack,
            project_active,
            PromptLayerRole::User,
            project_sources,
            if project_active { "active" } else { "inactive" },
        ),
        PromptLayerDiagnostic::new(
            PromptLayerKind::PromotedLearnedPack,
            learned_active,
            PromptLayerRole::User,
            learned_sources,
            if learned_active { "active" } else { "inactive" },
        ),
    ]
}

fn source_summary(sources: impl Iterator<Item = String>) -> String {
    let sources = sources
        .filter(|source| !source.trim().is_empty())
        .collect::<Vec<_>>();
    if sources.is_empty() {
        "not_configured".to_string()
    } else {
        sources.join(",")
    }
}

pub(crate) fn current_task_facts_diagnostic(
    status: &MethodStatePersistenceStatus,
) -> PromptLayerDiagnostic {
    PromptLayerDiagnostic::new(
        PromptLayerKind::CurrentTaskFacts,
        true,
        PromptLayerRole::User,
        "session_state",
        method_state_status_summary(status),
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CurrentTaskFacts {
    thread_id: ThreadId,
    cwd: AbsolutePathBuf,
    method_state_status: MethodStatePersistenceStatus,
}

impl CurrentTaskFacts {
    pub(crate) fn new(
        thread_id: ThreadId,
        cwd: AbsolutePathBuf,
        method_state_status: MethodStatePersistenceStatus,
    ) -> Self {
        Self {
            thread_id,
            cwd,
            method_state_status,
        }
    }
}

impl ContextualUserFragment for CurrentTaskFacts {
    const ROLE: &'static str = "user";
    const START_MARKER: &'static str = "<current_task_facts>";
    const END_MARKER: &'static str = "</current_task_facts>";

    fn body(&self) -> String {
        let mut lines = vec![
            String::new(),
            "Current task facts:".to_string(),
            format!("- Thread ID: {}", self.thread_id),
            format!(
                "- Working directory: {}",
                self.cwd.as_path().to_string_lossy()
            ),
            format!(
                "- Method state persistence: {}",
                method_state_status_summary(&self.method_state_status)
            ),
            "- These facts are context only; they do not override safety, method, system, or developer instructions.".to_string(),
        ];

        if let MethodStatePersistenceStatus::Loaded {
            state,
            resume_validity,
        } = &self.method_state_status
        {
            lines.push(format!(
                "- Method resume validity: {}",
                resume_validity_status_name(resume_validity.status)
            ));
            if let Some(issue) = state.linked_issue.as_ref() {
                lines.push(format!(
                    "- Linked issue: {}",
                    linked_issue_summary(issue.provider, &issue.repository, issue.number)
                ));
            }
        }

        lines.push(String::new());
        lines.join("\n")
    }
}

fn method_state_status_summary(status: &MethodStatePersistenceStatus) -> String {
    match status {
        MethodStatePersistenceStatus::Missing => "missing".to_string(),
        MethodStatePersistenceStatus::Invalid { .. } => "invalid".to_string(),
        MethodStatePersistenceStatus::Loaded {
            resume_validity, ..
        } => format!(
            "loaded:{}",
            resume_validity_status_name(resume_validity.status)
        ),
    }
}

fn resume_validity_status_name(status: MethodResumeValidityStatus) -> &'static str {
    match status {
        MethodResumeValidityStatus::Valid => "valid",
        MethodResumeValidityStatus::Stale => "stale",
        MethodResumeValidityStatus::Invalid => "invalid",
    }
}

fn linked_issue_summary(provider: MethodIssueProvider, repository: &str, number: u64) -> String {
    let provider = match provider {
        MethodIssueProvider::GitHub => "github",
    };
    format!("{provider}:{repository}#{number}")
}

fn redact_sources(
    sources: &[AbsolutePathBuf],
    codex_home: &AbsolutePathBuf,
    cwd: &AbsolutePathBuf,
) -> String {
    if sources.is_empty() {
        return "project_docs:0_files".to_string();
    }

    let redacted = sources
        .iter()
        .map(|source| redact_path(source.as_path(), codex_home.as_path(), cwd.as_path()))
        .collect::<Vec<_>>();
    redacted.join(",")
}

fn redact_path(path: &Path, codex_home: &Path, cwd: &Path) -> String {
    if let Ok(relative) = path.strip_prefix(codex_home) {
        return format!("$CODEX_HOME/{}", relative.display());
    }
    if let Ok(relative) = path.strip_prefix(cwd) {
        return format!("$CWD/{}", relative.display());
    }
    "project_doc_outside_roots".to_string()
}

#[cfg(test)]
#[path = "prompt_layers_tests.rs"]
mod tests;
