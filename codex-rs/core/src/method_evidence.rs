use std::collections::BTreeSet;
use std::path::Path;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use codex_git_utils::collect_git_info;
use codex_git_utils::get_git_repo_root;
use codex_protocol::exec_output::ExecToolCallOutput;
use codex_protocol::method_state::METHOD_EVIDENCE_RECEIPT_SCHEMA_VERSION;
use codex_protocol::method_state::MethodEvidence;
use codex_protocol::method_state::MethodEvidenceExitStatus;
use codex_protocol::method_state::MethodEvidenceGitState;
use codex_protocol::method_state::MethodEvidenceGitStateStatus;
use codex_protocol::method_state::MethodEvidenceKind;
use codex_protocol::method_state::MethodEvidenceReceipt;
use codex_protocol::method_state::MethodEvidenceRedactionStatus;
use codex_protocol::method_state::MethodEvidenceRequirement;
use codex_protocol::method_state::MethodEvidenceSessionMetadata;
use codex_protocol::method_state::merge_method_evidence_redaction_status;
use codex_protocol::method_state::redact_method_evidence_command;
use codex_protocol::method_state::redact_method_evidence_output;
use codex_shell_command::bash::parse_shell_lc_plain_commands;
use codex_shell_command::parse_command::extract_shell_command;
use codex_shell_command::parse_command::shlex_join;
use codex_utils_absolute_path::AbsolutePathBuf;
use tokio::process::Command;
use tokio::time::timeout;

use crate::context_packs::EvidenceRequirementInspection;

const OUTPUT_SUMMARY_MAX_BYTES: usize = 2 * 1024;
const GIT_COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EvidenceCommandCategory {
    Test,
    Build,
    Lint,
    Typecheck,
    FormatCheck,
    StaticAnalysis,
    ProjectCommand,
}

impl EvidenceCommandCategory {
    fn evidence_kind(self) -> MethodEvidenceKind {
        match self {
            Self::Test => MethodEvidenceKind::Test,
            Self::Build
            | Self::Lint
            | Self::Typecheck
            | Self::FormatCheck
            | Self::StaticAnalysis
            | Self::ProjectCommand => MethodEvidenceKind::Command,
        }
    }

    fn id_fragment(self) -> &'static str {
        match self {
            Self::Test => "test",
            Self::Build => "build",
            Self::Lint => "lint",
            Self::Typecheck => "typecheck",
            Self::FormatCheck => "format-check",
            Self::StaticAnalysis => "static-analysis",
            Self::ProjectCommand => "project-command",
        }
    }

    fn display(self) -> &'static str {
        match self {
            Self::Test => "test",
            Self::Build => "build",
            Self::Lint => "lint",
            Self::Typecheck => "typecheck",
            Self::FormatCheck => "format-check",
            Self::StaticAnalysis => "static-analysis",
            Self::ProjectCommand => "project",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EvidenceSessionSnapshot {
    pub(crate) session_id: Option<String>,
    pub(crate) thread_id: Option<String>,
    pub(crate) provider: Option<String>,
    pub(crate) model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EvidenceCommandMatch {
    category: EvidenceCommandCategory,
    requirement_ids: Vec<String>,
    claim_ids: Vec<String>,
    falsifier_ids: Vec<String>,
}

pub(crate) async fn build_method_evidence_for_command(
    call_id: &str,
    command: &[String],
    cwd: &AbsolutePathBuf,
    output: &ExecToolCallOutput,
    method_requirements: &[MethodEvidenceRequirement],
    context_pack_requirements: &[EvidenceRequirementInspection],
    session: EvidenceSessionSnapshot,
) -> Option<MethodEvidence> {
    let matched =
        match_configured_command(command, method_requirements, context_pack_requirements)?;
    let captured_at_unix_seconds = now_unix_seconds();
    let (receipt, redaction_status) =
        build_receipt(command, cwd, output, captured_at_unix_seconds, session).await;
    let exit = receipt.exit_status.exit_code.map_or_else(
        || "without exit code".to_string(),
        |code| format!("with exit code {code}"),
    );

    Some(MethodEvidence {
        id: format!(
            "evidence:{}:{}",
            matched.category.id_fragment(),
            sanitize_id_fragment(call_id)
        ),
        summary: format!(
            "{} command completed {exit}: {}",
            matched.category.display(),
            shlex_join(command)
        ),
        kind: matched.category.evidence_kind(),
        requirement_ids: matched.requirement_ids,
        claim_ids: matched.claim_ids,
        falsifier_ids: matched.falsifier_ids,
        source: Some("harness exec_command".to_string()),
        captured_at_unix_seconds,
        receipt: Some(MethodEvidenceReceipt {
            redaction_status,
            ..receipt
        }),
    })
}

fn match_configured_command(
    command: &[String],
    method_requirements: &[MethodEvidenceRequirement],
    context_pack_requirements: &[EvidenceRequirementInspection],
) -> Option<EvidenceCommandMatch> {
    let candidates = command_candidates(command);
    if candidates.is_empty() {
        return None;
    }

    let mut requirement_ids = BTreeSet::new();
    let mut claim_ids = BTreeSet::new();
    let mut falsifier_ids = BTreeSet::new();

    for requirement in method_requirements {
        if requirement
            .commands
            .iter()
            .any(|configured| candidates.contains(configured))
        {
            requirement_ids.insert(requirement.id.clone());
            claim_ids.extend(requirement.claim_ids.iter().cloned());
            falsifier_ids.extend(requirement.falsifier_ids.iter().cloned());
        }
    }

    for requirement in context_pack_requirements {
        if requirement
            .commands
            .iter()
            .any(|configured| candidates.contains(configured))
        {
            requirement_ids.insert(requirement.id.clone());
            if let Some(method_requirement) = method_requirements
                .iter()
                .find(|method_requirement| method_requirement.id == requirement.id)
            {
                claim_ids.extend(method_requirement.claim_ids.iter().cloned());
                falsifier_ids.extend(method_requirement.falsifier_ids.iter().cloned());
            }
        }
    }

    if requirement_ids.is_empty() {
        return None;
    }

    Some(EvidenceCommandMatch {
        category: classify_command(command).unwrap_or(EvidenceCommandCategory::ProjectCommand),
        requirement_ids: requirement_ids.into_iter().collect(),
        claim_ids: claim_ids.into_iter().collect(),
        falsifier_ids: falsifier_ids.into_iter().collect(),
    })
}

fn command_candidates(command: &[String]) -> BTreeSet<String> {
    let mut candidates = BTreeSet::new();
    if command.is_empty() {
        return candidates;
    }

    candidates.insert(shlex_join(command));
    if let Some((_shell, script)) = extract_shell_command(command) {
        candidates.insert(script.to_string());
    }
    if let Some(commands) = parse_shell_lc_plain_commands(command)
        && commands.len() == 1
    {
        candidates.insert(shlex_join(&commands[0]));
    }

    candidates
}

fn classify_command(command: &[String]) -> Option<EvidenceCommandCategory> {
    let tokens = normalized_single_command_tokens(command)?;
    let cmd = tokens.first()?.as_str();
    let args = &tokens[1..];

    match cmd {
        "cargo" if args.first().is_some_and(|arg| arg == "test") => {
            Some(EvidenceCommandCategory::Test)
        }
        "cargo" if args.first().is_some_and(|arg| arg == "build") => {
            Some(EvidenceCommandCategory::Build)
        }
        "cargo" if args.first().is_some_and(|arg| arg == "clippy") => {
            Some(EvidenceCommandCategory::Lint)
        }
        "cargo" if args.first().is_some_and(|arg| arg == "fmt") && has_arg(args, "--check") => {
            Some(EvidenceCommandCategory::FormatCheck)
        }
        "cargo"
            if args
                .first()
                .is_some_and(|arg| arg == "audit" || arg == "deny") =>
        {
            Some(EvidenceCommandCategory::StaticAnalysis)
        }
        "npm" if args.first().is_some_and(|arg| arg == "test") => {
            Some(EvidenceCommandCategory::Test)
        }
        "npm" if args.first().is_some_and(|arg| arg == "audit") => {
            Some(EvidenceCommandCategory::StaticAnalysis)
        }
        "npm" if args.first().is_some_and(|arg| arg == "run") => {
            classify_package_script(args.get(1)?)
        }
        "pnpm" | "yarn" if args.first().is_some_and(|arg| arg == "run") => args
            .get(1)
            .and_then(|script| classify_package_script(script)),
        "pnpm" | "yarn" => args
            .first()
            .and_then(|script| classify_package_script(script)),
        "pytest" => Some(EvidenceCommandCategory::Test),
        "go" if args.first().is_some_and(|arg| arg == "test") => {
            Some(EvidenceCommandCategory::Test)
        }
        "go" if args.first().is_some_and(|arg| arg == "build") => {
            Some(EvidenceCommandCategory::Build)
        }
        "tsc" => Some(EvidenceCommandCategory::Typecheck),
        "mypy" => Some(EvidenceCommandCategory::Typecheck),
        "ruff" if args.first().is_some_and(|arg| arg == "format") && has_arg(args, "--check") => {
            Some(EvidenceCommandCategory::FormatCheck)
        }
        "eslint" | "ruff" => Some(EvidenceCommandCategory::Lint),
        "rustfmt" | "prettier" if has_arg(args, "--check") => {
            Some(EvidenceCommandCategory::FormatCheck)
        }
        "semgrep" => Some(EvidenceCommandCategory::StaticAnalysis),
        _ => None,
    }
}

fn classify_package_script(script: &str) -> Option<EvidenceCommandCategory> {
    if script == "test" || script.starts_with("test:") {
        Some(EvidenceCommandCategory::Test)
    } else if script == "build" || script.starts_with("build:") {
        Some(EvidenceCommandCategory::Build)
    } else if script == "lint" || script.starts_with("lint:") {
        Some(EvidenceCommandCategory::Lint)
    } else if script == "typecheck" || script.starts_with("typecheck:") {
        Some(EvidenceCommandCategory::Typecheck)
    } else if script == "format:check" || script == "fmt:check" {
        Some(EvidenceCommandCategory::FormatCheck)
    } else if script == "audit" || script == "scan" {
        Some(EvidenceCommandCategory::StaticAnalysis)
    } else {
        None
    }
}

fn normalized_single_command_tokens(command: &[String]) -> Option<Vec<String>> {
    if let Some(commands) = parse_shell_lc_plain_commands(command) {
        if commands.len() == 1 {
            return commands.into_iter().next();
        }
        return None;
    }
    Some(command.to_vec())
}

fn has_arg(args: &[String], needle: &str) -> bool {
    args.iter().any(|arg| arg == needle)
}

async fn build_receipt(
    command: &[String],
    cwd: &AbsolutePathBuf,
    output: &ExecToolCallOutput,
    captured_at_unix_seconds: i64,
    session: EvidenceSessionSnapshot,
) -> (MethodEvidenceReceipt, MethodEvidenceRedactionStatus) {
    let (redacted_command, command_redaction_status) = redact_method_evidence_command(command);
    let output_summary = truncate_output_summary(&output.aggregated_output.text);
    let (redacted_output, output_redaction_status) = redact_method_evidence_output(&output_summary);
    let redaction_status =
        merge_method_evidence_redaction_status(command_redaction_status, output_redaction_status);

    (
        MethodEvidenceReceipt {
            schema_version: METHOD_EVIDENCE_RECEIPT_SCHEMA_VERSION,
            command: redacted_command,
            cwd: cwd.as_path().to_string_lossy().into_owned(),
            captured_at_unix_seconds,
            git_state: capture_git_state(cwd.as_path()).await,
            exit_status: MethodEvidenceExitStatus {
                exit_code: Some(output.exit_code),
                timed_out: output.timed_out,
            },
            output_summary: redacted_output,
            artifacts: Vec::new(),
            session: MethodEvidenceSessionMetadata {
                session_id: session.session_id,
                thread_id: session.thread_id,
                provider: session.provider,
                model: session.model,
            },
            redaction_status,
        },
        redaction_status,
    )
}

async fn capture_git_state(cwd: &Path) -> MethodEvidenceGitState {
    let Some(git_info) = collect_git_info(cwd).await else {
        return MethodEvidenceGitState {
            status: MethodEvidenceGitStateStatus::Unavailable,
            repository: None,
            branch: None,
            commit: None,
            dirty: None,
            unavailable_reason: Some(
                "not a git repository or git metadata unavailable".to_string(),
            ),
        };
    };

    let dirty = git_dirty(cwd).await;
    MethodEvidenceGitState {
        status: MethodEvidenceGitStateStatus::Captured,
        repository: git_info
            .repository_url
            .or_else(|| get_git_repo_root(cwd).map(|root| root.display().to_string())),
        branch: git_info.branch,
        commit: git_info.commit_hash.map(|sha| sha.0),
        dirty,
        unavailable_reason: None,
    }
}

async fn git_dirty(cwd: &Path) -> Option<bool> {
    let output = timeout(
        GIT_COMMAND_TIMEOUT,
        Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(cwd)
            .output(),
    )
    .await
    .ok()?
    .ok()?;
    output.status.success().then_some(!output.stdout.is_empty())
}

fn truncate_output_summary(output: &str) -> String {
    if output.len() <= OUTPUT_SUMMARY_MAX_BYTES {
        return output.to_string();
    }

    let mut end = OUTPUT_SUMMARY_MAX_BYTES;
    while !output.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n[... evidence output truncated ...]", &output[..end])
}

fn sanitize_id_fragment(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

fn now_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use codex_protocol::exec_output::ExecToolCallOutput;
    use codex_protocol::exec_output::StreamOutput;

    use super::*;

    fn requirement() -> MethodEvidenceRequirement {
        MethodEvidenceRequirement {
            id: "requirement:ci".to_string(),
            summary: "Run local CI".to_string(),
            required: true,
            commands: vec!["./scripts/ci_local.sh".to_string()],
            claim_ids: vec!["claim:verified".to_string()],
            falsifier_ids: vec!["falsifier:ci-fails".to_string()],
        }
    }

    #[test]
    fn configured_direct_command_matches_requirement_links() {
        let matched = match_configured_command(
            &["./scripts/ci_local.sh".to_string()],
            &[requirement()],
            &[],
        )
        .expect("match");

        assert_eq!(matched.category, EvidenceCommandCategory::ProjectCommand);
        assert_eq!(matched.requirement_ids, vec!["requirement:ci"]);
        assert_eq!(matched.claim_ids, vec!["claim:verified"]);
        assert_eq!(matched.falsifier_ids, vec!["falsifier:ci-fails"]);
    }

    #[test]
    fn configured_shell_wrapper_matches_single_inner_command() {
        let matched = match_configured_command(
            &[
                "zsh".to_string(),
                "-lc".to_string(),
                "./scripts/ci_local.sh".to_string(),
            ],
            &[requirement()],
            &[],
        )
        .expect("match");

        assert_eq!(matched.requirement_ids, vec!["requirement:ci"]);
    }

    #[test]
    fn unconfigured_command_does_not_match() {
        assert!(
            match_configured_command(&["cargo".to_string(), "test".to_string()], &[], &[])
                .is_none()
        );
    }

    #[test]
    fn context_pack_command_matches_and_inherits_method_links_by_id() {
        let pack_requirement = EvidenceRequirementInspection {
            id: "requirement:ci".to_string(),
            description: "Run local CI".to_string(),
            commands: vec!["./scripts/ci_local.sh".to_string()],
        };
        let method_requirement = MethodEvidenceRequirement {
            commands: Vec::new(),
            ..requirement()
        };

        let matched = match_configured_command(
            &["./scripts/ci_local.sh".to_string()],
            &[method_requirement],
            &[pack_requirement],
        )
        .expect("match");

        assert_eq!(matched.requirement_ids, vec!["requirement:ci"]);
        assert_eq!(matched.claim_ids, vec!["claim:verified"]);
        assert_eq!(matched.falsifier_ids, vec!["falsifier:ci-fails"]);
    }

    #[test]
    fn classifies_common_command_categories() {
        assert_eq!(
            classify_command(&["cargo".to_string(), "test".to_string()]),
            Some(EvidenceCommandCategory::Test)
        );
        assert_eq!(
            classify_command(&["npm".to_string(), "run".to_string(), "build".to_string()]),
            Some(EvidenceCommandCategory::Build)
        );
        assert_eq!(
            classify_command(&["pnpm".to_string(), "typecheck".to_string()]),
            Some(EvidenceCommandCategory::Typecheck)
        );
        assert_eq!(
            classify_command(&["yarn".to_string(), "run".to_string(), "lint".to_string()]),
            Some(EvidenceCommandCategory::Lint)
        );
        assert_eq!(
            classify_command(&[
                "cargo".to_string(),
                "fmt".to_string(),
                "--".to_string(),
                "--check".to_string()
            ]),
            Some(EvidenceCommandCategory::FormatCheck)
        );
        assert_eq!(
            classify_command(&[
                "ruff".to_string(),
                "format".to_string(),
                "--check".to_string()
            ]),
            Some(EvidenceCommandCategory::FormatCheck)
        );
        assert_eq!(
            classify_command(&["semgrep".to_string(), "scan".to_string()]),
            Some(EvidenceCommandCategory::StaticAnalysis)
        );
    }

    #[tokio::test]
    async fn builds_failed_receipt_for_configured_command() {
        let output = ExecToolCallOutput {
            exit_code: 101,
            stdout: StreamOutput::new(String::new()),
            stderr: StreamOutput::new("test failed token secret".to_string()),
            aggregated_output: StreamOutput::new("test failed token secret".to_string()),
            duration: Duration::from_millis(20),
            timed_out: false,
        };

        let evidence = build_method_evidence_for_command(
            "call-1",
            &["./scripts/ci_local.sh".to_string()],
            &AbsolutePathBuf::try_from("/tmp").expect("absolute path"),
            &output,
            &[requirement()],
            &[],
            EvidenceSessionSnapshot {
                session_id: Some("session-1".to_string()),
                thread_id: Some("thread-1".to_string()),
                provider: Some("test-provider".to_string()),
                model: Some("test-model".to_string()),
            },
        )
        .await
        .expect("evidence");

        let receipt = evidence.receipt.as_ref().expect("receipt");
        assert_eq!(receipt.exit_status.exit_code, Some(101));
        assert_eq!(
            evidence.falsifier_ids,
            vec!["falsifier:ci-fails".to_string()]
        );
        assert_eq!(
            receipt.redaction_status,
            MethodEvidenceRedactionStatus::Redacted
        );
        assert!(!evidence.has_successful_receipt());
    }
}
