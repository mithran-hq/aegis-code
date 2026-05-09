use crate::aegis_secret::SensitiveCommandAnalysis;
use crate::aegis_secret::analyze_sensitive_command;
pub use crate::sandbox_policy::SandboxPolicyContext;
use crate::sandbox_policy::SandboxPolicyViolation;
use crate::sandbox_policy::evaluate_sandbox_policy;
use crate::state::MethodStatePersistenceStatus;
use codex_protocol::aegis_secret_policy::AegisSecretRiskCategory;
use codex_protocol::method_state::MethodResumeValidityStatus;
use codex_protocol::protocol::AegisPreflightDecisionEvent;
use codex_protocol::protocol::AegisPreflightVerdict;
use codex_shell_command::bash::parse_shell_lc_plain_commands;
use codex_shell_command::bash::parse_shell_lc_single_command_prefix;
use codex_utils_absolute_path::AbsolutePathBuf;
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToolPreflightSubject {
    Command {
        command: Vec<String>,
        cwd: AbsolutePathBuf,
    },
    FileSystemWrite {
        cwd: AbsolutePathBuf,
        paths: Vec<AbsolutePathBuf>,
        change_count: usize,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolPreflightSpec {
    pub subject: ToolPreflightSubject,
    pub sandbox_bypass_requested: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ToolPreflightDecision {
    pub verdict: AegisPreflightVerdict,
    pub risk_category: Option<AegisSecretRiskCategory>,
    pub reason: String,
    pub required_evidence_ids: Vec<String>,
    pub(crate) sandbox_policy_violation: Option<SandboxPolicyViolation>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ToolPreflightContext {
    pub method_state_available: bool,
    pub method_state_valid: bool,
    pub linked_issue_available: bool,
    pub sandbox_policy: Option<SandboxPolicyContext>,
}

impl ToolPreflightContext {
    pub(crate) fn from_status(status: MethodStatePersistenceStatus) -> Self {
        match status {
            MethodStatePersistenceStatus::Loaded {
                state,
                resume_validity,
            } => {
                let method_state_valid =
                    resume_validity.status == MethodResumeValidityStatus::Valid;
                Self {
                    method_state_available: true,
                    method_state_valid,
                    linked_issue_available: state.linked_issue.is_some(),
                    sandbox_policy: None,
                }
            }
            MethodStatePersistenceStatus::Missing
            | MethodStatePersistenceStatus::Invalid { .. } => Self {
                method_state_available: false,
                method_state_valid: false,
                linked_issue_available: false,
                sandbox_policy: None,
            },
        }
    }

    #[cfg(test)]
    fn valid_with_issue() -> Self {
        Self {
            method_state_available: true,
            method_state_valid: true,
            linked_issue_available: true,
            sandbox_policy: None,
        }
    }

    fn has_task_scope(&self) -> bool {
        self.method_state_available && self.method_state_valid && self.linked_issue_available
    }

    fn missing_context_reason(&self, action: &str) -> String {
        let mut missing = Vec::new();
        if !self.method_state_available {
            missing.push("loaded method state");
        } else if !self.method_state_valid {
            missing.push("valid method state");
        }
        if !self.linked_issue_available {
            missing.push("linked task issue");
        }
        format!(
            "Aegis preflight blocked {action}: missing {}. Required evidence: evidence:task-scope.",
            missing.join(" and ")
        )
    }
}

pub fn evaluate_preflight(
    spec: &ToolPreflightSpec,
    context: &ToolPreflightContext,
    workspace_cwd: &AbsolutePathBuf,
) -> ToolPreflightDecision {
    match &spec.subject {
        ToolPreflightSubject::Command { command, .. } => {
            evaluate_command(command, spec.sandbox_bypass_requested, context)
        }
        ToolPreflightSubject::FileSystemWrite {
            paths,
            change_count,
            ..
        } => evaluate_filesystem(paths, *change_count, workspace_cwd, context),
    }
}

pub(crate) fn event_for_decision(
    call_id: &str,
    turn_id: &str,
    tool_name: &str,
    spec: &ToolPreflightSpec,
    decision: &ToolPreflightDecision,
) -> AegisPreflightDecisionEvent {
    let (command, paths) = match &spec.subject {
        ToolPreflightSubject::Command { command, .. } => {
            (Some(redact_command(command)), Vec::new())
        }
        ToolPreflightSubject::FileSystemWrite { paths, .. } => (
            None,
            paths
                .iter()
                .map(|path| path.as_path().display().to_string())
                .collect(),
        ),
    };

    AegisPreflightDecisionEvent {
        call_id: call_id.to_string(),
        turn_id: turn_id.to_string(),
        tool_name: tool_name.to_string(),
        verdict: decision.verdict,
        risk_category: decision.risk_category,
        reason: decision.reason.clone(),
        required_evidence_ids: decision.required_evidence_ids.clone(),
        command,
        paths,
    }
}

fn redact_command(command: &[String]) -> Vec<String> {
    let mut redact_next = false;
    command
        .iter()
        .map(|arg| {
            if redact_next {
                redact_next = false;
                return "<redacted>".to_string();
            }

            let lower = arg.to_ascii_lowercase();
            let sensitive = lower.contains("token")
                || lower.contains("password")
                || lower.contains("secret")
                || lower.contains("authorization")
                || lower.contains("bearer")
                || lower.contains("api-key")
                || lower.contains("api_key")
                || lower.contains("apikey")
                || lower == "-k"
                || lower == "--key";
            if !sensitive {
                return arg.clone();
            }

            if let Some((name, _)) = arg.split_once('=') {
                format!("{name}=<redacted>")
            } else if arg.starts_with('-') {
                redact_next = true;
                arg.clone()
            } else {
                "<redacted>".to_string()
            }
        })
        .collect()
}

fn evaluate_command(
    command: &[String],
    sandbox_bypass_requested: bool,
    context: &ToolPreflightContext,
) -> ToolPreflightDecision {
    if let Some(action) = classify_command(command) {
        if action.is_protected()
            && let Some(violation) = sandbox_policy_violation(context, sandbox_bypass_requested)
        {
            return block_sandbox_policy_violation(violation, action.risk_category);
        }
        if action.needs_task_scope && !context.has_task_scope() {
            return block_missing_context(context, action.description, action.risk_category);
        }
        if action.requires_confirmation || (sandbox_bypass_requested && action.high_risk) {
            return require_confirmation(
                action
                    .risk_category
                    .unwrap_or(AegisSecretRiskCategory::Other),
                action.description,
            );
        }
        return allow(action.risk_category, action.description);
    }

    if sandbox_bypass_requested && !context.has_task_scope() {
        return block_missing_context(
            context,
            "sandbox bypass request",
            Some(AegisSecretRiskCategory::Other),
        );
    }

    if sandbox_bypass_requested && let Some(violation) = sandbox_policy_violation(context, true) {
        return block_sandbox_policy_violation(violation, Some(AegisSecretRiskCategory::Other));
    }

    allow(None, "No preflight gate matched")
}

fn evaluate_filesystem(
    paths: &[AbsolutePathBuf],
    change_count: usize,
    workspace_cwd: &AbsolutePathBuf,
    context: &ToolPreflightContext,
) -> ToolPreflightDecision {
    let out_of_scope = paths
        .iter()
        .any(|path| !path.as_path().starts_with(workspace_cwd.as_path()));
    if out_of_scope {
        return ToolPreflightDecision {
            verdict: AegisPreflightVerdict::Block,
            risk_category: Some(AegisSecretRiskCategory::DestructiveAction),
            reason: "Aegis preflight blocked filesystem write: target path is outside the current task workspace. Required evidence: evidence:task-scope.".to_string(),
            required_evidence_ids: vec!["evidence:task-scope".to_string()],
            sandbox_policy_violation: None,
        };
    }

    if change_count > 20 {
        if !context.has_task_scope() {
            return block_missing_context(
                context,
                "broad filesystem write",
                Some(AegisSecretRiskCategory::DestructiveAction),
            );
        }
        return require_confirmation(
            AegisSecretRiskCategory::DestructiveAction,
            "broad filesystem write",
        );
    }

    allow(
        Some(AegisSecretRiskCategory::Other),
        "Filesystem write is scoped to the current workspace",
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CommandAction {
    description: &'static str,
    risk_category: Option<AegisSecretRiskCategory>,
    needs_task_scope: bool,
    requires_confirmation: bool,
    high_risk: bool,
}

impl CommandAction {
    fn is_protected(&self) -> bool {
        self.high_risk || self.needs_task_scope || self.requires_confirmation
    }
}

fn sandbox_policy_violation(
    context: &ToolPreflightContext,
    sandbox_bypass_requested: bool,
) -> Option<SandboxPolicyViolation> {
    context.sandbox_policy.as_ref().and_then(|sandbox_policy| {
        evaluate_sandbox_policy(sandbox_policy, sandbox_bypass_requested)
    })
}

fn classify_command(command: &[String]) -> Option<CommandAction> {
    for plain in plain_commands(command) {
        if let Some(action) = classify_plain_command(&plain) {
            return Some(action);
        }
    }
    None
}

fn plain_commands(command: &[String]) -> Vec<Vec<String>> {
    if let Some(commands) = parse_shell_lc_plain_commands(command)
        && !commands.is_empty()
    {
        return commands;
    }

    if let Some(single_command) = parse_shell_lc_single_command_prefix(command) {
        return vec![single_command];
    }

    vec![command.to_vec()]
}

fn classify_plain_command(command: &[String]) -> Option<CommandAction> {
    let program = command.first().and_then(|arg| command_basename(arg))?;
    let args = &command[1..];

    match program.as_str() {
        "gh" => classify_gh(args),
        "git" => classify_git(args),
        "aws" | "gcloud" | "kubectl" | "terraform" => classify_cloud_or_infra(&program, args),
        "rm" | "chmod" | "chown" => classify_destructive_local(&program, args),
        _ => match analyze_sensitive_command(command) {
            SensitiveCommandAnalysis::Reject(_) => Some(CommandAction {
                description: "unmediated sensitive command wrapper",
                risk_category: Some(AegisSecretRiskCategory::SensitiveCommand),
                needs_task_scope: true,
                requires_confirmation: false,
                high_risk: true,
            }),
            SensitiveCommandAnalysis::Single(_) | SensitiveCommandAnalysis::NotSensitive => None,
        },
    }
}

fn classify_gh(args: &[String]) -> Option<CommandAction> {
    let first = args.first()?.as_str();
    let second = args.get(1).map(String::as_str);
    let mutating = matches!(
        (first, second),
        ("pr", Some("merge"))
            | ("pr", Some("close"))
            | ("issue", Some("close"))
            | ("issue", Some("delete"))
            | ("release", _)
            | ("label", _)
            | ("milestone", _)
    );
    mutating.then_some(CommandAction {
        description: "GitHub state mutation",
        risk_category: Some(AegisSecretRiskCategory::RepositoryMutation),
        needs_task_scope: true,
        requires_confirmation: false,
        high_risk: true,
    })
}

fn classify_git(args: &[String]) -> Option<CommandAction> {
    let first = args.first()?.as_str();
    let destructive = matches!(first, "reset" | "clean" | "push")
        && (args.iter().any(|arg| {
            matches!(
                arg.as_str(),
                "--hard" | "-f" | "--force" | "--force-with-lease" | "-fd" | "-xfd"
            )
        }) || first == "clean");

    destructive.then_some(CommandAction {
        description: "destructive git operation",
        risk_category: Some(AegisSecretRiskCategory::RepositoryMutation),
        needs_task_scope: true,
        requires_confirmation: true,
        high_risk: true,
    })
}

fn classify_cloud_or_infra(program: &str, args: &[String]) -> Option<CommandAction> {
    let read_only = match program {
        "terraform" => args.first().is_some_and(|arg| {
            matches!(
                arg.as_str(),
                "plan" | "validate" | "fmt" | "show" | "output" | "providers" | "version"
            )
        }),
        "kubectl" => args.first().is_some_and(|arg| {
            matches!(
                arg.as_str(),
                "get" | "describe" | "logs" | "version" | "config"
            )
        }),
        "aws" => args
            .iter()
            .any(|arg| matches!(arg.as_str(), "help" | "list" | "ls" | "describe" | "get")),
        "gcloud" => args
            .iter()
            .any(|arg| matches!(arg.as_str(), "help" | "list" | "describe" | "get")),
        _ => false,
    };

    if read_only {
        return None;
    }

    Some(CommandAction {
        description: "cloud or infrastructure mutation",
        risk_category: Some(AegisSecretRiskCategory::CloudMutation),
        needs_task_scope: true,
        requires_confirmation: false,
        high_risk: true,
    })
}

fn classify_destructive_local(program: &str, args: &[String]) -> Option<CommandAction> {
    let destructive = match program {
        "rm" => args.iter().any(|arg| {
            let arg = arg.as_str();
            arg == "-rf" || arg == "-fr" || arg.contains('r') && arg.contains('f')
        }),
        "chmod" | "chown" => args.iter().any(|arg| arg == "-R" || arg.starts_with("-R")),
        _ => false,
    };

    destructive.then_some(CommandAction {
        description: "destructive local filesystem command",
        risk_category: Some(AegisSecretRiskCategory::DestructiveAction),
        needs_task_scope: true,
        requires_confirmation: true,
        high_risk: true,
    })
}

fn block_missing_context(
    context: &ToolPreflightContext,
    action: &str,
    risk_category: Option<AegisSecretRiskCategory>,
) -> ToolPreflightDecision {
    ToolPreflightDecision {
        verdict: AegisPreflightVerdict::Block,
        risk_category,
        reason: context.missing_context_reason(action),
        required_evidence_ids: vec!["evidence:task-scope".to_string()],
        sandbox_policy_violation: None,
    }
}

fn block_sandbox_policy_violation(
    violation: SandboxPolicyViolation,
    risk_category: Option<AegisSecretRiskCategory>,
) -> ToolPreflightDecision {
    ToolPreflightDecision {
        verdict: AegisPreflightVerdict::Block,
        risk_category,
        reason: violation.reason.clone(),
        required_evidence_ids: vec!["evidence:sandbox-policy".to_string()],
        sandbox_policy_violation: Some(violation),
    }
}

fn require_confirmation(
    risk_category: AegisSecretRiskCategory,
    action: &str,
) -> ToolPreflightDecision {
    ToolPreflightDecision {
        verdict: AegisPreflightVerdict::RequireConfirmation,
        risk_category: Some(risk_category),
        reason: format!("Aegis preflight requires confirmation for {action}."),
        required_evidence_ids: vec!["evidence:user-confirmation".to_string()],
        sandbox_policy_violation: None,
    }
}

fn allow(risk_category: Option<AegisSecretRiskCategory>, reason: &str) -> ToolPreflightDecision {
    ToolPreflightDecision {
        verdict: AegisPreflightVerdict::Allow,
        risk_category,
        reason: reason.to_string(),
        required_evidence_ids: Vec::new(),
        sandbox_policy_violation: None,
    }
}

fn command_basename(command: &str) -> Option<String> {
    Path::new(command)
        .file_name()
        .and_then(|name| name.to_str())
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_config::SandboxModeRequirement;

    fn command(command: &[&str], sandbox_bypass_requested: bool) -> ToolPreflightSpec {
        ToolPreflightSpec {
            subject: ToolPreflightSubject::Command {
                command: command.iter().map(|arg| arg.to_string()).collect(),
                cwd: AbsolutePathBuf::try_from("/repo").unwrap(),
            },
            sandbox_bypass_requested,
        }
    }

    fn missing_context() -> ToolPreflightContext {
        ToolPreflightContext {
            method_state_available: false,
            method_state_valid: false,
            linked_issue_available: false,
            sandbox_policy: None,
        }
    }

    fn sandbox_context(
        active_mode: Option<SandboxModeRequirement>,
        allowed_modes: Vec<SandboxModeRequirement>,
    ) -> ToolPreflightContext {
        ToolPreflightContext {
            sandbox_policy: Some(SandboxPolicyContext {
                active_mode,
                allowed_modes,
                source: None,
            }),
            ..ToolPreflightContext::valid_with_issue()
        }
    }

    #[test]
    fn github_mutation_without_context_is_blocked() {
        let decision = evaluate_preflight(
            &command(&["gh", "pr", "merge", "123"], false),
            &missing_context(),
            &AbsolutePathBuf::try_from("/repo").unwrap(),
        );

        assert_eq!(decision.verdict, AegisPreflightVerdict::Block);
        assert!(decision.reason.contains("linked task issue"));
    }

    #[test]
    fn cloud_read_only_command_is_allowed() {
        let decision = evaluate_preflight(
            &command(&["kubectl", "get", "pods"], false),
            &missing_context(),
            &AbsolutePathBuf::try_from("/repo").unwrap(),
        );

        assert_eq!(decision.verdict, AegisPreflightVerdict::Allow);
    }

    #[test]
    fn cloud_mutation_without_context_is_blocked() {
        let decision = evaluate_preflight(
            &command(&["terraform", "apply"], false),
            &missing_context(),
            &AbsolutePathBuf::try_from("/repo").unwrap(),
        );

        assert_eq!(decision.verdict, AegisPreflightVerdict::Block);
        assert_eq!(
            decision.required_evidence_ids,
            vec!["evidence:task-scope".to_string()]
        );
    }

    #[test]
    fn shell_wrapped_github_mutation_without_context_is_blocked() {
        let decision = evaluate_preflight(
            &command(&["bash", "-lc", "gh pr merge 123"], false),
            &missing_context(),
            &AbsolutePathBuf::try_from("/repo").unwrap(),
        );

        assert_eq!(decision.verdict, AegisPreflightVerdict::Block);
        assert_eq!(
            decision.risk_category,
            Some(AegisSecretRiskCategory::RepositoryMutation)
        );
    }

    #[test]
    fn destructive_local_command_with_context_requires_confirmation() {
        let decision = evaluate_preflight(
            &command(&["rm", "-rf", "target"], false),
            &ToolPreflightContext::valid_with_issue(),
            &AbsolutePathBuf::try_from("/repo").unwrap(),
        );

        assert_eq!(decision.verdict, AegisPreflightVerdict::RequireConfirmation);
    }

    #[test]
    fn destructive_git_command_with_context_requires_confirmation() {
        let decision = evaluate_preflight(
            &command(&["git", "reset", "--hard"], false),
            &ToolPreflightContext::valid_with_issue(),
            &AbsolutePathBuf::try_from("/repo").unwrap(),
        );

        assert_eq!(decision.verdict, AegisPreflightVerdict::RequireConfirmation);
    }

    #[test]
    fn out_of_scope_filesystem_write_is_blocked() {
        let spec = ToolPreflightSpec {
            subject: ToolPreflightSubject::FileSystemWrite {
                cwd: AbsolutePathBuf::try_from("/repo").unwrap(),
                paths: vec![AbsolutePathBuf::try_from("/tmp/outside.txt").unwrap()],
                change_count: 1,
            },
            sandbox_bypass_requested: false,
        };
        let decision = evaluate_preflight(
            &spec,
            &ToolPreflightContext::valid_with_issue(),
            &AbsolutePathBuf::try_from("/repo").unwrap(),
        );

        assert_eq!(decision.verdict, AegisPreflightVerdict::Block);
        assert!(
            decision
                .reason
                .contains("outside the current task workspace")
        );
    }

    #[test]
    fn sandbox_bypass_without_context_is_blocked() {
        let decision = evaluate_preflight(
            &command(&["cargo", "test"], true),
            &missing_context(),
            &AbsolutePathBuf::try_from("/repo").unwrap(),
        );

        assert_eq!(decision.verdict, AegisPreflightVerdict::Block);
        assert!(decision.reason.contains("sandbox bypass request"));
    }

    #[test]
    fn protected_command_with_allowed_sandbox_posture_is_allowed() {
        let decision = evaluate_preflight(
            &command(&["gh", "pr", "merge", "123"], false),
            &sandbox_context(
                Some(SandboxModeRequirement::WorkspaceWrite),
                vec![
                    SandboxModeRequirement::ReadOnly,
                    SandboxModeRequirement::WorkspaceWrite,
                ],
            ),
            &AbsolutePathBuf::try_from("/repo").unwrap(),
        );

        assert_eq!(decision.verdict, AegisPreflightVerdict::Allow);
    }

    #[test]
    fn protected_command_with_blocked_sandbox_posture_is_blocked() {
        let decision = evaluate_preflight(
            &command(&["gh", "pr", "merge", "123"], false),
            &sandbox_context(
                Some(SandboxModeRequirement::DangerFullAccess),
                vec![
                    SandboxModeRequirement::ReadOnly,
                    SandboxModeRequirement::WorkspaceWrite,
                ],
            ),
            &AbsolutePathBuf::try_from("/repo").unwrap(),
        );

        assert_eq!(decision.verdict, AegisPreflightVerdict::Block);
        assert_eq!(
            decision.required_evidence_ids,
            vec!["evidence:sandbox-policy".to_string()]
        );
        assert!(decision.sandbox_policy_violation.is_some());
    }

    #[test]
    fn sandbox_override_is_blocked_when_policy_excludes_full_access() {
        let decision = evaluate_preflight(
            &command(&["cargo", "test"], true),
            &sandbox_context(
                Some(SandboxModeRequirement::WorkspaceWrite),
                vec![
                    SandboxModeRequirement::ReadOnly,
                    SandboxModeRequirement::WorkspaceWrite,
                ],
            ),
            &AbsolutePathBuf::try_from("/repo").unwrap(),
        );

        assert_eq!(decision.verdict, AegisPreflightVerdict::Block);
        assert!(decision.reason.contains("sandbox override"));
    }

    #[test]
    fn missing_sandbox_posture_blocks_protected_command() {
        let decision = evaluate_preflight(
            &command(&["gh", "pr", "merge", "123"], false),
            &sandbox_context(
                None,
                vec![
                    SandboxModeRequirement::ReadOnly,
                    SandboxModeRequirement::WorkspaceWrite,
                ],
            ),
            &AbsolutePathBuf::try_from("/repo").unwrap(),
        );

        assert_eq!(decision.verdict, AegisPreflightVerdict::Block);
        assert!(decision.reason.contains("sandbox posture is missing"));
    }

    #[test]
    fn preflight_event_redacts_sensitive_command_arguments() {
        let spec = command(
            &[
                "gh",
                "api",
                "--header",
                "authorization: bearer secret-token",
                "--token=abc",
                "OPENAI_API_KEY=sk-redaction-test",
                "--password",
                "pw",
            ],
            false,
        );
        let decision = allow(None, "allowed");
        let event = event_for_decision("call", "turn", "shell", &spec, &decision);

        assert_eq!(
            event.command,
            Some(vec![
                "gh".to_string(),
                "api".to_string(),
                "--header".to_string(),
                "<redacted>".to_string(),
                "--token=<redacted>".to_string(),
                "OPENAI_API_KEY=<redacted>".to_string(),
                "--password".to_string(),
                "<redacted>".to_string(),
            ])
        );
    }
}
