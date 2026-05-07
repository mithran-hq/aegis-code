//! Typed Aegis method state shared by runtime, persistence, and UI surfaces.

use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use ts_rs::TS;

pub const METHOD_STATE_SCHEMA_VERSION: u32 = 1;

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
                    let cited_by_closure = closure_evidence_ids
                        .map(|ids| ids.iter().any(|id| id == &evidence.id))
                        .unwrap_or(true);
                    satisfies_requirement && cited_by_closure
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
        closure.evidence_ids.iter().all(|id| {
            self.evidence
                .iter()
                .any(|evidence| evidence.id.as_str() == id.as_str())
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source: Option<String>,
    pub captured_at_unix_seconds: i64,
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
            }],
            evidence: vec![MethodEvidence {
                id: "evidence:test".to_string(),
                summary: "cargo test -p codex-protocol method_state passed".to_string(),
                kind: MethodEvidenceKind::Test,
                requirement_ids: vec!["requirement:serialization".to_string()],
                claim_ids: vec!["claim:model-exists".to_string()],
                source: Some("local".to_string()),
                captured_at_unix_seconds: 1_779_999_000,
            }],
            gates: vec![MethodGate {
                id: "gate:local-ci".to_string(),
                name: "Local CI".to_string(),
                status: MethodGateStatus::Pending,
                evidence_requirement_ids: vec!["requirement:serialization".to_string()],
                rationale: None,
            }],
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
    fn closed_state_requires_closure_referenced_evidence() {
        let mut state = base_state(MethodWorkStatus::Closed);
        state.evidence.push(MethodEvidence {
            id: "evidence:unrelated".to_string(),
            summary: "Unrelated command output".to_string(),
            kind: MethodEvidenceKind::Command,
            requirement_ids: Vec::new(),
            claim_ids: Vec::new(),
            source: Some("local".to_string()),
            captured_at_unix_seconds: 1_779_999_050,
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
