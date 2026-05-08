use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use codex_protocol::method_state::METHOD_EVIDENCE_RECEIPT_SCHEMA_VERSION;
use codex_protocol::method_state::MethodEvidence;
use codex_protocol::method_state::MethodEvidenceExitStatus;
use codex_protocol::method_state::MethodEvidenceReceipt;
use codex_protocol::method_state::MethodEvidenceSessionMetadata;
use codex_protocol::method_state::MethodReviewFinding;
use codex_protocol::method_state::MethodReviewFindingStatus;
use codex_protocol::method_state::MethodReviewSeverity;
use codex_protocol::method_state::MethodState;
use codex_protocol::method_state::redact_method_evidence_output;
use codex_protocol::protocol::ReviewFinding;
use codex_protocol::protocol::ReviewOutputEvent;
use codex_utils_absolute_path::AbsolutePathBuf;

use crate::method_evidence::capture_git_state;
use crate::review_format::render_review_output_text;

const REVIEW_OUTPUT_SUMMARY_MAX_BYTES: usize = 4096;

pub(crate) struct ReviewSessionSnapshot {
    pub(crate) turn_id: String,
    pub(crate) session_id: Option<String>,
    pub(crate) thread_id: Option<String>,
    pub(crate) provider: Option<String>,
    pub(crate) model: Option<String>,
}

pub(crate) async fn record_method_review_output(
    method_state: &mut MethodState,
    output: &ReviewOutputEvent,
    cwd: &AbsolutePathBuf,
    session: ReviewSessionSnapshot,
) {
    let captured_at_unix_seconds = now_unix_seconds();
    let git_state = capture_git_state(cwd.as_path()).await;
    let (output_summary, redaction_status) =
        redact_method_evidence_output(&truncate_output_summary(&render_review_output_text(output)));
    let evidence_id = format!("evidence:review:{}", sanitize_id_fragment(&session.turn_id));

    let receipt = MethodEvidenceReceipt {
        schema_version: METHOD_EVIDENCE_RECEIPT_SCHEMA_VERSION,
        command: vec!["aegis".to_string(), "review".to_string()],
        cwd: cwd.to_string_lossy().to_string(),
        captured_at_unix_seconds,
        git_state,
        exit_status: MethodEvidenceExitStatus {
            exit_code: Some(0),
            timed_out: false,
        },
        output_summary,
        artifacts: Vec::new(),
        session: MethodEvidenceSessionMetadata {
            session_id: session.session_id,
            thread_id: session.thread_id,
            provider: session.provider,
            model: session.model,
            sandbox_posture: None,
        },
        redaction_status,
    };

    apply_review_output_with_receipt(
        method_state,
        output,
        &evidence_id,
        captured_at_unix_seconds,
        receipt,
    );
}

pub(crate) fn method_review_prompt_context(method_state: &MethodState) -> String {
    let linked_issue = method_state
        .linked_issue
        .as_ref()
        .map(|issue| format!("{}#{}", issue.repository, issue.number))
        .unwrap_or_else(|| "none".to_string());
    let success_criteria = bullet_list(&method_state.intent.success_criteria);
    let falsifiers = bullet_list(
        &method_state
            .falsifiers
            .iter()
            .map(|falsifier| format!("{} ({:?})", falsifier.summary, falsifier.status))
            .collect::<Vec<_>>(),
    );
    let evidence_requirements = bullet_list(
        &method_state
            .evidence_requirements
            .iter()
            .filter(|requirement| requirement.required)
            .map(|requirement| requirement.summary.clone())
            .collect::<Vec<_>>(),
    );
    let closure_status = method_state
        .closure
        .as_ref()
        .map(|closure| closure.summary.as_str())
        .unwrap_or("not closed");

    format!(
        "\n\nAegis method context:\nLinked issue: {linked_issue}\nWork status: {:?}\nClosure: {closure_status}\nSuccess criteria:\n{success_criteria}\nFalsifiers:\n{falsifiers}\nRequired evidence:\n{evidence_requirements}\n",
        method_state.status
    )
}

pub(crate) fn sort_review_findings(findings: &mut [ReviewFinding]) {
    findings.sort_by_key(|finding| review_priority_rank(finding.priority));
}

fn apply_review_output_with_receipt(
    method_state: &mut MethodState,
    output: &ReviewOutputEvent,
    evidence_id: &str,
    captured_at_unix_seconds: i64,
    receipt: MethodEvidenceReceipt,
) {
    method_state
        .evidence
        .retain(|existing| existing.id != evidence_id);
    method_state.evidence.push(MethodEvidence {
        id: evidence_id.to_string(),
        summary: review_evidence_summary(output),
        kind: codex_protocol::method_state::MethodEvidenceKind::Other,
        requirement_ids: Vec::new(),
        claim_ids: Vec::new(),
        falsifier_ids: Vec::new(),
        source: Some("aegis review".to_string()),
        captured_at_unix_seconds,
        receipt: Some(receipt),
    });

    let finding_prefix = evidence_id.replacen("evidence:", "finding:", 1);
    method_state
        .review_findings
        .retain(|finding| !finding.id.starts_with(&finding_prefix));
    method_state
        .review_findings
        .extend(review_findings_for_output(
            output,
            &finding_prefix,
            evidence_id,
            captured_at_unix_seconds,
        ));
    method_state.provenance.updated_at_unix_seconds = captured_at_unix_seconds;
}

fn review_findings_for_output(
    output: &ReviewOutputEvent,
    finding_prefix: &str,
    evidence_id: &str,
    captured_at_unix_seconds: i64,
) -> Vec<MethodReviewFinding> {
    let mut findings = output.findings.clone();
    sort_review_findings(&mut findings);
    if findings.is_empty() {
        return vec![MethodReviewFinding {
            id: format!("{finding_prefix}:clean"),
            summary: clean_review_summary(output),
            severity: MethodReviewSeverity::Info,
            status: MethodReviewFindingStatus::Addressed,
            claim_ids: Vec::new(),
            evidence_ids: vec![evidence_id.to_string()],
            reviewed_at_unix_seconds: captured_at_unix_seconds,
            reviewer: Some("aegis review".to_string()),
        }];
    }

    findings
        .iter()
        .enumerate()
        .map(|(idx, finding)| MethodReviewFinding {
            id: format!("{finding_prefix}:{}", idx + 1),
            summary: review_finding_summary(finding),
            severity: review_severity(finding.priority),
            status: MethodReviewFindingStatus::Open,
            claim_ids: Vec::new(),
            evidence_ids: vec![evidence_id.to_string()],
            reviewed_at_unix_seconds: captured_at_unix_seconds,
            reviewer: Some("aegis review".to_string()),
        })
        .collect()
}

fn review_evidence_summary(output: &ReviewOutputEvent) -> String {
    if output.findings.is_empty() {
        "Aegis review completed with no actionable findings.".to_string()
    } else {
        format!(
            "Aegis review completed with {} actionable finding(s).",
            output.findings.len()
        )
    }
}

fn clean_review_summary(output: &ReviewOutputEvent) -> String {
    let explanation = output.overall_explanation.trim();
    if explanation.is_empty() {
        "Aegis review found no actionable findings.".to_string()
    } else {
        format!("Aegis review found no actionable findings: {explanation}")
    }
}

fn review_finding_summary(finding: &ReviewFinding) -> String {
    let path = finding.code_location.absolute_file_path.display();
    let start = finding.code_location.line_range.start;
    let end = finding.code_location.line_range.end;
    format!(
        "{} - {path}:{start}-{end}: {}",
        finding.title,
        finding.body.trim()
    )
}

fn review_severity(priority: i32) -> MethodReviewSeverity {
    match priority {
        0 => MethodReviewSeverity::Blocking,
        1 => MethodReviewSeverity::High,
        2 => MethodReviewSeverity::Medium,
        3 => MethodReviewSeverity::Low,
        _ => MethodReviewSeverity::Info,
    }
}

fn review_priority_rank(priority: i32) -> i32 {
    if (0..=3).contains(&priority) {
        priority
    } else {
        4
    }
}

fn bullet_list(items: &[String]) -> String {
    if items.is_empty() {
        return "- none".to_string();
    }
    items
        .iter()
        .map(|item| format!("- {item}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn truncate_output_summary(output: &str) -> String {
    if output.len() <= REVIEW_OUTPUT_SUMMARY_MAX_BYTES {
        return output.to_string();
    }
    let mut end = REVIEW_OUTPUT_SUMMARY_MAX_BYTES;
    while !output.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n[... review output truncated ...]", &output[..end])
}

fn sanitize_id_fragment(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    let trimmed = sanitized.trim_matches('-');
    if trimmed.is_empty() {
        "review".to_string()
    } else {
        trimmed.to_string()
    }
}

fn now_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use codex_protocol::method_state::METHOD_STATE_SCHEMA_VERSION;
    use codex_protocol::method_state::MethodEvidenceGitState;
    use codex_protocol::method_state::MethodEvidenceGitStateStatus;
    use codex_protocol::method_state::MethodEvidenceRedactionStatus;
    use codex_protocol::method_state::MethodIntent;
    use codex_protocol::method_state::MethodProvenance;
    use codex_protocol::method_state::MethodProvenanceSource;
    use codex_protocol::method_state::MethodResumeContext;
    use codex_protocol::method_state::MethodWorkStatus;
    use codex_protocol::protocol::ReviewCodeLocation;
    use codex_protocol::protocol::ReviewLineRange;

    use super::*;

    fn method_state() -> MethodState {
        MethodState {
            schema_version: METHOD_STATE_SCHEMA_VERSION,
            intent: MethodIntent {
                summary: "Ship review persistence".to_string(),
                success_criteria: vec!["Review findings are persisted".to_string()],
            },
            linked_issue: None,
            status: MethodWorkStatus::Incomplete,
            claims: Vec::new(),
            assumptions: Vec::new(),
            falsifiers: Vec::new(),
            evidence_requirements: Vec::new(),
            evidence: Vec::new(),
            gates: Vec::new(),
            engine_alerts: Vec::new(),
            review_findings: Vec::new(),
            closure: None,
            resume_context: MethodResumeContext {
                repository: Some("repo".to_string()),
                branch: Some("main".to_string()),
                commit: Some("abc123".to_string()),
                linked_issue: None,
                schema_version: Some(METHOD_STATE_SCHEMA_VERSION),
                sandbox_posture: None,
            },
            provenance: MethodProvenance {
                created_at_unix_seconds: 1,
                updated_at_unix_seconds: 1,
                source: MethodProvenanceSource::Agent,
                actor: Some("tester".to_string()),
            },
        }
    }

    fn finding(title: &str, priority: i32) -> ReviewFinding {
        ReviewFinding {
            title: title.to_string(),
            body: "Body".to_string(),
            confidence_score: 0.9,
            priority,
            code_location: ReviewCodeLocation {
                absolute_file_path: PathBuf::from("/repo/src/lib.rs"),
                line_range: ReviewLineRange { start: 1, end: 1 },
            },
        }
    }

    fn receipt() -> MethodEvidenceReceipt {
        MethodEvidenceReceipt {
            schema_version: METHOD_EVIDENCE_RECEIPT_SCHEMA_VERSION,
            command: vec!["aegis".to_string(), "review".to_string()],
            cwd: "/repo".to_string(),
            captured_at_unix_seconds: 2,
            git_state: MethodEvidenceGitState {
                status: MethodEvidenceGitStateStatus::Captured,
                repository: Some("repo".to_string()),
                branch: Some("main".to_string()),
                commit: Some("abc123".to_string()),
                dirty: Some(true),
                unavailable_reason: None,
            },
            exit_status: MethodEvidenceExitStatus {
                exit_code: Some(0),
                timed_out: false,
            },
            output_summary: "review".to_string(),
            artifacts: Vec::new(),
            session: MethodEvidenceSessionMetadata {
                session_id: Some("session".to_string()),
                thread_id: Some("thread".to_string()),
                provider: Some("provider".to_string()),
                model: Some("model".to_string()),
                sandbox_posture: None,
            },
            redaction_status: MethodEvidenceRedactionStatus::NotNeeded,
        }
    }

    #[test]
    fn risky_review_findings_are_ordered_and_mapped_by_severity() {
        let mut state = method_state();
        let output = ReviewOutputEvent {
            findings: vec![finding("P3", 3), finding("P1", 1), finding("P2", 2)],
            overall_correctness: "patch is incorrect".to_string(),
            overall_explanation: "Risky".to_string(),
            overall_confidence_score: 0.8,
        };

        apply_review_output_with_receipt(&mut state, &output, "evidence:review:turn", 2, receipt());

        assert_eq!(
            state
                .review_findings
                .iter()
                .map(|finding| finding.summary.as_str())
                .collect::<Vec<_>>(),
            vec![
                "P1 - /repo/src/lib.rs:1-1: Body",
                "P2 - /repo/src/lib.rs:1-1: Body",
                "P3 - /repo/src/lib.rs:1-1: Body",
            ]
        );
        assert_eq!(
            state.review_findings[0].severity,
            MethodReviewSeverity::High
        );
        assert_eq!(
            state.review_findings[1].severity,
            MethodReviewSeverity::Medium
        );
        assert_eq!(state.review_findings[2].severity, MethodReviewSeverity::Low);
        assert_eq!(
            state.review_findings[0].status,
            MethodReviewFindingStatus::Open
        );
        assert_eq!(
            state.review_findings[0].evidence_ids,
            vec!["evidence:review:turn".to_string()]
        );
    }

    #[test]
    fn clean_review_persists_addressed_info_finding() {
        let mut state = method_state();
        let output = ReviewOutputEvent {
            findings: Vec::new(),
            overall_correctness: "patch is correct".to_string(),
            overall_explanation: "No findings.".to_string(),
            overall_confidence_score: 0.9,
        };

        apply_review_output_with_receipt(&mut state, &output, "evidence:review:turn", 2, receipt());

        assert_eq!(state.evidence.len(), 1);
        assert_eq!(state.review_findings.len(), 1);
        assert_eq!(state.review_findings[0].id, "finding:review:turn:clean");
        assert_eq!(
            state.review_findings[0].severity,
            MethodReviewSeverity::Info
        );
        assert_eq!(
            state.review_findings[0].status,
            MethodReviewFindingStatus::Addressed
        );
    }

    #[test]
    fn out_of_scope_and_under_tested_reviews_persist_open_findings() {
        let mut state = method_state();
        let output = ReviewOutputEvent {
            findings: vec![
                finding("[P1] Out-of-scope file changed", 1),
                finding("[P2] Missing regression test", 2),
            ],
            overall_correctness: "patch is incorrect".to_string(),
            overall_explanation: "Scope and coverage risks remain.".to_string(),
            overall_confidence_score: 0.8,
        };

        apply_review_output_with_receipt(&mut state, &output, "evidence:review:turn", 2, receipt());

        assert_eq!(state.review_findings.len(), 2);
        assert!(
            state
                .review_findings
                .iter()
                .all(|finding| finding.status == MethodReviewFindingStatus::Open)
        );
        assert_eq!(
            state.review_findings[0].severity,
            MethodReviewSeverity::High
        );
        assert_eq!(
            state.review_findings[1].severity,
            MethodReviewSeverity::Medium
        );
    }
}
