#![allow(dead_code)]

use crate::state::MethodStatePersistenceStatus;
use codex_protocol::aegis_safety_event::AegisSafetyEvent;
use codex_protocol::aegis_safety_event::AegisSafetyEventCategory;
use codex_protocol::aegis_safety_event::AegisSafetyEventContext;
use codex_protocol::aegis_safety_event::AegisSafetyRedactionRule;
use codex_protocol::aegis_safety_event::AegisSafetySeverityHint;
use codex_protocol::aegis_safety_event::redact_safety_event_argv;
use codex_protocol::aegis_safety_event::redact_safety_event_text;
use codex_protocol::method_state::MethodEvidence;
use codex_protocol::method_state::MethodEvidenceKind;
use codex_protocol::method_state::MethodEvidenceRedactionStatus;
use codex_protocol::method_state::MethodResumeValidityReason;
use codex_protocol::method_state::MethodResumeValidityStatus;
use codex_protocol::method_state::MethodReviewFinding;
use codex_protocol::method_state::MethodReviewFindingStatus;
use codex_protocol::method_state::MethodReviewSeverity;
use codex_protocol::protocol::AegisPreflightDecisionEvent;
use codex_protocol::protocol::AegisPreflightVerdict;
use serde::Serialize;
use serde_json::Value;
use serde_json::json;

pub(crate) fn preflight_decision_event(event: &AegisPreflightDecisionEvent) -> AegisSafetyEvent {
    let (category, severity_hint) = match event.verdict {
        AegisPreflightVerdict::Allow => (
            AegisSafetyEventCategory::ToolCall,
            AegisSafetySeverityHint::Info,
        ),
        AegisPreflightVerdict::RequireConfirmation => (
            AegisSafetyEventCategory::MethodGate,
            AegisSafetySeverityHint::Medium,
        ),
        AegisPreflightVerdict::Block => (
            AegisSafetyEventCategory::ToolDenial,
            AegisSafetySeverityHint::High,
        ),
    };

    let mut tags = vec![
        format!("tool:{}", event.tool_name),
        format!("verdict:{}", json_string(event.verdict)),
    ];
    if let Some(risk_category) = event.risk_category {
        tags.push(format!("risk:{}", json_string(risk_category)));
    }
    tags.extend(
        event
            .required_evidence_ids
            .iter()
            .map(|id| format!("required_evidence:{id}")),
    );

    let mut context = AegisSafetyEventContext::new();
    context.insert("call_id".to_string(), json!(event.call_id));
    context.insert("turn_id".to_string(), json!(event.turn_id));
    context.insert("tool_name".to_string(), json!(event.tool_name));
    context.insert("verdict".to_string(), json!(json_string(event.verdict)));
    context.insert("reason".to_string(), json!(event.reason));
    context.insert(
        "required_evidence_ids".to_string(),
        json!(event.required_evidence_ids),
    );
    if let Some(risk_category) = event.risk_category {
        context.insert(
            "risk_category".to_string(),
            json!(json_string(risk_category)),
        );
    }
    if !event.paths.is_empty() {
        context.insert("paths".to_string(), json!(event.paths));
    }

    let mut redactions = Vec::new();
    if let Some(command) = &event.command {
        let (argv, command_redactions) = redact_safety_event_argv(command, "context.command.argv");
        context.insert("command".to_string(), json!({ "argv": argv }));
        redactions.extend(command_redactions);
    }

    AegisSafetyEvent::new(
        category,
        severity_hint,
        format!(
            "Aegis preflight {:?} for {}",
            event.verdict, event.tool_name
        ),
        tags,
        context,
        redactions,
    )
}

pub(crate) fn method_evidence_event(evidence: &MethodEvidence) -> AegisSafetyEvent {
    let severity_hint = match evidence.receipt.as_ref() {
        Some(receipt)
            if receipt.exit_status.exit_code == Some(0) && !receipt.exit_status.timed_out =>
        {
            AegisSafetySeverityHint::Info
        }
        Some(_) => AegisSafetySeverityHint::Medium,
        None => AegisSafetySeverityHint::Low,
    };

    let mut tags = vec![format!("evidence:{}", evidence_kind_tag(evidence.kind))];
    tags.extend(
        evidence
            .requirement_ids
            .iter()
            .map(|id| format!("requirement:{id}")),
    );

    let mut context = AegisSafetyEventContext::new();
    context.insert("evidence_id".to_string(), json!(evidence.id));
    context.insert("kind".to_string(), json!(json_string(evidence.kind)));
    context.insert(
        "requirement_ids".to_string(),
        json!(evidence.requirement_ids),
    );
    context.insert("claim_ids".to_string(), json!(evidence.claim_ids));
    context.insert("falsifier_ids".to_string(), json!(evidence.falsifier_ids));
    context.insert(
        "captured_at_unix_seconds".to_string(),
        json!(evidence.captured_at_unix_seconds),
    );
    if let Some(source) = &evidence.source {
        context.insert("evidence_source".to_string(), json!(source));
    }

    let mut redactions = Vec::new();
    if let Some(receipt) = &evidence.receipt {
        let (command, command_redactions) =
            redact_safety_event_argv(&receipt.command, "context.receipt.command");
        let (output_summary, output_redactions) =
            redact_safety_event_text(&receipt.output_summary, "context.receipt.output_summary");
        redactions.extend(command_redactions);
        redactions.extend(output_redactions);
        if receipt.redaction_status == MethodEvidenceRedactionStatus::Redacted {
            redactions.push(AegisSafetyRedactionRule::new(
                "context.receipt",
                "method evidence receipt was already redacted",
            ));
        }

        context.insert(
            "receipt".to_string(),
            json!({
                "command": command,
                "cwd": receipt.cwd,
                "exit_status": receipt.exit_status,
                "output_summary": output_summary,
                "artifacts": receipt.artifacts,
                "session": receipt.session,
                "redaction_status": receipt.redaction_status,
            }),
        );
    }

    AegisSafetyEvent::new(
        AegisSafetyEventCategory::Evidence,
        severity_hint,
        evidence.summary.clone(),
        tags,
        context,
        redactions,
    )
}

pub(crate) fn resume_status_event(status: &MethodStatePersistenceStatus) -> AegisSafetyEvent {
    match status {
        MethodStatePersistenceStatus::Missing => simple_event(
            AegisSafetyEventCategory::Resume,
            AegisSafetySeverityHint::Low,
            "No persisted Aegis method state was loaded",
            vec!["resume:missing".to_string()],
            AegisSafetyEventContext::new(),
        ),
        MethodStatePersistenceStatus::Invalid { diagnostic } => {
            let mut context = AegisSafetyEventContext::new();
            context.insert("diagnostic".to_string(), json!(diagnostic.message));
            simple_event(
                AegisSafetyEventCategory::Resume,
                AegisSafetySeverityHint::High,
                "Persisted Aegis method state is invalid",
                vec!["resume:invalid".to_string()],
                context,
            )
        }
        MethodStatePersistenceStatus::Loaded {
            state,
            resume_validity,
        } => {
            let severity_hint = match resume_validity.status {
                MethodResumeValidityStatus::Valid => AegisSafetySeverityHint::Info,
                MethodResumeValidityStatus::Stale => AegisSafetySeverityHint::Low,
                MethodResumeValidityStatus::Invalid => AegisSafetySeverityHint::High,
            };
            let mut context = AegisSafetyEventContext::new();
            context.insert(
                "status".to_string(),
                json!(json_string(resume_validity.status)),
            );
            context.insert("reasons".to_string(), json!(resume_validity.reasons));
            context.insert(
                "method_status".to_string(),
                json!(json_string(state.status)),
            );
            if let Some(issue) = &state.linked_issue {
                context.insert(
                    "linked_issue".to_string(),
                    json!({
                        "provider": issue.provider,
                        "repository": issue.repository,
                        "number": issue.number,
                    }),
                );
            }
            let reason_tags = resume_validity
                .reasons
                .iter()
                .map(|reason| format!("resume_reason:{}", resume_reason_tag(*reason)));
            simple_event(
                AegisSafetyEventCategory::Resume,
                severity_hint,
                "Loaded persisted Aegis method state",
                std::iter::once(format!("resume:{}", json_string(resume_validity.status)))
                    .chain(reason_tags)
                    .collect(),
                context,
            )
        }
    }
}

pub(crate) fn review_finding_event(finding: &MethodReviewFinding) -> AegisSafetyEvent {
    let severity_hint = match finding.severity {
        MethodReviewSeverity::Info => AegisSafetySeverityHint::Info,
        MethodReviewSeverity::Low => AegisSafetySeverityHint::Low,
        MethodReviewSeverity::Medium => AegisSafetySeverityHint::Medium,
        MethodReviewSeverity::High => AegisSafetySeverityHint::High,
        MethodReviewSeverity::Blocking => AegisSafetySeverityHint::Critical,
    };

    let mut context = AegisSafetyEventContext::new();
    context.insert("finding_id".to_string(), json!(finding.id));
    context.insert("severity".to_string(), json!(json_string(finding.severity)));
    context.insert("status".to_string(), json!(json_string(finding.status)));
    context.insert("claim_ids".to_string(), json!(finding.claim_ids));
    context.insert("evidence_ids".to_string(), json!(finding.evidence_ids));
    context.insert(
        "reviewed_at_unix_seconds".to_string(),
        json!(finding.reviewed_at_unix_seconds),
    );
    if let Some(reviewer) = &finding.reviewer {
        context.insert("reviewer".to_string(), json!(reviewer));
    }

    AegisSafetyEvent::new(
        AegisSafetyEventCategory::Review,
        severity_hint,
        finding.summary.clone(),
        vec![
            format!("review:{}", review_severity_tag(finding.severity)),
            format!("review_status:{}", review_status_tag(finding.status)),
        ],
        context,
        Vec::new(),
    )
}

pub(crate) fn provider_event(
    summary: impl Into<String>,
    provider: impl Into<String>,
    model: impl Into<String>,
    severity_hint: AegisSafetySeverityHint,
) -> AegisSafetyEvent {
    let provider = provider.into();
    let model = model.into();
    let mut context = AegisSafetyEventContext::new();
    context.insert("provider".to_string(), json!(provider));
    context.insert("model".to_string(), json!(model));
    simple_event(
        AegisSafetyEventCategory::Provider,
        severity_hint,
        summary,
        vec!["provider:selected".to_string()],
        context,
    )
}

pub(crate) fn sandbox_event(
    summary: impl Into<String>,
    sandbox_mode: impl Into<String>,
    permission_profile: Option<String>,
    severity_hint: AegisSafetySeverityHint,
) -> AegisSafetyEvent {
    let sandbox_mode = sandbox_mode.into();
    let mut context = AegisSafetyEventContext::new();
    context.insert("sandbox_mode".to_string(), json!(sandbox_mode));
    if let Some(permission_profile) = permission_profile {
        context.insert("permission_profile".to_string(), json!(permission_profile));
    }
    simple_event(
        AegisSafetyEventCategory::Sandbox,
        severity_hint,
        summary,
        vec!["sandbox:posture".to_string()],
        context,
    )
}

pub(crate) fn runtime_checkpoint_event(
    thread_id: impl Into<String>,
    checkpoint_id: impl Into<String>,
    label: impl Into<String>,
) -> AegisSafetyEvent {
    let mut context = AegisSafetyEventContext::new();
    context.insert("thread_id".to_string(), json!(thread_id.into()));
    context.insert("checkpoint_id".to_string(), json!(checkpoint_id.into()));
    context.insert("label".to_string(), json!(label.into()));
    simple_event(
        AegisSafetyEventCategory::Runtime,
        AegisSafetySeverityHint::Info,
        "Aegis Agent Runtime checkpoint",
        vec!["runtime:checkpoint".to_string()],
        context,
    )
}

fn simple_event(
    category: AegisSafetyEventCategory,
    severity_hint: AegisSafetySeverityHint,
    summary: impl Into<String>,
    tags: Vec<String>,
    context: AegisSafetyEventContext,
) -> AegisSafetyEvent {
    AegisSafetyEvent::new(category, severity_hint, summary, tags, context, Vec::new())
}

fn evidence_kind_tag(kind: MethodEvidenceKind) -> &'static str {
    match kind {
        MethodEvidenceKind::Command => "command",
        MethodEvidenceKind::Test => "test",
        MethodEvidenceKind::File => "file",
        MethodEvidenceKind::Commit => "commit",
        MethodEvidenceKind::GitHub => "github",
        MethodEvidenceKind::Human => "human",
        MethodEvidenceKind::Other => "other",
    }
}

fn resume_reason_tag(reason: MethodResumeValidityReason) -> &'static str {
    match reason {
        MethodResumeValidityReason::MissingPersistedRepository => "missing_persisted_repository",
        MethodResumeValidityReason::MissingCurrentRepository => "missing_current_repository",
        MethodResumeValidityReason::RepositoryMismatch => "repository_mismatch",
        MethodResumeValidityReason::MissingPersistedBranch => "missing_persisted_branch",
        MethodResumeValidityReason::MissingCurrentBranch => "missing_current_branch",
        MethodResumeValidityReason::BranchChanged => "branch_changed",
        MethodResumeValidityReason::MissingPersistedCommit => "missing_persisted_commit",
        MethodResumeValidityReason::MissingCurrentCommit => "missing_current_commit",
        MethodResumeValidityReason::CommitChanged => "commit_changed",
        MethodResumeValidityReason::MissingPersistedIssue => "missing_persisted_issue",
        MethodResumeValidityReason::MissingCurrentIssue => "missing_current_issue",
        MethodResumeValidityReason::IssueMismatch => "issue_mismatch",
        MethodResumeValidityReason::MissingPersistedSchemaVersion => {
            "missing_persisted_schema_version"
        }
        MethodResumeValidityReason::MissingCurrentSchemaVersion => "missing_current_schema_version",
        MethodResumeValidityReason::SchemaVersionMismatch => "schema_version_mismatch",
    }
}

fn review_severity_tag(severity: MethodReviewSeverity) -> &'static str {
    match severity {
        MethodReviewSeverity::Info => "info",
        MethodReviewSeverity::Low => "low",
        MethodReviewSeverity::Medium => "medium",
        MethodReviewSeverity::High => "high",
        MethodReviewSeverity::Blocking => "blocking",
    }
}

fn review_status_tag(status: MethodReviewFindingStatus) -> &'static str {
    match status {
        MethodReviewFindingStatus::Open => "open",
        MethodReviewFindingStatus::Addressed => "addressed",
        MethodReviewFindingStatus::AcceptedRisk => "accepted_risk",
    }
}

fn json_string<T>(value: T) -> String
where
    T: Serialize,
{
    match serde_json::to_value(value).expect("enum serializes") {
        Value::String(value) => value,
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::MethodStatePersistenceDiagnostic;
    use codex_protocol::aegis_safety_event::AEGIS_SAFETY_EVENT_SOURCE_TAG;
    use codex_protocol::aegis_secret_policy::AegisSecretRiskCategory;
    use codex_protocol::method_state::METHOD_EVIDENCE_RECEIPT_SCHEMA_VERSION;
    use codex_protocol::method_state::METHOD_STATE_SCHEMA_VERSION;
    use codex_protocol::method_state::MethodEvidenceArtifactKind;
    use codex_protocol::method_state::MethodEvidenceArtifactRef;
    use codex_protocol::method_state::MethodEvidenceExitStatus;
    use codex_protocol::method_state::MethodEvidenceGitState;
    use codex_protocol::method_state::MethodEvidenceGitStateStatus;
    use codex_protocol::method_state::MethodEvidenceReceipt;
    use codex_protocol::method_state::MethodEvidenceSessionMetadata;
    use codex_protocol::method_state::MethodIntent;
    use codex_protocol::method_state::MethodLinkedIssue;
    use codex_protocol::method_state::MethodProvenance;
    use codex_protocol::method_state::MethodProvenanceSource;
    use codex_protocol::method_state::MethodResumeContext;
    use codex_protocol::method_state::MethodResumeValidityReport;
    use codex_protocol::method_state::MethodState;
    use codex_protocol::method_state::MethodWorkStatus;
    use pretty_assertions::assert_eq;

    #[test]
    fn preflight_block_maps_to_tool_denial_with_redacted_command() {
        let event = preflight_decision_event(&AegisPreflightDecisionEvent {
            call_id: "call-1".to_string(),
            turn_id: "turn-1".to_string(),
            tool_name: "exec_command".to_string(),
            verdict: AegisPreflightVerdict::Block,
            risk_category: Some(AegisSecretRiskCategory::RepositoryMutation),
            reason: "missing linked task".to_string(),
            required_evidence_ids: vec!["task-scope".to_string()],
            command: Some(vec![
                "gh".to_string(),
                "api".to_string(),
                "--token".to_string(),
                "secret-value".to_string(),
            ]),
            paths: Vec::new(),
        });

        let value = serde_json::to_value(&event).expect("serialize event");
        assert_eq!(value["source"], AEGIS_SAFETY_EVENT_SOURCE_TAG);
        assert_eq!(event.category, AegisSafetyEventCategory::ToolDenial);
        assert_eq!(event.severity_hint, AegisSafetySeverityHint::High);
        assert!(event.tags.contains(&"verdict:block".to_string()));
        assert!(event.tags.contains(&"risk:repository_mutation".to_string()));
        assert_eq!(
            value["context"]["command"]["argv"],
            json!(["gh", "api", "--token", "<redacted>"])
        );
        assert_eq!(event.redactions[0].field_path, "context.command.argv[3]");
        assert!(
            !serde_json::to_string(&event)
                .expect("serialize event")
                .contains("secret-value")
        );
    }

    #[test]
    fn preflight_allow_and_confirmation_use_distinct_categories() {
        let mut decision = AegisPreflightDecisionEvent {
            call_id: "call-1".to_string(),
            turn_id: "turn-1".to_string(),
            tool_name: "apply_patch".to_string(),
            verdict: AegisPreflightVerdict::Allow,
            risk_category: None,
            reason: "allowed".to_string(),
            required_evidence_ids: Vec::new(),
            command: None,
            paths: vec!["/repo/src/lib.rs".to_string()],
        };

        assert_eq!(
            preflight_decision_event(&decision).category,
            AegisSafetyEventCategory::ToolCall
        );
        decision.verdict = AegisPreflightVerdict::RequireConfirmation;
        assert_eq!(
            preflight_decision_event(&decision).category,
            AegisSafetyEventCategory::MethodGate
        );
    }

    #[test]
    fn method_evidence_maps_receipts_and_redacts_output() {
        let evidence = MethodEvidence {
            id: "evidence:test:call-1".to_string(),
            summary: "cargo test passed".to_string(),
            kind: MethodEvidenceKind::Test,
            requirement_ids: vec!["evidence:tests".to_string()],
            claim_ids: vec!["claim:tests".to_string()],
            falsifier_ids: Vec::new(),
            source: Some("harness exec_command".to_string()),
            captured_at_unix_seconds: 1,
            receipt: Some(MethodEvidenceReceipt {
                schema_version: METHOD_EVIDENCE_RECEIPT_SCHEMA_VERSION,
                command: vec!["cargo".to_string(), "test".to_string()],
                cwd: "/repo".to_string(),
                captured_at_unix_seconds: 1,
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
                output_summary: "Authorization Bearer secret-value test ok".to_string(),
                artifacts: vec![MethodEvidenceArtifactRef {
                    kind: MethodEvidenceArtifactKind::Uri,
                    value: "https://example.test/run".to_string(),
                    digest: None,
                }],
                session: MethodEvidenceSessionMetadata {
                    session_id: Some("session-1".to_string()),
                    thread_id: Some("thread-1".to_string()),
                    provider: Some("openai".to_string()),
                    model: Some("gpt".to_string()),
                },
                redaction_status: MethodEvidenceRedactionStatus::NotNeeded,
            }),
        };

        let event = method_evidence_event(&evidence);
        assert_eq!(event.category, AegisSafetyEventCategory::Evidence);
        assert!(event.tags.contains(&"evidence:test".to_string()));
        assert!(
            !serde_json::to_string(&event)
                .expect("serialize event")
                .contains("secret-value")
        );
        assert!(!event.redactions.is_empty());
    }

    #[test]
    fn invalid_resume_status_maps_to_high_resume_event() {
        let event = resume_status_event(&MethodStatePersistenceStatus::Invalid {
            diagnostic: MethodStatePersistenceDiagnostic {
                message: "corrupt method state".to_string(),
            },
        });

        assert_eq!(event.category, AegisSafetyEventCategory::Resume);
        assert_eq!(event.severity_hint, AegisSafetySeverityHint::High);
        assert!(event.tags.contains(&"resume:invalid".to_string()));
    }

    #[test]
    fn stale_resume_reasons_are_stable_tags() {
        let event = resume_status_event(&MethodStatePersistenceStatus::Loaded {
            state: method_state(),
            resume_validity: MethodResumeValidityReport {
                status: MethodResumeValidityStatus::Stale,
                reasons: vec![MethodResumeValidityReason::BranchChanged],
            },
        });

        assert!(event.tags.contains(&"resume:stale".to_string()));
        assert!(
            event
                .tags
                .contains(&"resume_reason:branch_changed".to_string())
        );
    }

    #[test]
    fn review_finding_severity_maps_to_safety_severity() {
        let event = review_finding_event(&MethodReviewFinding {
            id: "finding:review:1".to_string(),
            summary: "Blocking issue".to_string(),
            severity: MethodReviewSeverity::Blocking,
            status: MethodReviewFindingStatus::Open,
            claim_ids: Vec::new(),
            evidence_ids: vec!["evidence:review:1".to_string()],
            reviewed_at_unix_seconds: 1,
            reviewer: Some("aegis review".to_string()),
        });

        assert_eq!(event.category, AegisSafetyEventCategory::Review);
        assert_eq!(event.severity_hint, AegisSafetySeverityHint::Critical);
        assert!(event.tags.contains(&"review:blocking".to_string()));
        assert!(event.tags.contains(&"review_status:open".to_string()));
    }

    #[test]
    fn provider_sandbox_and_runtime_constructors_cover_remaining_families() {
        let provider = provider_event(
            "Selected provider",
            "openai",
            "gpt-5.2",
            AegisSafetySeverityHint::Info,
        );
        let sandbox = sandbox_event(
            "Sandbox posture selected",
            "workspace-write",
            Some("on-request".to_string()),
            AegisSafetySeverityHint::Info,
        );
        let runtime = runtime_checkpoint_event("thread-1", "checkpoint-1", "after tests");

        assert_eq!(provider.category, AegisSafetyEventCategory::Provider);
        assert_eq!(sandbox.category, AegisSafetyEventCategory::Sandbox);
        assert_eq!(runtime.category, AegisSafetyEventCategory::Runtime);
        assert!(runtime.tags.contains(&"runtime:checkpoint".to_string()));
    }

    fn method_state() -> MethodState {
        MethodState {
            schema_version: METHOD_STATE_SCHEMA_VERSION,
            intent: MethodIntent {
                summary: "Implement task".to_string(),
                success_criteria: vec!["tests pass".to_string()],
            },
            linked_issue: Some(MethodLinkedIssue {
                provider: codex_protocol::method_state::MethodIssueProvider::GitHub,
                repository: "mithran-hq/aegis-code".to_string(),
                number: 23,
                title: Some("Task: Define Aegis Code runtime event schema".to_string()),
                url: None,
            }),
            status: MethodWorkStatus::Incomplete,
            claims: Vec::new(),
            assumptions: Vec::new(),
            falsifiers: Vec::new(),
            evidence_requirements: Vec::new(),
            evidence: Vec::new(),
            gates: Vec::new(),
            review_findings: Vec::new(),
            closure: None,
            resume_context: MethodResumeContext {
                repository: Some("mithran-hq/aegis-code".to_string()),
                branch: Some("master".to_string()),
                commit: Some("abc123".to_string()),
                linked_issue: None,
                schema_version: Some(METHOD_STATE_SCHEMA_VERSION),
            },
            provenance: MethodProvenance {
                created_at_unix_seconds: 1,
                updated_at_unix_seconds: 1,
                source: MethodProvenanceSource::Agent,
                actor: Some("codex".to_string()),
            },
        }
    }
}
