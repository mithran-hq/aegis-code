use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use codex_core::config::Config;
use codex_core::context_packs::ContextPackDiagnostic;
use codex_core::context_packs::ContextPackDiagnosticStatus;
use codex_core::context_packs::ContextPackInspection;
use codex_core::context_packs::ContextPackKind;
use codex_core::context_packs::PromotionStatus;
use codex_core::context_packs::inspect_context_pack_path;
use codex_core::doctor::DoctorReport;
use codex_core::doctor::build_doctor_report;
use codex_core::issue_train;
use codex_core::tool_preflight;
use codex_protocol::method_state::MethodEvidence;
use codex_protocol::method_state::MethodEvidenceReceipt;
use codex_protocol::method_state::MethodEvidenceRedactionStatus;
use codex_protocol::method_state::MethodFalsifierStatus;
use codex_protocol::method_state::MethodReviewFindingStatus;
use codex_protocol::method_state::MethodReviewSeverity;
use codex_protocol::method_state::MethodState;
use codex_protocol::method_state::MethodWorkStatus;
use codex_protocol::method_state::merge_method_evidence_redaction_status;
use codex_protocol::method_state::redact_method_evidence_command;
use codex_protocol::method_state::redact_method_evidence_output;
use codex_utils_absolute_path::AbsolutePathBuf;
use rmcp::model::CallToolResult;
use rmcp::model::Content;
use rmcp::model::JsonObject;
use rmcp::model::Tool;
use schemars::JsonSchema;
use schemars::r#gen::SchemaSettings;
use serde::Deserialize;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

pub(crate) const AEGIS_STATUS_TOOL: &str = "aegis_status";
pub(crate) const AEGIS_CHECK_TOOL: &str = "aegis_check";
pub(crate) const AEGIS_EVIDENCE_TOOL: &str = "aegis_evidence";
pub(crate) const AEGIS_REVIEW_TOOL: &str = "aegis_review";
pub(crate) const AEGIS_CONTEXT_PACK_LIST_TOOL: &str = "aegis_context_pack_list";
pub(crate) const AEGIS_CONTEXT_PACK_INSPECT_TOOL: &str = "aegis_context_pack_inspect";
pub(crate) const AEGIS_POLICY_EXPLAIN_TOOL: &str = "aegis_policy_explain";
pub(crate) const AEGIS_ISSUE_VALIDATE_TOOL: &str = "aegis_issue_validate";
pub(crate) const AEGIS_DOCTOR_TOOL: &str = "aegis_doctor";

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EmptyInput {}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MethodStateInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    method_state: Option<MethodState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    method_state_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ContextPackListInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    kind: Option<ContextPackKindInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    status: Option<ContextPackStatusInput>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum ContextPackKindInput {
    User,
    Project,
    Learned,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum ContextPackStatusInput {
    Candidate,
    Promoted,
    Retired,
    Invalid,
    Unreadable,
    All,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ContextPackInspectInput {
    path: String,
    #[serde(default)]
    include_guidance: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PolicyExplainInput {
    subject: PolicySubjectInput,
    #[serde(default)]
    sandbox_bypass_requested: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    method_state: Option<MethodState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    tag = "type"
)]
enum PolicySubjectInput {
    Command {
        command: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
    },
    FilesystemWrite {
        paths: Vec<String>,
        #[serde(rename = "changeCount")]
        change_count: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct IssueValidateInput {
    parent: IssueSnapshotInput,
    #[serde(default)]
    children: Vec<IssueSnapshotInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct IssueSnapshotInput {
    number: u64,
    title: String,
    state: IssueStateInput,
    body: String,
    #[serde(default)]
    labels: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum IssueStateInput {
    Open,
    Closed,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct AegisStatusOutput {
    ok: bool,
    advisory_only: bool,
    version: String,
    cwd: String,
    codex_home: String,
    config_path: String,
    provider: ProviderOutput,
    context_packs: ContextPackSummaryOutput,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct ProviderOutput {
    id: String,
    name: String,
    model: String,
    wire_api: String,
    env_key: Option<String>,
    env_key_present: Option<bool>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct ContextPackSummaryOutput {
    total: usize,
    active: usize,
    candidate: usize,
    promoted: usize,
    retired: usize,
    invalid: usize,
    unreadable: usize,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct AegisFinding {
    severity: FindingSeverityOutput,
    code: String,
    message: String,
    remediation: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum FindingSeverityOutput {
    Error,
    Warning,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct AegisCheckOutput {
    ok: bool,
    advisory_only: bool,
    method_status: String,
    closure_valid: bool,
    required_evidence_gaps: usize,
    open_falsifiers: usize,
    open_blocking_review_findings: usize,
    findings: Vec<AegisFinding>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct AegisEvidenceOutput {
    ok: bool,
    advisory_only: bool,
    requirements: Vec<EvidenceRequirementOutput>,
    evidence: Vec<EvidenceOutput>,
    closure_gaps: Vec<EvidenceRequirementOutput>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct EvidenceRequirementOutput {
    id: String,
    summary: String,
    required: bool,
    commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct EvidenceOutput {
    id: String,
    summary: String,
    kind: String,
    requirement_ids: Vec<String>,
    successful_receipt: bool,
    receipt: Option<EvidenceReceiptOutput>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct EvidenceReceiptOutput {
    command: Vec<String>,
    cwd: String,
    exit_code: Option<i32>,
    timed_out: bool,
    output_summary: String,
    redaction_status: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct AegisReviewOutput {
    ok: bool,
    advisory_only: bool,
    findings: Vec<ReviewFindingOutput>,
    open_blocking_findings: usize,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct ReviewFindingOutput {
    id: String,
    summary: String,
    severity: String,
    status: String,
    evidence_ids: Vec<String>,
    reviewed_at_unix_seconds: i64,
    reviewer: Option<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct ContextPackListOutput {
    ok: bool,
    advisory_only: bool,
    summary: ContextPackSummaryOutput,
    packs: Vec<ContextPackDiagnosticOutput>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct ContextPackDiagnosticOutput {
    path: String,
    pack_id: Option<String>,
    kind: Option<String>,
    schema_version: Option<u64>,
    promotion_status: Option<String>,
    status: String,
    active: bool,
    reason: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct ContextPackInspectOutput {
    ok: bool,
    advisory_only: bool,
    path: String,
    pack_id: String,
    kind: String,
    schema_version: u64,
    name: String,
    description: Option<String>,
    promotion_status: String,
    evidence_requirements: Vec<ContextPackEvidenceRequirementOutput>,
    provider_preferred: Option<String>,
    provider_fallbacks: Vec<String>,
    guidance_included: bool,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct ContextPackEvidenceRequirementOutput {
    id: String,
    description: String,
    commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct PolicyExplainOutput {
    ok: bool,
    advisory_only: bool,
    verdict: String,
    risk_category: Option<String>,
    reason: String,
    required_evidence_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct IssueValidateOutput {
    ok: bool,
    advisory_only: bool,
    valid: bool,
    parent_issue: u64,
    child_count: usize,
    findings: Vec<IssueFindingOutput>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct IssueFindingOutput {
    severity: String,
    code: String,
    issue_number: Option<u64>,
    message: String,
    remediation: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct DoctorOutput {
    ok: bool,
    advisory_only: bool,
    version: String,
    codex_home: String,
    cwd: String,
    config_path: String,
    provider: ProviderOutput,
    context_packs: Vec<ContextPackDiagnosticOutput>,
    aegis_engine_alerts: AegisEngineAlertsOutput,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct AegisEngineAlertsOutput {
    enabled: bool,
    alerts_path: String,
    candidate_inputs_path: String,
    malformed_count: usize,
    stale_count: usize,
    active_warning_count: usize,
    active_blocking_count: usize,
    last_read_error: Option<String>,
}

pub(crate) fn create_aegis_tools() -> Vec<Tool> {
    vec![
        create_tool::<EmptyInput, AegisStatusOutput>(
            AEGIS_STATUS_TOOL,
            "Aegis Status",
            "Return a redacted advisory status summary for Aegis Code.",
        ),
        create_tool::<MethodStateInput, AegisCheckOutput>(
            AEGIS_CHECK_TOOL,
            "Aegis Check",
            "Check supplied Aegis method state for closure, evidence, falsifier, and review gaps.",
        ),
        create_tool::<MethodStateInput, AegisEvidenceOutput>(
            AEGIS_EVIDENCE_TOOL,
            "Aegis Evidence",
            "Summarize redacted evidence requirements, receipts, and closure gaps from method state.",
        ),
        create_tool::<MethodStateInput, AegisReviewOutput>(
            AEGIS_REVIEW_TOOL,
            "Aegis Review",
            "Summarize persisted method review findings from method state.",
        ),
        create_tool::<ContextPackListInput, ContextPackListOutput>(
            AEGIS_CONTEXT_PACK_LIST_TOOL,
            "Aegis Context Pack List",
            "List configured context packs and diagnostics without exposing guidance text.",
        ),
        create_tool::<ContextPackInspectInput, ContextPackInspectOutput>(
            AEGIS_CONTEXT_PACK_INSPECT_TOOL,
            "Aegis Context Pack Inspect",
            "Inspect one context pack without exposing guidance text.",
        ),
        create_tool::<PolicyExplainInput, PolicyExplainOutput>(
            AEGIS_POLICY_EXPLAIN_TOOL,
            "Aegis Policy Explain",
            "Explain the advisory Aegis preflight decision for a command or filesystem write.",
        ),
        create_tool::<IssueValidateInput, IssueValidateOutput>(
            AEGIS_ISSUE_VALIDATE_TOOL,
            "Aegis Issue Validate",
            "Validate a supplied parent/child GitHub issue snapshot using Aegis issue-train rules.",
        ),
        create_tool::<EmptyInput, DoctorOutput>(
            AEGIS_DOCTOR_TOOL,
            "Aegis Doctor",
            "Return redacted Aegis Code doctor diagnostics.",
        ),
    ]
}

pub(crate) fn handle_aegis_tool_call(
    name: &str,
    arguments: Option<JsonObject>,
    config: &Config,
) -> Option<CallToolResult> {
    let result = match name {
        AEGIS_STATUS_TOOL => handle_status(arguments, config),
        AEGIS_CHECK_TOOL => handle_check(arguments),
        AEGIS_EVIDENCE_TOOL => handle_evidence(arguments),
        AEGIS_REVIEW_TOOL => handle_review(arguments),
        AEGIS_CONTEXT_PACK_LIST_TOOL => handle_context_pack_list(arguments, config),
        AEGIS_CONTEXT_PACK_INSPECT_TOOL => handle_context_pack_inspect(arguments, config),
        AEGIS_POLICY_EXPLAIN_TOOL => handle_policy_explain(arguments, config),
        AEGIS_ISSUE_VALIDATE_TOOL => handle_issue_validate(arguments),
        AEGIS_DOCTOR_TOOL => handle_doctor(arguments, config),
        _ => return None,
    };

    Some(match result {
        Ok(result) => result,
        Err(err) => error_result(err.to_string()),
    })
}

fn handle_status(arguments: Option<JsonObject>, config: &Config) -> anyhow::Result<CallToolResult> {
    let _input: EmptyInput = parse_args(arguments)?;
    let report = build_doctor_report(config);
    let provider = provider_output_from_doctor(&report);
    let output = AegisStatusOutput {
        ok: true,
        advisory_only: true,
        version: report.version,
        cwd: report.cwd,
        codex_home: report.codex_home,
        config_path: report.config_path,
        provider,
        context_packs: context_pack_summary(config.context_packs.diagnostics()),
    };
    Ok(success_result("Aegis status is available.", &output)?)
}

fn handle_check(arguments: Option<JsonObject>) -> anyhow::Result<CallToolResult> {
    let state = load_method_state(parse_args(arguments)?)?;
    let mut findings = Vec::new();

    if state.status == MethodWorkStatus::Closed && !state.is_closure_valid() {
        findings.push(AegisFinding {
            severity: FindingSeverityOutput::Error,
            code: "invalid_closure".to_string(),
            message: "Closed method state does not satisfy closure requirements.".to_string(),
            remediation: "Cite successful required evidence and review findings before closure."
                .to_string(),
        });
    }

    for gap in state.closure_evidence_gaps() {
        findings.push(AegisFinding {
            severity: FindingSeverityOutput::Error,
            code: "missing_required_evidence".to_string(),
            message: format!("Required evidence `{}` is not satisfied.", gap.id),
            remediation: "Run the required check and attach a successful redacted receipt."
                .to_string(),
        });
    }

    for falsifier in &state.falsifiers {
        match falsifier.status {
            MethodFalsifierStatus::Open => findings.push(AegisFinding {
                severity: FindingSeverityOutput::Error,
                code: "open_falsifier".to_string(),
                message: format!("Falsifier `{}` is still open.", falsifier.id),
                remediation: "Disprove or confirm the falsifier before closing work.".to_string(),
            }),
            MethodFalsifierStatus::Confirmed => findings.push(AegisFinding {
                severity: FindingSeverityOutput::Error,
                code: "confirmed_falsifier".to_string(),
                message: format!("Falsifier `{}` is confirmed.", falsifier.id),
                remediation: "Fix the issue or split/block the work instead of closing it."
                    .to_string(),
            }),
            MethodFalsifierStatus::Disproved => {}
        }
    }

    for finding in &state.review_findings {
        if finding.status != MethodReviewFindingStatus::Open {
            continue;
        }
        let severity = match finding.severity {
            MethodReviewSeverity::Blocking
            | MethodReviewSeverity::High
            | MethodReviewSeverity::Medium => FindingSeverityOutput::Error,
            MethodReviewSeverity::Low | MethodReviewSeverity::Info => {
                FindingSeverityOutput::Warning
            }
        };
        findings.push(AegisFinding {
            severity,
            code: "open_review_finding".to_string(),
            message: format!("Review finding `{}` is still open.", finding.id),
            remediation: "Address the finding or mark it as accepted risk before closure."
                .to_string(),
        });
    }

    let open_falsifiers = state
        .falsifiers
        .iter()
        .filter(|f| f.status == MethodFalsifierStatus::Open)
        .count();
    let open_blocking_review_findings = state
        .review_findings
        .iter()
        .filter(|f| f.status == MethodReviewFindingStatus::Open)
        .filter(|f| {
            matches!(
                f.severity,
                MethodReviewSeverity::Blocking
                    | MethodReviewSeverity::High
                    | MethodReviewSeverity::Medium
            )
        })
        .count();
    let closure_gaps = state.closure_evidence_gaps().len();
    let ok = !findings
        .iter()
        .any(|finding| finding.severity == FindingSeverityOutput::Error);
    let output = AegisCheckOutput {
        ok,
        advisory_only: true,
        method_status: json_string(&state.status),
        closure_valid: state.is_closure_valid(),
        required_evidence_gaps: closure_gaps,
        open_falsifiers,
        open_blocking_review_findings,
        findings,
    };

    Ok(success_result("Aegis check completed.", &output)?)
}

fn handle_evidence(arguments: Option<JsonObject>) -> anyhow::Result<CallToolResult> {
    let state = load_method_state(parse_args(arguments)?)?;
    let output = AegisEvidenceOutput {
        ok: true,
        advisory_only: true,
        requirements: state
            .evidence_requirements
            .iter()
            .map(evidence_requirement_output)
            .collect(),
        evidence: state.evidence.iter().map(evidence_output).collect(),
        closure_gaps: state
            .closure_evidence_gaps()
            .iter()
            .map(evidence_requirement_output)
            .collect(),
    };
    Ok(success_result(
        "Aegis evidence summary is available.",
        &output,
    )?)
}

fn handle_review(arguments: Option<JsonObject>) -> anyhow::Result<CallToolResult> {
    let state = load_method_state(parse_args(arguments)?)?;
    let findings = state
        .review_findings
        .iter()
        .map(|finding| ReviewFindingOutput {
            id: finding.id.clone(),
            summary: redact_text(&finding.summary),
            severity: json_string(&finding.severity),
            status: json_string(&finding.status),
            evidence_ids: finding.evidence_ids.clone(),
            reviewed_at_unix_seconds: finding.reviewed_at_unix_seconds,
            reviewer: finding
                .reviewer
                .as_ref()
                .map(|reviewer| redact_text(reviewer)),
        })
        .collect::<Vec<_>>();
    let open_blocking_findings = state
        .review_findings
        .iter()
        .filter(|finding| finding.status == MethodReviewFindingStatus::Open)
        .filter(|finding| {
            matches!(
                finding.severity,
                MethodReviewSeverity::Blocking
                    | MethodReviewSeverity::High
                    | MethodReviewSeverity::Medium
            )
        })
        .count();
    let output = AegisReviewOutput {
        ok: open_blocking_findings == 0,
        advisory_only: true,
        findings,
        open_blocking_findings,
    };
    Ok(success_result(
        "Aegis review summary is available.",
        &output,
    )?)
}

fn handle_context_pack_list(
    arguments: Option<JsonObject>,
    config: &Config,
) -> anyhow::Result<CallToolResult> {
    let input: ContextPackListInput = parse_args(arguments)?;
    let packs = config
        .context_packs
        .diagnostics()
        .iter()
        .filter(|diagnostic| context_pack_matches_kind(diagnostic, input.kind))
        .filter(|diagnostic| context_pack_matches_status(diagnostic, input.status))
        .map(context_pack_diagnostic_output)
        .collect::<Vec<_>>();
    let output = ContextPackListOutput {
        ok: true,
        advisory_only: true,
        summary: context_pack_summary(config.context_packs.diagnostics()),
        packs,
    };
    Ok(success_result(
        "Aegis context-pack diagnostics are available.",
        &output,
    )?)
}

fn handle_context_pack_inspect(
    arguments: Option<JsonObject>,
    config: &Config,
) -> anyhow::Result<CallToolResult> {
    let input: ContextPackInspectInput = parse_args(arguments)?;
    if input.include_guidance {
        anyhow::bail!("includeGuidance is not supported by the MCP advisory surface");
    }
    let path = resolve_absolute_path(&input.path, config)?;
    let inspection = inspect_context_pack_path(&path, false)?;
    let output = context_pack_inspection_output(inspection);
    Ok(success_result(
        "Aegis context-pack inspection is available.",
        &output,
    )?)
}

fn handle_policy_explain(
    arguments: Option<JsonObject>,
    config: &Config,
) -> anyhow::Result<CallToolResult> {
    let input: PolicyExplainInput = parse_args(arguments)?;
    let cwd = match &input.subject {
        PolicySubjectInput::Command { cwd, .. }
        | PolicySubjectInput::FilesystemWrite { cwd, .. } => cwd
            .as_deref()
            .map(|raw| resolve_absolute_path(raw, config))
            .transpose()?
            .unwrap_or_else(|| config.cwd.clone()),
    };
    let subject = match input.subject {
        PolicySubjectInput::Command { command, .. } => {
            if command.is_empty() {
                anyhow::bail!("command must contain at least one argument");
            }
            tool_preflight::ToolPreflightSubject::Command { command, cwd }
        }
        PolicySubjectInput::FilesystemWrite {
            paths,
            change_count,
            ..
        } => {
            let paths = paths
                .iter()
                .map(|path| resolve_absolute_path(path, config))
                .collect::<anyhow::Result<Vec<_>>>()?;
            tool_preflight::ToolPreflightSubject::FileSystemWrite {
                cwd,
                paths,
                change_count,
            }
        }
    };
    let context = input
        .method_state
        .as_ref()
        .map(tool_preflight_context_from_method_state)
        .unwrap_or(tool_preflight::ToolPreflightContext {
            method_state_available: false,
            method_state_valid: false,
            linked_issue_available: false,
        });
    let spec = tool_preflight::ToolPreflightSpec {
        subject,
        sandbox_bypass_requested: input.sandbox_bypass_requested,
    };
    let decision = tool_preflight::evaluate_preflight(&spec, &context, &config.cwd);
    let output = PolicyExplainOutput {
        ok: true,
        advisory_only: true,
        verdict: json_string(&decision.verdict),
        risk_category: decision
            .risk_category
            .as_ref()
            .map(|risk| json_string(risk)),
        reason: redact_text(&decision.reason),
        required_evidence_ids: decision.required_evidence_ids,
    };
    Ok(success_result(
        "Aegis policy explanation is available.",
        &output,
    )?)
}

fn handle_issue_validate(arguments: Option<JsonObject>) -> anyhow::Result<CallToolResult> {
    let input: IssueValidateInput = parse_args(arguments)?;
    let snapshot = issue_train::IssueTrainSnapshot {
        parent: issue_snapshot(input.parent),
        children: input.children.into_iter().map(issue_snapshot).collect(),
    };
    let report = issue_train::validate_issue_train(&snapshot);
    let output = IssueValidateOutput {
        ok: report.valid,
        advisory_only: true,
        valid: report.valid,
        parent_issue: report.parent_issue,
        child_count: report.child_count,
        findings: report
            .findings
            .into_iter()
            .map(|finding| IssueFindingOutput {
                severity: json_string(&finding.severity),
                code: finding.code,
                issue_number: finding.issue_number,
                message: redact_text(&finding.message),
                remediation: redact_text(&finding.remediation),
            })
            .collect(),
    };
    Ok(success_result(
        "Aegis issue validation completed.",
        &output,
    )?)
}

fn handle_doctor(arguments: Option<JsonObject>, config: &Config) -> anyhow::Result<CallToolResult> {
    let _input: EmptyInput = parse_args(arguments)?;
    let report = build_doctor_report(config);
    let output = doctor_output(report)?;
    Ok(success_result(
        "Aegis doctor diagnostics are available.",
        &output,
    )?)
}

fn load_method_state(input: MethodStateInput) -> anyhow::Result<MethodState> {
    match (input.method_state, input.method_state_path) {
        (Some(state), None) => Ok(state),
        (None, Some(path)) => {
            let text = std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read method state from {path}"))?;
            serde_json::from_str(&text)
                .with_context(|| format!("failed to parse method state from {path}"))
        }
        (Some(_), Some(_)) => {
            anyhow::bail!("provide either methodState or methodStatePath, not both")
        }
        (None, None) => anyhow::bail!("methodState or methodStatePath is required"),
    }
}

fn create_tool<I, O>(name: &'static str, title: &'static str, description: &'static str) -> Tool
where
    I: JsonSchema,
    O: JsonSchema,
{
    Tool {
        name: name.into(),
        title: Some(title.to_string()),
        input_schema: schema_for::<I>(),
        output_schema: Some(schema_for::<O>()),
        description: Some(description.into()),
        annotations: None,
        execution: None,
        icons: None,
        meta: None,
    }
}

fn schema_for<T: JsonSchema>() -> Arc<JsonObject> {
    let schema = SchemaSettings::draft2019_09()
        .with(|settings| {
            settings.inline_subschemas = true;
            settings.option_add_null_type = false;
        })
        .into_generator()
        .into_root_schema_for::<T>();
    let schema_value = serde_json::to_value(&schema).expect("tool schema should serialize");
    let mut schema_object = match schema_value {
        Value::Object(object) => object,
        _ => panic!("tool schema should serialize to a JSON object"),
    };
    let mut tool_schema = JsonObject::new();
    for key in ["properties", "required", "type", "$defs", "definitions"] {
        if let Some(value) = schema_object.remove(key) {
            tool_schema.insert(key.to_string(), value);
        }
    }
    Arc::new(tool_schema)
}

fn parse_args<T: DeserializeOwned>(arguments: Option<JsonObject>) -> anyhow::Result<T> {
    serde_json::from_value(Value::Object(arguments.unwrap_or_default()))
        .context("failed to parse Aegis MCP tool arguments")
}

fn success_result<T: Serialize>(summary: &str, output: &T) -> anyhow::Result<CallToolResult> {
    let structured_content = structured_content(output)?;
    Ok(CallToolResult {
        content: vec![Content::text(summary.to_string())],
        structured_content: Some(Value::Object(structured_content)),
        is_error: Some(false),
        meta: None,
    })
}

fn error_result(message: String) -> CallToolResult {
    CallToolResult {
        content: vec![Content::text(message)],
        structured_content: None,
        is_error: Some(true),
        meta: None,
    }
}

fn structured_content<T: Serialize>(output: &T) -> anyhow::Result<JsonObject> {
    match serde_json::to_value(output)? {
        Value::Object(object) => Ok(object),
        value => {
            let mut object = JsonObject::new();
            object.insert("value".to_string(), value);
            Ok(object)
        }
    }
}

fn context_pack_summary(diagnostics: &[ContextPackDiagnostic]) -> ContextPackSummaryOutput {
    let mut summary = ContextPackSummaryOutput {
        total: diagnostics.len(),
        active: diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.active)
            .count(),
        candidate: 0,
        promoted: 0,
        retired: 0,
        invalid: 0,
        unreadable: 0,
    };
    for diagnostic in diagnostics {
        match diagnostic.diagnostic_status() {
            ContextPackDiagnosticStatus::Candidate => summary.candidate += 1,
            ContextPackDiagnosticStatus::Promoted => summary.promoted += 1,
            ContextPackDiagnosticStatus::Retired => summary.retired += 1,
            ContextPackDiagnosticStatus::Invalid => summary.invalid += 1,
            ContextPackDiagnosticStatus::Unreadable => summary.unreadable += 1,
        }
    }
    summary
}

fn context_pack_diagnostic_output(
    diagnostic: &ContextPackDiagnostic,
) -> ContextPackDiagnosticOutput {
    ContextPackDiagnosticOutput {
        path: diagnostic.path.clone(),
        pack_id: diagnostic.pack_id.clone(),
        kind: diagnostic
            .kind
            .map(context_pack_kind_label)
            .map(str::to_string),
        schema_version: diagnostic.schema_version,
        promotion_status: diagnostic
            .promotion_status
            .map(promotion_status_label)
            .map(str::to_string),
        status: context_pack_status_label(diagnostic.diagnostic_status()).to_string(),
        active: diagnostic.active,
        reason: redact_text(&diagnostic.reason),
    }
}

fn context_pack_matches_kind(
    diagnostic: &ContextPackDiagnostic,
    kind: Option<ContextPackKindInput>,
) -> bool {
    let Some(kind) = kind else {
        return true;
    };
    matches!(
        (diagnostic.kind, kind),
        (Some(ContextPackKind::User), ContextPackKindInput::User)
            | (
                Some(ContextPackKind::Project),
                ContextPackKindInput::Project
            )
            | (
                Some(ContextPackKind::Learned),
                ContextPackKindInput::Learned
            )
    )
}

fn context_pack_matches_status(
    diagnostic: &ContextPackDiagnostic,
    status: Option<ContextPackStatusInput>,
) -> bool {
    let Some(status) = status else {
        return true;
    };
    matches!(
        (diagnostic.diagnostic_status(), status),
        (_, ContextPackStatusInput::All)
            | (
                ContextPackDiagnosticStatus::Candidate,
                ContextPackStatusInput::Candidate
            )
            | (
                ContextPackDiagnosticStatus::Promoted,
                ContextPackStatusInput::Promoted
            )
            | (
                ContextPackDiagnosticStatus::Retired,
                ContextPackStatusInput::Retired
            )
            | (
                ContextPackDiagnosticStatus::Invalid,
                ContextPackStatusInput::Invalid
            )
            | (
                ContextPackDiagnosticStatus::Unreadable,
                ContextPackStatusInput::Unreadable
            )
    )
}

fn context_pack_inspection_output(inspection: ContextPackInspection) -> ContextPackInspectOutput {
    let provider_defaults = inspection.provider_defaults;
    ContextPackInspectOutput {
        ok: true,
        advisory_only: true,
        path: inspection.path,
        pack_id: inspection.pack_id,
        kind: context_pack_kind_label(inspection.kind).to_string(),
        schema_version: inspection.schema_version,
        name: redact_text(&inspection.name),
        description: inspection.description.as_deref().map(redact_text),
        promotion_status: promotion_status_label(inspection.promotion.status).to_string(),
        evidence_requirements: inspection
            .evidence_requirements
            .into_iter()
            .map(|requirement| ContextPackEvidenceRequirementOutput {
                id: requirement.id,
                description: redact_text(&requirement.description),
                commands: requirement
                    .commands
                    .into_iter()
                    .map(|command| redact_text(&command))
                    .collect(),
            })
            .collect(),
        provider_preferred: provider_defaults
            .as_ref()
            .and_then(|defaults| defaults.preferred.clone()),
        provider_fallbacks: provider_defaults
            .map(|defaults| defaults.fallbacks)
            .unwrap_or_default(),
        guidance_included: false,
    }
}

fn provider_output_from_doctor(report: &DoctorReport) -> ProviderOutput {
    ProviderOutput {
        id: report.provider.id.clone(),
        name: report.provider.name.clone(),
        model: report.provider.model.clone(),
        wire_api: report.provider.wire_api.clone(),
        env_key: report.provider.env_key.clone(),
        env_key_present: report.provider.env_key_present,
    }
}

fn doctor_output(report: DoctorReport) -> anyhow::Result<DoctorOutput> {
    let provider = provider_output_from_doctor(&report);
    let alerts = report.aegis_engine_alerts;
    Ok(DoctorOutput {
        ok: true,
        advisory_only: true,
        version: report.version,
        codex_home: report.codex_home,
        cwd: report.cwd,
        config_path: report.config_path,
        provider,
        context_packs: report
            .context_packs
            .iter()
            .map(context_pack_diagnostic_output)
            .collect(),
        aegis_engine_alerts: AegisEngineAlertsOutput {
            enabled: alerts.enabled,
            alerts_path: alerts.alerts_path,
            candidate_inputs_path: alerts.candidate_inputs_path,
            malformed_count: alerts.malformed_count,
            stale_count: alerts.stale_count,
            active_warning_count: alerts.active_warning_count,
            active_blocking_count: alerts.active_blocking_count,
            last_read_error: alerts.last_read_error.as_deref().map(redact_text),
        },
    })
}

fn evidence_requirement_output(
    requirement: &codex_protocol::method_state::MethodEvidenceRequirement,
) -> EvidenceRequirementOutput {
    EvidenceRequirementOutput {
        id: requirement.id.clone(),
        summary: redact_text(&requirement.summary),
        required: requirement.required,
        commands: requirement
            .commands
            .iter()
            .map(|command| redact_text(command))
            .collect(),
    }
}

fn evidence_output(evidence: &MethodEvidence) -> EvidenceOutput {
    EvidenceOutput {
        id: evidence.id.clone(),
        summary: redact_text(&evidence.summary),
        kind: json_string(&evidence.kind),
        requirement_ids: evidence.requirement_ids.clone(),
        successful_receipt: evidence.has_successful_receipt(),
        receipt: evidence.receipt.as_ref().map(redacted_receipt),
    }
}

fn redacted_receipt(receipt: &MethodEvidenceReceipt) -> EvidenceReceiptOutput {
    let (command, command_redaction) = redact_method_evidence_command(&receipt.command);
    let (output_summary, output_redaction) = redact_method_evidence_output(&receipt.output_summary);
    let redaction_status = merge_method_evidence_redaction_status(
        receipt.redaction_status,
        merge_method_evidence_redaction_status(command_redaction, output_redaction),
    );
    EvidenceReceiptOutput {
        command,
        cwd: receipt.cwd.clone(),
        exit_code: receipt.exit_status.exit_code,
        timed_out: receipt.exit_status.timed_out,
        output_summary,
        redaction_status: redaction_status_label(redaction_status).to_string(),
    }
}

fn issue_snapshot(input: IssueSnapshotInput) -> issue_train::IssueSnapshot {
    issue_train::IssueSnapshot {
        number: input.number,
        title: input.title,
        state: match input.state {
            IssueStateInput::Open => issue_train::IssueState::Open,
            IssueStateInput::Closed => issue_train::IssueState::Closed,
        },
        body: input.body,
        labels: input.labels,
    }
}

fn tool_preflight_context_from_method_state(
    state: &MethodState,
) -> tool_preflight::ToolPreflightContext {
    tool_preflight::ToolPreflightContext {
        method_state_available: true,
        method_state_valid: state.is_closure_valid(),
        linked_issue_available: state.linked_issue.is_some(),
    }
}

fn resolve_absolute_path(raw: &str, config: &Config) -> anyhow::Result<AbsolutePathBuf> {
    let path = PathBuf::from(raw);
    let path = if path.is_absolute() {
        path
    } else {
        config.cwd.as_path().join(path)
    };
    AbsolutePathBuf::try_from(path).context("path must resolve to an absolute path")
}

fn redact_text(text: &str) -> String {
    redact_method_evidence_output(text).0
}

fn json_string<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "unknown".to_string())
}

fn context_pack_kind_label(kind: ContextPackKind) -> &'static str {
    match kind {
        ContextPackKind::User => "user",
        ContextPackKind::Project => "project",
        ContextPackKind::Learned => "learned",
    }
}

fn context_pack_status_label(status: ContextPackDiagnosticStatus) -> &'static str {
    match status {
        ContextPackDiagnosticStatus::Candidate => "candidate",
        ContextPackDiagnosticStatus::Promoted => "promoted",
        ContextPackDiagnosticStatus::Retired => "retired",
        ContextPackDiagnosticStatus::Invalid => "invalid",
        ContextPackDiagnosticStatus::Unreadable => "unreadable",
    }
}

fn promotion_status_label(status: PromotionStatus) -> &'static str {
    match status {
        PromotionStatus::Candidate => "candidate",
        PromotionStatus::Promoted => "promoted",
        PromotionStatus::Retired => "retired",
    }
}

fn redaction_status_label(status: MethodEvidenceRedactionStatus) -> &'static str {
    match status {
        MethodEvidenceRedactionStatus::NotNeeded => "not_needed",
        MethodEvidenceRedactionStatus::Redacted => "redacted",
        MethodEvidenceRedactionStatus::Unknown => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aegis_tools_have_structured_schemas() {
        let tools = create_aegis_tools();
        let names = tools
            .iter()
            .map(|tool| tool.name.as_ref())
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                AEGIS_STATUS_TOOL,
                AEGIS_CHECK_TOOL,
                AEGIS_EVIDENCE_TOOL,
                AEGIS_REVIEW_TOOL,
                AEGIS_CONTEXT_PACK_LIST_TOOL,
                AEGIS_CONTEXT_PACK_INSPECT_TOOL,
                AEGIS_POLICY_EXPLAIN_TOOL,
                AEGIS_ISSUE_VALIDATE_TOOL,
                AEGIS_DOCTOR_TOOL,
            ]
        );
        for tool in tools {
            assert!(tool.input_schema.contains_key("type"));
            assert!(tool.output_schema.is_some());
            assert!(tool.description.is_some());
        }
    }

    #[test]
    fn evidence_receipt_output_is_redacted() {
        let receipt = MethodEvidenceReceipt {
            schema_version: 1,
            command: vec![
                "curl".to_string(),
                "-H".to_string(),
                "Authorization: bearer secret-token".to_string(),
            ],
            cwd: "/tmp".to_string(),
            captured_at_unix_seconds: 1,
            git_state: codex_protocol::method_state::MethodEvidenceGitState {
                status: codex_protocol::method_state::MethodEvidenceGitStateStatus::Unavailable,
                repository: None,
                branch: None,
                commit: None,
                dirty: None,
                unavailable_reason: None,
            },
            exit_status: codex_protocol::method_state::MethodEvidenceExitStatus {
                exit_code: Some(0),
                timed_out: false,
            },
            output_summary: "Authorization: bearer secret-token passed".to_string(),
            artifacts: Vec::new(),
            session: codex_protocol::method_state::MethodEvidenceSessionMetadata {
                session_id: None,
                thread_id: None,
                provider: None,
                model: None,
            },
            redaction_status: MethodEvidenceRedactionStatus::NotNeeded,
        };

        let output = redacted_receipt(&receipt);

        assert_eq!(output.command[2], "<redacted>");
        assert!(output.output_summary.contains("<redacted>"));
        assert_eq!(output.redaction_status, "redacted");
    }

    #[test]
    fn policy_schema_uses_camel_case_change_count() {
        let tool = create_aegis_tools()
            .into_iter()
            .find(|tool| tool.name == AEGIS_POLICY_EXPLAIN_TOOL)
            .expect("policy tool");
        let schema = serde_json::to_string(&tool.input_schema).expect("serialize schema");

        assert!(schema.contains("changeCount"));
        assert!(!schema.contains("change_count"));
    }
}
