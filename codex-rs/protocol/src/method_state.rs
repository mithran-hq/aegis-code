//! Typed Aegis method state shared by runtime, persistence, and UI surfaces.

use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use ts_rs::TS;

pub const METHOD_STATE_SCHEMA_VERSION: u32 = 1;
pub const METHOD_EVIDENCE_RECEIPT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum MethodStatusKind {
    Missing,
    Loaded,
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct MethodContextPackStatusSummary {
    pub active: u64,
    pub ignored: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct MethodStatusSummary {
    pub kind: MethodStatusKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub sandbox_posture: Option<MethodSandboxPosture>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub linked_issue: Option<MethodLinkedIssue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub work_status: Option<MethodWorkStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub resume_validity: Option<MethodResumeValidityStatus>,
    #[serde(default)]
    pub resume_reasons: Vec<MethodResumeValidityReason>,
    pub required_evidence_total: u64,
    pub required_evidence_satisfied: u64,
    pub evidence_total: u64,
    pub gates_pending: u64,
    pub gates_failed: u64,
    pub gates_blocked: u64,
    pub review_open_blocking: u64,
    pub review_open_high: u64,
    pub review_open_medium: u64,
    pub engine_alerts_warned: u64,
    pub engine_alerts_blocked: u64,
    pub context_packs: MethodContextPackStatusSummary,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub diagnostic: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub updated_at_unix_seconds: Option<i64>,
}

impl MethodStatusSummary {
    pub fn missing(context_packs: MethodContextPackStatusSummary) -> Self {
        Self {
            kind: MethodStatusKind::Missing,
            sandbox_posture: None,
            linked_issue: None,
            work_status: None,
            resume_validity: None,
            resume_reasons: Vec::new(),
            required_evidence_total: 0,
            required_evidence_satisfied: 0,
            evidence_total: 0,
            gates_pending: 0,
            gates_failed: 0,
            gates_blocked: 0,
            review_open_blocking: 0,
            review_open_high: 0,
            review_open_medium: 0,
            engine_alerts_warned: 0,
            engine_alerts_blocked: 0,
            context_packs,
            diagnostic: None,
            updated_at_unix_seconds: None,
        }
    }

    pub fn invalid(diagnostic: String, context_packs: MethodContextPackStatusSummary) -> Self {
        Self {
            diagnostic: Some(diagnostic),
            kind: MethodStatusKind::Invalid,
            ..Self::missing(context_packs)
        }
    }

    pub fn loaded(
        state: &MethodState,
        resume_validity: &MethodResumeValidityReport,
        context_packs: MethodContextPackStatusSummary,
    ) -> Self {
        let required_evidence = state
            .evidence_requirements
            .iter()
            .filter(|requirement| requirement.required)
            .collect::<Vec<_>>();
        let required_evidence_satisfied = required_evidence
            .iter()
            .filter(|requirement| {
                state.evidence.iter().any(|evidence| {
                    evidence.requirement_ids.contains(&requirement.id)
                        && evidence.has_successful_receipt()
                })
            })
            .count() as u64;

        Self {
            kind: MethodStatusKind::Loaded,
            sandbox_posture: state.resume_context.sandbox_posture.clone(),
            linked_issue: state.linked_issue.clone(),
            work_status: Some(state.status),
            resume_validity: Some(resume_validity.status),
            resume_reasons: resume_validity.reasons.clone(),
            required_evidence_total: required_evidence.len() as u64,
            required_evidence_satisfied,
            evidence_total: state.evidence.len() as u64,
            gates_pending: state
                .gates
                .iter()
                .filter(|gate| gate.status == MethodGateStatus::Pending)
                .count() as u64,
            gates_failed: state
                .gates
                .iter()
                .filter(|gate| gate.status == MethodGateStatus::Failed)
                .count() as u64,
            gates_blocked: state
                .gates
                .iter()
                .filter(|gate| gate.status == MethodGateStatus::Blocked)
                .count() as u64,
            review_open_blocking: state
                .review_findings
                .iter()
                .filter(|finding| {
                    finding.status == MethodReviewFindingStatus::Open
                        && finding.severity == MethodReviewSeverity::Blocking
                })
                .count() as u64,
            review_open_high: state
                .review_findings
                .iter()
                .filter(|finding| {
                    finding.status == MethodReviewFindingStatus::Open
                        && finding.severity == MethodReviewSeverity::High
                })
                .count() as u64,
            review_open_medium: state
                .review_findings
                .iter()
                .filter(|finding| {
                    finding.status == MethodReviewFindingStatus::Open
                        && finding.severity == MethodReviewSeverity::Medium
                })
                .count() as u64,
            engine_alerts_warned: state
                .engine_alerts
                .iter()
                .filter(|alert| alert.status == MethodEngineAlertStatus::Warned)
                .count() as u64,
            engine_alerts_blocked: state
                .engine_alerts
                .iter()
                .filter(|alert| alert.status == MethodEngineAlertStatus::Blocked)
                .count() as u64,
            context_packs,
            diagnostic: None,
            updated_at_unix_seconds: Some(state.provenance.updated_at_unix_seconds),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct MethodSandboxPosture {
    pub mode: String,
    pub permission_profile: String,
    pub enforcement: String,
    pub network: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub policy: Option<MethodSandboxPolicySummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct MethodSandboxPolicySummary {
    pub status: MethodSandboxPolicyStatus,
    #[serde(default)]
    pub allowed_modes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub diagnostic: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum MethodSandboxPolicyStatus {
    Unrestricted,
    Allowed,
    Blocked,
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct MethodState {
    pub schema_version: u32,
    pub intent: MethodIntent,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub linked_issue: Option<MethodLinkedIssue>,
    pub status: MethodWorkStatus,
    #[serde(default)]
    pub claims: Vec<MethodClaim>,
    #[serde(default)]
    pub assumptions: Vec<MethodAssumption>,
    #[serde(default)]
    pub falsifiers: Vec<MethodFalsifier>,
    #[serde(default)]
    pub evidence_requirements: Vec<MethodEvidenceRequirement>,
    #[serde(default)]
    pub evidence: Vec<MethodEvidence>,
    #[serde(default)]
    pub gates: Vec<MethodGate>,
    #[serde(default)]
    pub engine_alerts: Vec<MethodEngineAlert>,
    #[serde(default)]
    pub review_findings: Vec<MethodReviewFinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub closure: Option<MethodClosureState>,
    pub resume_context: MethodResumeContext,
    pub provenance: MethodProvenance,
}

impl MethodState {
    pub fn closure_evidence_gaps(&self) -> Vec<MethodEvidenceRequirement> {
        let closure_evidence_ids = self
            .closure
            .as_ref()
            .map(|closure| closure.evidence_ids.as_slice());

        self.evidence_requirements
            .iter()
            .filter(|requirement| requirement.required)
            .filter(|requirement| {
                !self.evidence.iter().any(|evidence| {
                    let satisfies_requirement = evidence.requirement_ids.contains(&requirement.id);
                    let has_successful_receipt = evidence.has_successful_receipt();
                    let cited_by_closure = closure_evidence_ids
                        .map(|ids| ids.iter().any(|id| id == &evidence.id))
                        .unwrap_or(true);
                    satisfies_requirement && has_successful_receipt && cited_by_closure
                })
            })
            .cloned()
            .collect()
    }

    pub fn is_closure_valid(&self) -> bool {
        if self.status != MethodWorkStatus::Closed {
            return true;
        }

        let Some(closure) = &self.closure else {
            return false;
        };
        if closure.evidence_ids.is_empty() || !self.closure_evidence_gaps().is_empty() {
            return false;
        }
        if closure.review_finding_ids.is_empty() || !self.closure_review_findings_valid(closure) {
            return false;
        }
        if self
            .engine_alerts
            .iter()
            .any(|alert| alert.status == MethodEngineAlertStatus::Blocked)
        {
            return false;
        }
        closure.evidence_ids.iter().all(|id| {
            self.evidence.iter().any(|evidence| {
                evidence.id.as_str() == id.as_str() && evidence.has_successful_receipt()
            })
        })
    }

    fn closure_review_findings_valid(&self, closure: &MethodClosureState) -> bool {
        let cited_findings = closure
            .review_finding_ids
            .iter()
            .filter_map(|id| {
                self.review_findings
                    .iter()
                    .find(|finding| finding.id == *id)
            })
            .collect::<Vec<_>>();
        if cited_findings.len() != closure.review_finding_ids.len() {
            return false;
        }

        !self.review_findings.iter().any(|finding| {
            finding.status == MethodReviewFindingStatus::Open
                && matches!(
                    finding.severity,
                    MethodReviewSeverity::Blocking
                        | MethodReviewSeverity::High
                        | MethodReviewSeverity::Medium
                )
        })
    }

    pub fn compute_resume_validity(
        &self,
        current_context: &MethodResumeContext,
    ) -> MethodResumeValidityReport {
        let mut reasons = Vec::new();

        compare_required_string(
            &self.resume_context.repository,
            &current_context.repository,
            MethodResumeValidityReason::MissingPersistedRepository,
            MethodResumeValidityReason::MissingCurrentRepository,
            MethodResumeValidityReason::RepositoryMismatch,
            &mut reasons,
        );
        compare_required_string(
            &self.resume_context.branch,
            &current_context.branch,
            MethodResumeValidityReason::MissingPersistedBranch,
            MethodResumeValidityReason::MissingCurrentBranch,
            MethodResumeValidityReason::BranchChanged,
            &mut reasons,
        );
        compare_required_string(
            &self.resume_context.commit,
            &current_context.commit,
            MethodResumeValidityReason::MissingPersistedCommit,
            MethodResumeValidityReason::MissingCurrentCommit,
            MethodResumeValidityReason::CommitChanged,
            &mut reasons,
        );

        match (
            &self.resume_context.linked_issue,
            &current_context.linked_issue,
        ) {
            (Some(persisted), Some(current)) if persisted != current => {
                reasons.push(MethodResumeValidityReason::IssueMismatch);
            }
            (Some(_), Some(_)) => {}
            (None, _) => reasons.push(MethodResumeValidityReason::MissingPersistedIssue),
            (_, None) => reasons.push(MethodResumeValidityReason::MissingCurrentIssue),
        }

        match (
            self.resume_context.schema_version,
            current_context.schema_version,
        ) {
            (Some(persisted), Some(current)) if persisted != current => {
                reasons.push(MethodResumeValidityReason::SchemaVersionMismatch);
            }
            (Some(_), Some(_)) => {}
            (None, _) => reasons.push(MethodResumeValidityReason::MissingPersistedSchemaVersion),
            (_, None) => reasons.push(MethodResumeValidityReason::MissingCurrentSchemaVersion),
        }

        match (
            &self.resume_context.sandbox_posture,
            &current_context.sandbox_posture,
        ) {
            (Some(persisted), Some(current)) if persisted != current => {
                reasons.push(MethodResumeValidityReason::SandboxPostureChanged);
            }
            (Some(_), Some(_)) => {}
            (None, Some(_)) => {
                reasons.push(MethodResumeValidityReason::MissingPersistedSandboxPosture)
            }
            (Some(_), None) => {
                reasons.push(MethodResumeValidityReason::MissingCurrentSandboxPosture)
            }
            (None, None) => {}
        }

        let status = if reasons
            .iter()
            .any(MethodResumeValidityReason::invalidates_resume)
        {
            MethodResumeValidityStatus::Invalid
        } else if reasons.is_empty() {
            MethodResumeValidityStatus::Valid
        } else {
            MethodResumeValidityStatus::Stale
        };

        MethodResumeValidityReport { status, reasons }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct MethodIntent {
    pub summary: String,
    #[serde(default)]
    pub success_criteria: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct MethodLinkedIssue {
    pub provider: MethodIssueProvider,
    pub repository: String,
    pub number: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct MethodIssueRef {
    pub provider: MethodIssueProvider,
    pub repository: String,
    pub number: u64,
}

impl From<&MethodLinkedIssue> for MethodIssueRef {
    fn from(value: &MethodLinkedIssue) -> Self {
        Self {
            provider: value.provider,
            repository: value.repository.clone(),
            number: value.number,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum MethodIssueProvider {
    GitHub,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum MethodWorkStatus {
    Incomplete,
    Blocked,
    Failed,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct MethodClaim {
    pub id: String,
    pub summary: String,
    #[serde(default)]
    pub evidence_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct MethodAssumption {
    pub id: String,
    pub summary: String,
    #[serde(default)]
    pub falsifier_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct MethodFalsifier {
    pub id: String,
    pub summary: String,
    pub status: MethodFalsifierStatus,
    #[serde(default)]
    pub evidence_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum MethodFalsifierStatus {
    Open,
    Disproved,
    Confirmed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct MethodEvidenceRequirement {
    pub id: String,
    pub summary: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub commands: Vec<String>,
    #[serde(default)]
    pub claim_ids: Vec<String>,
    #[serde(default)]
    pub falsifier_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct MethodEvidence {
    pub id: String,
    pub summary: String,
    pub kind: MethodEvidenceKind,
    #[serde(default)]
    pub requirement_ids: Vec<String>,
    #[serde(default)]
    pub claim_ids: Vec<String>,
    #[serde(default)]
    pub falsifier_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source: Option<String>,
    pub captured_at_unix_seconds: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub receipt: Option<MethodEvidenceReceipt>,
}

impl MethodEvidence {
    pub fn has_successful_receipt(&self) -> bool {
        self.receipt.as_ref().is_some_and(|receipt| {
            receipt.exit_status.exit_code == Some(0) && !receipt.exit_status.timed_out
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum MethodEvidenceKind {
    Command,
    Test,
    File,
    Commit,
    GitHub,
    Human,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct MethodEvidenceReceipt {
    pub schema_version: u32,
    pub command: Vec<String>,
    pub cwd: String,
    pub captured_at_unix_seconds: i64,
    pub git_state: MethodEvidenceGitState,
    pub exit_status: MethodEvidenceExitStatus,
    pub output_summary: String,
    #[serde(default)]
    pub artifacts: Vec<MethodEvidenceArtifactRef>,
    pub session: MethodEvidenceSessionMetadata,
    pub redaction_status: MethodEvidenceRedactionStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct MethodEvidenceGitState {
    pub status: MethodEvidenceGitStateStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub repository: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub commit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub dirty: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum MethodEvidenceGitStateStatus {
    Captured,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct MethodEvidenceExitStatus {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub timed_out: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct MethodEvidenceArtifactRef {
    pub kind: MethodEvidenceArtifactKind,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub digest: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum MethodEvidenceArtifactKind {
    Path,
    Uri,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct MethodEvidenceSessionMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub thread_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub sandbox_posture: Option<MethodSandboxPosture>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum MethodEvidenceRedactionStatus {
    NotNeeded,
    Redacted,
    Unknown,
}

pub fn redact_method_evidence_command(
    command: &[String],
) -> (Vec<String>, MethodEvidenceRedactionStatus) {
    let mut redacted = false;
    let mut redact_next = false;
    let command = command
        .iter()
        .map(|arg| {
            if redact_next {
                redact_next = false;
                redacted = true;
                return "<redacted>".to_string();
            }

            if !is_sensitive_evidence_token(arg) {
                return arg.clone();
            }

            redacted = true;
            if let Some((name, _)) = arg.split_once('=') {
                format!("{name}=<redacted>")
            } else if arg.starts_with('-') {
                redact_next = true;
                arg.clone()
            } else {
                redact_next = true;
                "<redacted>".to_string()
            }
        })
        .collect();

    (command, redaction_status(redacted))
}

pub fn redact_method_evidence_output(output: &str) -> (String, MethodEvidenceRedactionStatus) {
    let mut redacted = false;
    let mut redact_next = false;
    let output = output
        .split_whitespace()
        .map(|token| {
            if redact_next {
                let lower = token.to_ascii_lowercase();
                redact_next = lower.contains("authorization") || lower.contains("bearer");
                redacted = true;
                return "<redacted>";
            }

            if is_sensitive_evidence_token(token) {
                redacted = true;
                let lower = token.to_ascii_lowercase();
                if lower.contains("authorization")
                    || lower.contains("bearer")
                    || lower.contains("token")
                    || lower.contains("password")
                    || lower.contains("secret")
                    || lower.contains("api-key")
                    || lower.contains("apikey")
                    || lower == "-k"
                    || lower == "--key"
                {
                    redact_next = true;
                }
                "<redacted>"
            } else {
                token
            }
        })
        .collect::<Vec<_>>()
        .join(" ");

    (output, redaction_status(redacted))
}

pub fn merge_method_evidence_redaction_status(
    left: MethodEvidenceRedactionStatus,
    right: MethodEvidenceRedactionStatus,
) -> MethodEvidenceRedactionStatus {
    match (left, right) {
        (MethodEvidenceRedactionStatus::Redacted, _)
        | (_, MethodEvidenceRedactionStatus::Redacted) => MethodEvidenceRedactionStatus::Redacted,
        (MethodEvidenceRedactionStatus::Unknown, _)
        | (_, MethodEvidenceRedactionStatus::Unknown) => MethodEvidenceRedactionStatus::Unknown,
        _ => MethodEvidenceRedactionStatus::NotNeeded,
    }
}

fn redaction_status(redacted: bool) -> MethodEvidenceRedactionStatus {
    if redacted {
        MethodEvidenceRedactionStatus::Redacted
    } else {
        MethodEvidenceRedactionStatus::NotNeeded
    }
}

fn is_sensitive_evidence_token(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("token")
        || lower.contains("password")
        || lower.contains("secret")
        || lower.contains("authorization")
        || lower.contains("bearer")
        || lower.contains("api-key")
        || lower.contains("apikey")
        || lower == "-k"
        || lower == "--key"
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct MethodGate {
    pub id: String,
    pub name: String,
    pub status: MethodGateStatus,
    #[serde(default)]
    pub evidence_requirement_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub rationale: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum MethodGateStatus {
    Pending,
    Passed,
    Failed,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct MethodEngineAlert {
    pub id: String,
    pub summary: String,
    pub severity: MethodEngineAlertSeverity,
    pub action: MethodEngineAlertAction,
    pub status: MethodEngineAlertStatus,
    pub source_event: MethodEngineAlertSourceEvent,
    pub created_at_unix_seconds: i64,
    pub received_at_unix_seconds: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub candidate_input_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum MethodEngineAlertSeverity {
    Safe,
    Suspicious,
    Malicious,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum MethodEngineAlertAction {
    Observe,
    Warn,
    Block,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum MethodEngineAlertStatus {
    Observed,
    Warned,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct MethodEngineAlertSourceEvent {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub event_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub thread_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub turn_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub evidence_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub finding_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct MethodReviewFinding {
    pub id: String,
    pub summary: String,
    pub severity: MethodReviewSeverity,
    pub status: MethodReviewFindingStatus,
    #[serde(default)]
    pub claim_ids: Vec<String>,
    #[serde(default)]
    pub evidence_ids: Vec<String>,
    pub reviewed_at_unix_seconds: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub reviewer: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum MethodReviewSeverity {
    Info,
    Low,
    Medium,
    High,
    Blocking,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum MethodReviewFindingStatus {
    Open,
    Addressed,
    AcceptedRisk,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct MethodClosureState {
    pub closed_at_unix_seconds: i64,
    pub summary: String,
    pub evidence_ids: Vec<String>,
    #[serde(default)]
    pub review_finding_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub closed_by: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct MethodResumeContext {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub repository: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub commit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub linked_issue: Option<MethodIssueRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub schema_version: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub sandbox_posture: Option<MethodSandboxPosture>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct MethodResumeValidityReport {
    pub status: MethodResumeValidityStatus,
    pub reasons: Vec<MethodResumeValidityReason>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum MethodResumeValidityStatus {
    Valid,
    Stale,
    Invalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum MethodResumeValidityReason {
    MissingPersistedRepository,
    MissingCurrentRepository,
    RepositoryMismatch,
    MissingPersistedBranch,
    MissingCurrentBranch,
    BranchChanged,
    MissingPersistedCommit,
    MissingCurrentCommit,
    CommitChanged,
    MissingPersistedIssue,
    MissingCurrentIssue,
    IssueMismatch,
    MissingPersistedSchemaVersion,
    MissingCurrentSchemaVersion,
    SchemaVersionMismatch,
    MissingPersistedSandboxPosture,
    MissingCurrentSandboxPosture,
    SandboxPostureChanged,
}

impl MethodResumeValidityReason {
    fn invalidates_resume(&self) -> bool {
        matches!(
            self,
            Self::MissingPersistedRepository
                | Self::MissingCurrentRepository
                | Self::RepositoryMismatch
                | Self::MissingPersistedBranch
                | Self::MissingCurrentBranch
                | Self::MissingPersistedCommit
                | Self::MissingCurrentCommit
                | Self::MissingPersistedIssue
                | Self::MissingCurrentIssue
                | Self::IssueMismatch
                | Self::MissingPersistedSchemaVersion
                | Self::MissingCurrentSchemaVersion
                | Self::SchemaVersionMismatch
                | Self::MissingCurrentSandboxPosture
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct MethodProvenance {
    pub created_at_unix_seconds: i64,
    pub updated_at_unix_seconds: i64,
    pub source: MethodProvenanceSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub actor: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum MethodProvenanceSource {
    User,
    Agent,
    GitHubIssue,
    Resume,
    Import,
}

fn compare_required_string(
    persisted: &Option<String>,
    current: &Option<String>,
    missing_persisted: MethodResumeValidityReason,
    missing_current: MethodResumeValidityReason,
    changed: MethodResumeValidityReason,
    reasons: &mut Vec<MethodResumeValidityReason>,
) {
    match (persisted, current) {
        (Some(persisted), Some(current)) if persisted != current => reasons.push(changed),
        (Some(_), Some(_)) => {}
        (None, _) => reasons.push(missing_persisted),
        (_, None) => reasons.push(missing_current),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn issue() -> MethodLinkedIssue {
        MethodLinkedIssue {
            provider: MethodIssueProvider::GitHub,
            repository: "mithran-hq/aegis-code".to_string(),
            number: 8,
            title: Some("Task: Define Aegis Code method state model".to_string()),
            url: Some("https://github.com/mithran-hq/aegis-code/issues/8".to_string()),
        }
    }

    fn resume_context() -> MethodResumeContext {
        MethodResumeContext {
            repository: Some("mithran-hq/aegis-code".to_string()),
            branch: Some("master".to_string()),
            commit: Some("abc123".to_string()),
            linked_issue: Some(MethodIssueRef::from(&issue())),
            schema_version: Some(METHOD_STATE_SCHEMA_VERSION),
            sandbox_posture: None,
        }
    }

    fn receipt() -> MethodEvidenceReceipt {
        MethodEvidenceReceipt {
            schema_version: METHOD_EVIDENCE_RECEIPT_SCHEMA_VERSION,
            command: vec![
                "cargo".to_string(),
                "test".to_string(),
                "-p".to_string(),
                "codex-protocol".to_string(),
                "method_state".to_string(),
            ],
            cwd: "/repo".to_string(),
            captured_at_unix_seconds: 1_779_999_000,
            git_state: MethodEvidenceGitState {
                status: MethodEvidenceGitStateStatus::Captured,
                repository: Some("mithran-hq/aegis-code".to_string()),
                branch: Some("master".to_string()),
                commit: Some("abc123".to_string()),
                dirty: Some(false),
                unavailable_reason: None,
            },
            exit_status: MethodEvidenceExitStatus {
                exit_code: Some(0),
                timed_out: false,
            },
            output_summary: "test result: ok".to_string(),
            artifacts: Vec::new(),
            session: MethodEvidenceSessionMetadata {
                session_id: Some("session-1".to_string()),
                thread_id: Some("thread-1".to_string()),
                provider: Some("test-provider".to_string()),
                model: Some("test-model".to_string()),
                sandbox_posture: None,
            },
            redaction_status: MethodEvidenceRedactionStatus::NotNeeded,
        }
    }

    fn base_state(status: MethodWorkStatus) -> MethodState {
        MethodState {
            schema_version: METHOD_STATE_SCHEMA_VERSION,
            intent: MethodIntent {
                summary: "Represent method state as typed data".to_string(),
                success_criteria: vec!["serialization tests pass".to_string()],
            },
            linked_issue: Some(issue()),
            status,
            claims: vec![MethodClaim {
                id: "claim:model-exists".to_string(),
                summary: "Typed model exists".to_string(),
                evidence_ids: vec!["evidence:test".to_string()],
            }],
            assumptions: vec![MethodAssumption {
                id: "assumption:protocol-home".to_string(),
                summary: "codex-protocol is the right shared boundary".to_string(),
                falsifier_ids: vec!["falsifier:not-reusable".to_string()],
            }],
            falsifiers: vec![MethodFalsifier {
                id: "falsifier:not-reusable".to_string(),
                summary: "The model cannot be used outside core".to_string(),
                status: MethodFalsifierStatus::Disproved,
                evidence_ids: vec!["evidence:test".to_string()],
            }],
            evidence_requirements: vec![MethodEvidenceRequirement {
                id: "requirement:serialization".to_string(),
                summary: "Serialization round trip passes".to_string(),
                required: true,
                commands: vec!["cargo test -p codex-protocol method_state".to_string()],
                claim_ids: vec!["claim:model-exists".to_string()],
                falsifier_ids: vec!["falsifier:not-reusable".to_string()],
            }],
            evidence: vec![MethodEvidence {
                id: "evidence:test".to_string(),
                summary: "cargo test -p codex-protocol method_state passed".to_string(),
                kind: MethodEvidenceKind::Test,
                requirement_ids: vec!["requirement:serialization".to_string()],
                claim_ids: vec!["claim:model-exists".to_string()],
                falsifier_ids: vec!["falsifier:not-reusable".to_string()],
                source: Some("local".to_string()),
                captured_at_unix_seconds: 1_779_999_000,
                receipt: Some(receipt()),
            }],
            gates: vec![MethodGate {
                id: "gate:local-ci".to_string(),
                name: "Local CI".to_string(),
                status: MethodGateStatus::Pending,
                evidence_requirement_ids: vec!["requirement:serialization".to_string()],
                rationale: None,
            }],
            engine_alerts: Vec::new(),
            review_findings: vec![MethodReviewFinding {
                id: "finding:none".to_string(),
                summary: "No blocking review findings".to_string(),
                severity: MethodReviewSeverity::Info,
                status: MethodReviewFindingStatus::Addressed,
                claim_ids: vec!["claim:model-exists".to_string()],
                evidence_ids: vec!["evidence:test".to_string()],
                reviewed_at_unix_seconds: 1_779_999_100,
                reviewer: Some("codex".to_string()),
            }],
            closure: None,
            resume_context: resume_context(),
            provenance: MethodProvenance {
                created_at_unix_seconds: 1_779_998_000,
                updated_at_unix_seconds: 1_779_999_100,
                source: MethodProvenanceSource::Agent,
                actor: Some("codex".to_string()),
            },
        }
    }

    #[test]
    fn incomplete_blocked_failed_and_closed_states_round_trip() {
        for status in [
            MethodWorkStatus::Incomplete,
            MethodWorkStatus::Blocked,
            MethodWorkStatus::Failed,
            MethodWorkStatus::Closed,
        ] {
            let mut state = base_state(status);
            if status == MethodWorkStatus::Closed {
                state.closure = Some(MethodClosureState {
                    closed_at_unix_seconds: 1_779_999_200,
                    summary: "All required evidence is present".to_string(),
                    evidence_ids: vec!["evidence:test".to_string()],
                    review_finding_ids: vec!["finding:none".to_string()],
                    closed_by: Some("codex".to_string()),
                });
            }

            let json = serde_json::to_string(&state).expect("serialize method state");
            let round_tripped: MethodState =
                serde_json::from_str(&json).expect("deserialize method state");
            assert_eq!(round_tripped, state);
        }
    }

    #[test]
    fn claims_evidence_and_review_findings_are_distinct_records() {
        let state = base_state(MethodWorkStatus::Incomplete);
        assert_eq!(state.claims[0].id, "claim:model-exists");
        assert_eq!(state.evidence[0].id, "evidence:test");
        assert_eq!(state.review_findings[0].id, "finding:none");
        assert_eq!(state.claims[0].evidence_ids, vec!["evidence:test"]);
        assert_eq!(
            state.evidence[0].claim_ids,
            vec!["claim:model-exists".to_string()]
        );
        assert_eq!(
            state.review_findings[0].evidence_ids,
            vec!["evidence:test".to_string()]
        );
    }

    #[test]
    fn closed_state_without_required_evidence_is_invalid() {
        let mut state = base_state(MethodWorkStatus::Closed);
        state.evidence.clear();
        state.closure = Some(MethodClosureState {
            closed_at_unix_seconds: 1_779_999_200,
            summary: "Premature closure".to_string(),
            evidence_ids: Vec::new(),
            review_finding_ids: Vec::new(),
            closed_by: Some("codex".to_string()),
        });

        assert_eq!(
            state.closure_evidence_gaps()[0].id,
            "requirement:serialization"
        );
        assert!(!state.is_closure_valid());
    }

    #[test]
    fn closed_state_with_required_evidence_is_valid() {
        let mut state = base_state(MethodWorkStatus::Closed);
        state.closure = Some(MethodClosureState {
            closed_at_unix_seconds: 1_779_999_200,
            summary: "All required evidence is present".to_string(),
            evidence_ids: vec!["evidence:test".to_string()],
            review_finding_ids: vec!["finding:none".to_string()],
            closed_by: Some("codex".to_string()),
        });

        assert!(state.closure_evidence_gaps().is_empty());
        assert!(state.is_closure_valid());
    }

    #[test]
    fn closed_state_with_free_form_evidence_only_is_invalid() {
        let mut state = base_state(MethodWorkStatus::Closed);
        state.evidence[0].receipt = None;
        state.closure = Some(MethodClosureState {
            closed_at_unix_seconds: 1_779_999_200,
            summary: "Free-form evidence is not enough".to_string(),
            evidence_ids: vec!["evidence:test".to_string()],
            review_finding_ids: vec!["finding:none".to_string()],
            closed_by: Some("codex".to_string()),
        });

        assert_eq!(
            state.closure_evidence_gaps()[0].id,
            "requirement:serialization"
        );
        assert!(!state.is_closure_valid());
    }

    #[test]
    fn closed_state_with_failed_required_receipt_is_invalid() {
        let mut state = base_state(MethodWorkStatus::Closed);
        state.evidence[0]
            .receipt
            .as_mut()
            .expect("receipt")
            .exit_status
            .exit_code = Some(101);
        state.closure = Some(MethodClosureState {
            closed_at_unix_seconds: 1_779_999_200,
            summary: "Failed tests do not close required evidence".to_string(),
            evidence_ids: vec!["evidence:test".to_string()],
            review_finding_ids: vec!["finding:none".to_string()],
            closed_by: Some("codex".to_string()),
        });

        assert_eq!(
            state.closure_evidence_gaps()[0].id,
            "requirement:serialization"
        );
        assert!(!state.is_closure_valid());
    }

    #[test]
    fn evidence_requirement_defaults_accept_old_json() {
        let json = r#"{
            "id": "requirement:old",
            "summary": "Old persisted requirement",
            "required": true
        }"#;

        let requirement: MethodEvidenceRequirement =
            serde_json::from_str(json).expect("old requirement shape deserializes");
        assert_eq!(requirement.id, "requirement:old");
        assert!(requirement.commands.is_empty());
        assert!(requirement.claim_ids.is_empty());
        assert!(requirement.falsifier_ids.is_empty());
    }

    #[test]
    fn closed_state_requires_closure_referenced_evidence() {
        let mut state = base_state(MethodWorkStatus::Closed);
        state.evidence.push(MethodEvidence {
            id: "evidence:unrelated".to_string(),
            summary: "Unrelated command output".to_string(),
            kind: MethodEvidenceKind::Command,
            requirement_ids: Vec::new(),
            claim_ids: Vec::new(),
            falsifier_ids: Vec::new(),
            source: Some("local".to_string()),
            captured_at_unix_seconds: 1_779_999_050,
            receipt: Some(receipt()),
        });
        state.closure = Some(MethodClosureState {
            closed_at_unix_seconds: 1_779_999_200,
            summary: "Closure cites unrelated evidence".to_string(),
            evidence_ids: vec!["evidence:unrelated".to_string()],
            review_finding_ids: vec!["finding:none".to_string()],
            closed_by: Some("codex".to_string()),
        });

        assert_eq!(
            state.closure_evidence_gaps()[0].id,
            "requirement:serialization"
        );
        assert!(!state.is_closure_valid());
    }

    #[test]
    fn closed_state_requires_closure_referenced_review_finding() {
        let mut state = base_state(MethodWorkStatus::Closed);
        state.closure = Some(MethodClosureState {
            closed_at_unix_seconds: 1_779_999_200,
            summary: "Missing review finding citation".to_string(),
            evidence_ids: vec!["evidence:test".to_string()],
            review_finding_ids: Vec::new(),
            closed_by: Some("codex".to_string()),
        });

        assert!(!state.is_closure_valid());

        state.closure.as_mut().expect("closure").review_finding_ids =
            vec!["finding:missing".to_string()];
        assert!(!state.is_closure_valid());
    }

    #[test]
    fn closed_state_blocks_open_serious_review_findings() {
        let mut state = base_state(MethodWorkStatus::Closed);
        state.review_findings[0].status = MethodReviewFindingStatus::Open;
        state.review_findings[0].severity = MethodReviewSeverity::High;
        state.closure = Some(MethodClosureState {
            closed_at_unix_seconds: 1_779_999_200,
            summary: "Open serious review finding".to_string(),
            evidence_ids: vec!["evidence:test".to_string()],
            review_finding_ids: vec!["finding:none".to_string()],
            closed_by: Some("codex".to_string()),
        });

        assert!(!state.is_closure_valid());

        state.review_findings[0].status = MethodReviewFindingStatus::AcceptedRisk;
        assert!(state.is_closure_valid());
    }

    #[test]
    fn closed_state_blocks_blocking_engine_alerts() {
        let mut state = base_state(MethodWorkStatus::Closed);
        state.closure = Some(MethodClosureState {
            closed_at_unix_seconds: 1_779_999_200,
            summary: "Alert remains blocked".to_string(),
            evidence_ids: vec!["evidence:test".to_string()],
            review_finding_ids: vec!["finding:none".to_string()],
            closed_by: Some("codex".to_string()),
        });
        state.engine_alerts.push(MethodEngineAlert {
            id: "alert:malicious".to_string(),
            summary: "Malicious alert".to_string(),
            severity: MethodEngineAlertSeverity::Malicious,
            action: MethodEngineAlertAction::Block,
            status: MethodEngineAlertStatus::Blocked,
            source_event: MethodEngineAlertSourceEvent {
                event_id: Some("aegis-code:tool_call:call-1".to_string()),
                category: Some("tool_call".to_string()),
                session_id: Some("session-1".to_string()),
                thread_id: Some("thread-1".to_string()),
                turn_id: Some("turn-1".to_string()),
                call_id: Some("call-1".to_string()),
                evidence_id: None,
                finding_id: None,
            },
            created_at_unix_seconds: 1_779_999_100,
            received_at_unix_seconds: 1_779_999_101,
            candidate_input_id: Some("candidate-input:alert:malicious".to_string()),
        });

        assert!(!state.is_closure_valid());
    }

    #[test]
    fn command_receipt_success_round_trips() {
        let receipt = receipt();
        assert_eq!(
            receipt.schema_version,
            METHOD_EVIDENCE_RECEIPT_SCHEMA_VERSION
        );
        assert_eq!(receipt.exit_status.exit_code, Some(0));
        assert!(!receipt.exit_status.timed_out);
        assert_eq!(receipt.output_summary, "test result: ok");

        let json = serde_json::to_string(&receipt).expect("serialize receipt");
        let round_tripped: MethodEvidenceReceipt =
            serde_json::from_str(&json).expect("deserialize receipt");
        assert_eq!(round_tripped, receipt);
    }

    #[test]
    fn command_receipt_failure_round_trips() {
        let mut receipt = receipt();
        receipt.exit_status.exit_code = Some(101);
        receipt.output_summary = "test failed".to_string();

        let json = serde_json::to_string(&receipt).expect("serialize receipt");
        let round_tripped: MethodEvidenceReceipt =
            serde_json::from_str(&json).expect("deserialize receipt");
        assert_eq!(round_tripped.exit_status.exit_code, Some(101));
        assert_eq!(round_tripped.output_summary, "test failed");
    }

    #[test]
    fn receipt_redaction_removes_obvious_secrets() {
        let (command, command_status) = redact_method_evidence_command(&[
            "gh".to_string(),
            "api".to_string(),
            "--token=abc123".to_string(),
            "--password".to_string(),
            "pw".to_string(),
        ]);
        let (output, output_status) =
            redact_method_evidence_output("Authorization: bearer abc123 test passed");

        assert_eq!(
            command,
            vec![
                "gh".to_string(),
                "api".to_string(),
                "--token=<redacted>".to_string(),
                "--password".to_string(),
                "<redacted>".to_string(),
            ]
        );
        assert_eq!(command_status, MethodEvidenceRedactionStatus::Redacted);
        assert_eq!(
            output,
            "<redacted> <redacted> <redacted> test passed".to_string()
        );
        assert_eq!(
            merge_method_evidence_redaction_status(command_status, output_status),
            MethodEvidenceRedactionStatus::Redacted
        );
    }

    #[test]
    fn receipt_missing_git_state_round_trips() {
        let mut receipt = receipt();
        receipt.git_state = MethodEvidenceGitState {
            status: MethodEvidenceGitStateStatus::Unavailable,
            repository: None,
            branch: None,
            commit: None,
            dirty: None,
            unavailable_reason: Some("not a git repository".to_string()),
        };

        let json = serde_json::to_string(&receipt).expect("serialize receipt");
        let round_tripped: MethodEvidenceReceipt =
            serde_json::from_str(&json).expect("deserialize receipt");
        assert_eq!(
            round_tripped.git_state.status,
            MethodEvidenceGitStateStatus::Unavailable
        );
        assert_eq!(
            round_tripped.git_state.unavailable_reason.as_deref(),
            Some("not a git repository")
        );
    }

    #[test]
    fn receipt_artifact_references_round_trip() {
        let mut receipt = receipt();
        receipt.artifacts = vec![
            MethodEvidenceArtifactRef {
                kind: MethodEvidenceArtifactKind::Path,
                value: "target/test.log".to_string(),
                digest: Some("sha256:abc".to_string()),
            },
            MethodEvidenceArtifactRef {
                kind: MethodEvidenceArtifactKind::Uri,
                value: "https://github.com/mithran-hq/aegis-code/actions/runs/1".to_string(),
                digest: None,
            },
        ];

        let json = serde_json::to_string(&receipt).expect("serialize receipt");
        let round_tripped: MethodEvidenceReceipt =
            serde_json::from_str(&json).expect("deserialize receipt");
        assert_eq!(round_tripped.artifacts, receipt.artifacts);
    }

    #[test]
    fn evidence_can_link_to_falsifiers() {
        let state = base_state(MethodWorkStatus::Incomplete);
        assert_eq!(
            state.evidence[0].falsifier_ids,
            vec!["falsifier:not-reusable".to_string()]
        );
    }

    #[test]
    fn older_evidence_without_receipt_fields_deserializes() {
        let mut value = serde_json::to_value(base_state(MethodWorkStatus::Incomplete))
            .expect("serialize method state");
        let evidence = value["evidence"][0]
            .as_object_mut()
            .expect("evidence is object");
        evidence.remove("falsifier_ids");
        evidence.remove("receipt");

        let state: MethodState = serde_json::from_value(value).expect("deserialize method state");
        assert!(state.evidence[0].falsifier_ids.is_empty());
        assert!(state.evidence[0].receipt.is_none());
    }

    #[test]
    fn resume_validity_accepts_matching_context() {
        let state = base_state(MethodWorkStatus::Incomplete);
        let report = state.compute_resume_validity(&resume_context());
        assert_eq!(report.status, MethodResumeValidityStatus::Valid);
        assert!(report.reasons.is_empty());
    }

    #[test]
    fn resume_validity_marks_branch_and_commit_changes_stale() {
        let state = base_state(MethodWorkStatus::Incomplete);
        let mut current = resume_context();
        current.branch = Some("feature".to_string());
        current.commit = Some("def456".to_string());

        let report = state.compute_resume_validity(&current);
        assert_eq!(report.status, MethodResumeValidityStatus::Stale);
        assert_eq!(
            report.reasons,
            vec![
                MethodResumeValidityReason::BranchChanged,
                MethodResumeValidityReason::CommitChanged,
            ]
        );
    }

    #[test]
    fn resume_validity_rejects_missing_or_mismatched_identity_context() {
        let mut state = base_state(MethodWorkStatus::Incomplete);
        state.resume_context.repository = None;
        let mut current = resume_context();
        current.linked_issue = Some(MethodIssueRef {
            provider: MethodIssueProvider::GitHub,
            repository: "mithran-hq/aegis-code".to_string(),
            number: 99,
        });

        let report = state.compute_resume_validity(&current);
        assert_eq!(report.status, MethodResumeValidityStatus::Invalid);
        assert_eq!(
            report.reasons,
            vec![
                MethodResumeValidityReason::MissingPersistedRepository,
                MethodResumeValidityReason::IssueMismatch,
            ]
        );
    }
}
