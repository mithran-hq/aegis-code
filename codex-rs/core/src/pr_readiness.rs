use codex_protocol::method_state::MethodEvidence;
use codex_protocol::method_state::MethodEvidenceGitStateStatus;
use codex_protocol::method_state::MethodFalsifierStatus;
use codex_protocol::method_state::MethodIssueProvider;
use codex_protocol::method_state::MethodReviewFindingStatus;
use codex_protocol::method_state::MethodReviewSeverity;
use codex_protocol::method_state::MethodState;
use codex_protocol::method_state::MethodWorkStatus;
use regex_lite::Regex;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::collections::BTreeSet;

use crate::issue_train::FindingSeverity;
use crate::issue_train::IssueSnapshot;
use crate::issue_train::IssueTrainSnapshot;
use crate::issue_train::validate_issue_train;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullRequestSnapshot {
    pub number: u64,
    pub title: String,
    pub body: String,
    pub head_sha: String,
    pub base_ref: String,
    pub head_ref: String,
    pub changed_files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrReadinessSnapshot {
    pub repository: String,
    pub pull_request: PullRequestSnapshot,
    pub linked_issue: Option<IssueSnapshot>,
    pub parent_issue: Option<IssueSnapshot>,
    pub child_issues: Vec<IssueSnapshot>,
    pub method_state: Option<MethodState>,
    pub allowed_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrReadinessFinding {
    pub severity: FindingSeverity,
    pub code: String,
    pub subject: Option<String>,
    pub message: String,
    pub remediation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrReadinessReport {
    pub valid: bool,
    pub pr_number: u64,
    pub linked_issue_number: Option<u64>,
    pub changed_file_count: usize,
    pub findings: Vec<PrReadinessFinding>,
}

pub fn validate_pr_readiness(snapshot: &PrReadinessSnapshot) -> PrReadinessReport {
    let closing_refs = parse_closing_issue_refs(&snapshot.pull_request.body);
    let linked_issue_number = if closing_refs.len() == 1 {
        closing_refs.first().copied()
    } else {
        None
    };
    let mut findings = Vec::new();

    validate_linkage(snapshot, &closing_refs, &mut findings);
    validate_allowed_paths(snapshot, &mut findings);
    validate_method_state(snapshot, linked_issue_number, &mut findings);
    validate_issue_train_context(snapshot, linked_issue_number, &mut findings);

    let valid = !findings
        .iter()
        .any(|finding| finding.severity == FindingSeverity::Error);

    PrReadinessReport {
        valid,
        pr_number: snapshot.pull_request.number,
        linked_issue_number,
        changed_file_count: snapshot.pull_request.changed_files.len(),
        findings,
    }
}

pub fn parse_closing_issue_refs(body: &str) -> Vec<u64> {
    let regex = Regex::new(
        r"(?i)\b(?:fixes|closes|resolves)\s+(?:[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+)?#([0-9]+)\b",
    )
    .expect("closing issue regex should compile");
    regex
        .captures_iter(body)
        .filter_map(|captures| captures.get(1))
        .filter_map(|capture| capture.as_str().parse::<u64>().ok())
        .collect()
}

pub fn parse_allowed_paths_from_pr_body(body: &str) -> Vec<String> {
    let Some(section) = section_content_raw(body, "Allowed Paths") else {
        return Vec::new();
    };
    section
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            let path = trimmed
                .strip_prefix("- ")
                .or_else(|| trimmed.strip_prefix("* "))?
                .trim()
                .trim_matches('`')
                .trim();
            (!path.is_empty()).then(|| path.to_string())
        })
        .collect()
}

fn validate_linkage(
    snapshot: &PrReadinessSnapshot,
    closing_refs: &[u64],
    findings: &mut Vec<PrReadinessFinding>,
) {
    match closing_refs {
        [] => push_error(
            findings,
            "missing_closing_issue_ref",
            Some(format!("pr:{}", snapshot.pull_request.number)),
            "Pull request body does not contain a closing task issue reference.",
            "Add exactly one `Fixes #N`, `Closes #N`, or `Resolves #N` reference.",
        ),
        [_] => {}
        refs => push_error(
            findings,
            "multiple_closing_issue_refs",
            Some(format!("pr:{}", snapshot.pull_request.number)),
            &format!("Pull request body contains multiple closing issue references: {refs:?}."),
            "Keep one PR scoped to one child task issue.",
        ),
    }

    if snapshot.linked_issue.is_none() && closing_refs.len() == 1 {
        push_error(
            findings,
            "linked_issue_unavailable",
            closing_refs.first().map(|issue| format!("issue:{issue}")),
            "Linked task issue could not be loaded.",
            "Ensure the referenced issue exists and is accessible to the validator.",
        );
    }
}

fn validate_allowed_paths(snapshot: &PrReadinessSnapshot, findings: &mut Vec<PrReadinessFinding>) {
    let allowed_paths = normalized_allowed_paths(&snapshot.allowed_paths);
    if allowed_paths.is_empty() {
        push_error(
            findings,
            "missing_allowed_paths",
            Some(format!("pr:{}", snapshot.pull_request.number)),
            "No allowed path prefixes were provided for scope validation.",
            "Pass `--allowed-path` or add a PR body `## Allowed Paths` section with bullet entries.",
        );
        return;
    }

    for file in &snapshot.pull_request.changed_files {
        if !allowed_paths
            .iter()
            .any(|allowed| path_matches_allowed_prefix(file, allowed))
        {
            push_error(
                findings,
                "changed_file_outside_allowed_paths",
                Some(file.clone()),
                &format!("Changed file `{file}` is outside the allowed PR scope."),
                "Add the path prefix intentionally or split the out-of-scope change into its own issue.",
            );
        }
    }
}

fn validate_method_state(
    snapshot: &PrReadinessSnapshot,
    linked_issue_number: Option<u64>,
    findings: &mut Vec<PrReadinessFinding>,
) {
    let Some(method_state) = &snapshot.method_state else {
        push_error(
            findings,
            "missing_method_state",
            Some(format!("pr:{}", snapshot.pull_request.number)),
            "No method-state JSON artifact was provided.",
            "Run the validator with `--method-state <path>` from CI.",
        );
        return;
    };

    match (&method_state.linked_issue, linked_issue_number) {
        (Some(linked), Some(issue_number))
            if linked.provider == MethodIssueProvider::GitHub
                && linked.repository.eq_ignore_ascii_case(&snapshot.repository)
                && linked.number == issue_number => {}
        (Some(_), Some(issue_number)) => push_error(
            findings,
            "method_state_issue_mismatch",
            Some(format!("issue:{issue_number}")),
            "Method state linked issue does not match the PR closing issue.",
            "Use the method state artifact produced for this exact child task issue.",
        ),
        (None, Some(issue_number)) => push_error(
            findings,
            "method_state_missing_linked_issue",
            Some(format!("issue:{issue_number}")),
            "Method state does not record the linked issue.",
            "Record method state with the same GitHub child task issue that the PR closes.",
        ),
        _ => {}
    }

    if method_state.status != MethodWorkStatus::Closed {
        push_error(
            findings,
            "method_state_not_closed",
            Some("method_state".to_string()),
            "Method state is not closed.",
            "Close the method state only after all required evidence and review criteria are satisfied.",
        );
    }

    if !method_state.is_closure_valid() {
        push_error(
            findings,
            "invalid_closure_evidence",
            Some("method_state".to_string()),
            "Method state closure is missing successful required evidence.",
            "Ensure closure cites successful receipts for every required evidence requirement.",
        );
    }

    for gap in method_state.closure_evidence_gaps() {
        push_error(
            findings,
            "missing_required_evidence",
            Some(gap.id.clone()),
            &format!(
                "Required evidence `{}` is not satisfied by closure receipts.",
                gap.id
            ),
            "Run the required command and cite its successful evidence id in closure.",
        );
    }

    validate_falsifiers(method_state, findings);
    validate_review_findings(method_state, findings);
    validate_closure_receipts(method_state, &snapshot.pull_request.head_sha, findings);
}

fn validate_falsifiers(method_state: &MethodState, findings: &mut Vec<PrReadinessFinding>) {
    let evidence_by_id = evidence_by_id(method_state);
    for falsifier in &method_state.falsifiers {
        match falsifier.status {
            MethodFalsifierStatus::Open => push_error(
                findings,
                "open_falsifier",
                Some(falsifier.id.clone()),
                &format!("Falsifier `{}` is still open.", falsifier.id),
                "Disprove or confirm the falsifier before marking the PR ready.",
            ),
            MethodFalsifierStatus::Confirmed => push_error(
                findings,
                "confirmed_falsifier",
                Some(falsifier.id.clone()),
                &format!("Falsifier `{}` is confirmed.", falsifier.id),
                "Fix the implementation or explicitly split/block the work instead of closing the PR.",
            ),
            MethodFalsifierStatus::Disproved => {
                let has_successful_evidence = falsifier.evidence_ids.iter().any(|evidence_id| {
                    evidence_by_id
                        .get(evidence_id.as_str())
                        .is_some_and(|evidence| evidence.has_successful_receipt())
                });
                if !has_successful_evidence {
                    push_error(
                        findings,
                        "falsifier_missing_successful_evidence",
                        Some(falsifier.id.clone()),
                        &format!(
                            "Disproved falsifier `{}` has no successful evidence receipt.",
                            falsifier.id
                        ),
                        "Attach a successful evidence receipt to the falsifier.",
                    );
                }
            }
        }
    }
}

fn validate_review_findings(method_state: &MethodState, findings: &mut Vec<PrReadinessFinding>) {
    for finding in &method_state.review_findings {
        if finding.status != MethodReviewFindingStatus::Open {
            continue;
        }
        match finding.severity {
            MethodReviewSeverity::Blocking
            | MethodReviewSeverity::High
            | MethodReviewSeverity::Medium => push_error(
                findings,
                "open_blocking_review_finding",
                Some(finding.id.clone()),
                &format!("Review finding `{}` is still open.", finding.id),
                "Address the finding or mark it as accepted risk before closing the PR.",
            ),
            MethodReviewSeverity::Low | MethodReviewSeverity::Info => push_warning(
                findings,
                "open_nonblocking_review_finding",
                Some(finding.id.clone()),
                &format!(
                    "Non-blocking review finding `{}` is still open.",
                    finding.id
                ),
                "Consider addressing the finding before merging.",
            ),
        }
    }
}

fn validate_closure_receipts(
    method_state: &MethodState,
    head_sha: &str,
    findings: &mut Vec<PrReadinessFinding>,
) {
    let Some(closure) = &method_state.closure else {
        push_error(
            findings,
            "missing_closure",
            Some("method_state".to_string()),
            "Method state has no closure record.",
            "Record closure with the evidence ids used to justify PR readiness.",
        );
        return;
    };
    let evidence_by_id = evidence_by_id(method_state);

    for evidence_id in &closure.evidence_ids {
        let Some(evidence) = evidence_by_id.get(evidence_id.as_str()) else {
            push_error(
                findings,
                "closure_cites_missing_evidence",
                Some(evidence_id.clone()),
                &format!("Closure cites missing evidence `{evidence_id}`."),
                "Remove stale evidence ids or include the corresponding evidence record.",
            );
            continue;
        };
        let Some(receipt) = evidence.receipt.as_ref() else {
            push_error(
                findings,
                "closure_evidence_missing_receipt",
                Some(evidence_id.clone()),
                &format!("Closure evidence `{evidence_id}` has no receipt."),
                "Use a captured command receipt rather than free-form evidence.",
            );
            continue;
        };
        if receipt.git_state.status != MethodEvidenceGitStateStatus::Captured {
            push_error(
                findings,
                "receipt_git_state_unavailable",
                Some(evidence_id.clone()),
                &format!("Receipt for `{evidence_id}` has no captured git state."),
                "Re-run the evidence command in the PR checkout.",
            );
        }
        if receipt.git_state.commit.as_deref() != Some(head_sha) {
            push_error(
                findings,
                "receipt_commit_mismatch",
                Some(evidence_id.clone()),
                &format!("Receipt for `{evidence_id}` was not captured at the PR head commit."),
                "Re-run the evidence command after checking out the PR head SHA.",
            );
        }
        if receipt.git_state.dirty != Some(false) {
            push_error(
                findings,
                "receipt_dirty_git_state",
                Some(evidence_id.clone()),
                &format!("Receipt for `{evidence_id}` was not captured from a clean git state."),
                "Re-run the evidence command from a clean checkout.",
            );
        }
    }
}

fn validate_issue_train_context(
    snapshot: &PrReadinessSnapshot,
    linked_issue_number: Option<u64>,
    findings: &mut Vec<PrReadinessFinding>,
) {
    let Some(linked_issue) = &snapshot.linked_issue else {
        return;
    };
    if linked_issue_number.is_some_and(|number| number != linked_issue.number) {
        push_error(
            findings,
            "loaded_issue_mismatch",
            Some(format!("issue:{}", linked_issue.number)),
            "Loaded issue does not match the PR closing issue.",
            "Fetch the exact issue referenced by the PR closing keyword.",
        );
    }

    let Some(parent) = &snapshot.parent_issue else {
        push_error(
            findings,
            "missing_parent_plan",
            Some(format!("issue:{}", linked_issue.number)),
            "Parent plan issue was not available for issue-train validation.",
            "Ensure the repository has exactly one open parent plan issue or pass the correct parent in the caller.",
        );
        return;
    };

    let mut children = snapshot.child_issues.clone();
    children.retain(|child| child.number != linked_issue.number);
    children.push(linked_issue.clone());
    let issue_train_report = validate_issue_train(&IssueTrainSnapshot {
        parent: parent.clone(),
        children,
    });
    for issue_train_finding in issue_train_report.findings {
        let message = format!("Issue-train readiness: {}", issue_train_finding.message);
        let subject = issue_train_finding
            .issue_number
            .map(|number| format!("issue:{number}"));
        findings.push(PrReadinessFinding {
            severity: issue_train_finding.severity,
            code: format!("issue_train_{}", issue_train_finding.code),
            subject,
            message,
            remediation: issue_train_finding.remediation,
        });
    }
}

fn evidence_by_id(method_state: &MethodState) -> BTreeMap<&str, &MethodEvidence> {
    method_state
        .evidence
        .iter()
        .map(|evidence| (evidence.id.as_str(), evidence))
        .collect()
}

fn normalized_allowed_paths(paths: &[String]) -> Vec<String> {
    paths
        .iter()
        .filter_map(|path| {
            let normalized = normalize_path(path);
            (!normalized.is_empty()).then_some(normalized)
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn path_matches_allowed_prefix(file: &str, allowed: &str) -> bool {
    let file = normalize_path(file);
    if allowed == "." {
        return true;
    }
    file == allowed || file.starts_with(&format!("{allowed}/"))
}

fn normalize_path(path: &str) -> String {
    path.trim()
        .trim_matches('`')
        .trim_start_matches("./")
        .trim_matches('/')
        .to_string()
}

fn section_content_raw(body: &str, heading: &str) -> Option<String> {
    let mut lines = Vec::new();
    let mut in_section = false;
    let mut start_level = 0;

    for line in body.lines() {
        if let Some((level, title)) = parse_heading(line) {
            if in_section && level <= start_level {
                break;
            }
            if normalize_heading(title) == normalize_heading(heading) {
                in_section = true;
                start_level = level;
                continue;
            }
        }

        if in_section {
            lines.push(line);
        }
    }

    in_section.then(|| lines.join("\n").trim().to_string())
}

fn parse_heading(line: &str) -> Option<(usize, &str)> {
    let trimmed = line.trim_start();
    let level = trimmed.chars().take_while(|ch| *ch == '#').count();
    if level == 0 {
        return None;
    }
    let rest = trimmed.get(level..)?;
    if !rest.starts_with(' ') {
        return None;
    }
    Some((level, rest.trim().trim_matches('#').trim()))
}

fn normalize_heading(heading: &str) -> String {
    heading
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn push_error(
    findings: &mut Vec<PrReadinessFinding>,
    code: &str,
    subject: Option<String>,
    message: &str,
    remediation: &str,
) {
    findings.push(PrReadinessFinding {
        severity: FindingSeverity::Error,
        code: code.to_string(),
        subject,
        message: message.to_string(),
        remediation: remediation.to_string(),
    });
}

fn push_warning(
    findings: &mut Vec<PrReadinessFinding>,
    code: &str,
    subject: Option<String>,
    message: &str,
    remediation: &str,
) {
    findings.push(PrReadinessFinding {
        severity: FindingSeverity::Warning,
        code: code.to_string(),
        subject,
        message: message.to_string(),
        remediation: remediation.to_string(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_protocol::method_state::METHOD_EVIDENCE_RECEIPT_SCHEMA_VERSION;
    use codex_protocol::method_state::METHOD_STATE_SCHEMA_VERSION;
    use codex_protocol::method_state::MethodClosureState;
    use codex_protocol::method_state::MethodEvidenceExitStatus;
    use codex_protocol::method_state::MethodEvidenceGitState;
    use codex_protocol::method_state::MethodEvidenceKind;
    use codex_protocol::method_state::MethodEvidenceReceipt;
    use codex_protocol::method_state::MethodEvidenceRedactionStatus;
    use codex_protocol::method_state::MethodEvidenceRequirement;
    use codex_protocol::method_state::MethodEvidenceSessionMetadata;
    use codex_protocol::method_state::MethodFalsifier;
    use codex_protocol::method_state::MethodIntent;
    use codex_protocol::method_state::MethodLinkedIssue;
    use codex_protocol::method_state::MethodProvenance;
    use codex_protocol::method_state::MethodProvenanceSource;
    use codex_protocol::method_state::MethodResumeContext;
    use codex_protocol::method_state::MethodReviewFinding;
    use codex_protocol::method_state::MethodReviewFindingStatus;
    use codex_protocol::method_state::MethodReviewSeverity;

    const REPO: &str = "owner/repo";
    const HEAD_SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn valid_snapshot() -> PrReadinessSnapshot {
        PrReadinessSnapshot {
            repository: REPO.to_string(),
            pull_request: PullRequestSnapshot {
                number: 7,
                title: "Task PR".to_string(),
                body: "## Summary\n\nReady.\n\n## Issue\n\nFixes #20\n\n## Allowed Paths\n\n- codex-rs/core\n".to_string(),
                head_sha: HEAD_SHA.to_string(),
                base_ref: "master".to_string(),
                head_ref: "task-20".to_string(),
                changed_files: vec!["codex-rs/core/src/pr_readiness.rs".to_string()],
            },
            linked_issue: Some(issue(20, "Task: Implement PR readiness validator", vec![
                "aegis-code:task",
            ])),
            parent_issue: Some(parent_issue(false)),
            child_issues: vec![issue(20, "Task: Implement PR readiness validator", vec![
                "aegis-code:task",
            ])],
            method_state: Some(method_state(HEAD_SHA)),
            allowed_paths: vec!["codex-rs/core".to_string()],
        }
    }

    fn issue(number: u64, title: &str, labels: Vec<&str>) -> IssueSnapshot {
        IssueSnapshot {
            number,
            title: title.to_string(),
            state: crate::issue_train::IssueState::Open,
            body: "## Objective\n\nShip the task.\n\n## Scope\n\nImplement the validator.\n\n## Acceptance Criteria\n\n- Validator passes.\n\n## Falsifiers\n\n- Invalid PR passes.\n\n## Dependencies\n\nNone\n".to_string(),
            labels: labels.into_iter().map(str::to_string).collect(),
        }
    }

    fn parent_issue(checked: bool) -> IssueSnapshot {
        let marker = if checked { "x" } else { " " };
        IssueSnapshot {
            number: 1,
            title: "Plan: Aegis Code".to_string(),
            state: crate::issue_train::IssueState::Open,
            body: format!(
                "## Objective\n\nCoordinate work.\n\n## Child Issues\n\n- [{marker}] #20 Implement PR readiness validator\n\n## Evidence Required For Closure\n\nReconcile children.\n"
            ),
            labels: vec!["aegis-code:plan".to_string()],
        }
    }

    fn method_state(commit: &str) -> MethodState {
        MethodState {
            schema_version: METHOD_STATE_SCHEMA_VERSION,
            intent: MethodIntent {
                summary: "Ship PR readiness validator".to_string(),
                success_criteria: vec!["PR readiness is validated".to_string()],
            },
            linked_issue: Some(MethodLinkedIssue {
                provider: MethodIssueProvider::GitHub,
                repository: REPO.to_string(),
                number: 20,
                title: Some("Task: Implement PR readiness validator".to_string()),
                url: None,
            }),
            status: MethodWorkStatus::Closed,
            claims: Vec::new(),
            assumptions: Vec::new(),
            falsifiers: vec![MethodFalsifier {
                id: "falsifier:missing-linkage".to_string(),
                summary: "PR can pass with no linked issue".to_string(),
                status: MethodFalsifierStatus::Disproved,
                evidence_ids: vec!["evidence:test".to_string()],
            }],
            evidence_requirements: vec![MethodEvidenceRequirement {
                id: "requirement:test".to_string(),
                summary: "Tests pass".to_string(),
                required: true,
                commands: vec!["cargo test -p codex-core pr_readiness".to_string()],
                claim_ids: Vec::new(),
                falsifier_ids: vec!["falsifier:missing-linkage".to_string()],
            }],
            evidence: vec![MethodEvidence {
                id: "evidence:test".to_string(),
                summary: "Tests passed".to_string(),
                kind: MethodEvidenceKind::Test,
                requirement_ids: vec!["requirement:test".to_string()],
                claim_ids: Vec::new(),
                falsifier_ids: vec!["falsifier:missing-linkage".to_string()],
                source: Some("test".to_string()),
                captured_at_unix_seconds: 1,
                receipt: Some(receipt(commit, false, 0)),
            }],
            gates: Vec::new(),
            review_findings: Vec::new(),
            closure: Some(MethodClosureState {
                closed_at_unix_seconds: 2,
                summary: "Ready".to_string(),
                evidence_ids: vec!["evidence:test".to_string()],
                review_finding_ids: Vec::new(),
                closed_by: Some("tester".to_string()),
            }),
            resume_context: MethodResumeContext {
                repository: Some(REPO.to_string()),
                branch: Some("task-20".to_string()),
                commit: Some(commit.to_string()),
                linked_issue: None,
                schema_version: Some(METHOD_STATE_SCHEMA_VERSION),
            },
            provenance: MethodProvenance {
                source: MethodProvenanceSource::Agent,
                created_at_unix_seconds: 1,
                updated_at_unix_seconds: 2,
                actor: Some("tester".to_string()),
            },
        }
    }

    fn receipt(commit: &str, dirty: bool, exit_code: i32) -> MethodEvidenceReceipt {
        MethodEvidenceReceipt {
            schema_version: METHOD_EVIDENCE_RECEIPT_SCHEMA_VERSION,
            command: vec!["cargo".to_string(), "test".to_string()],
            cwd: "/repo".to_string(),
            captured_at_unix_seconds: 1,
            git_state: MethodEvidenceGitState {
                status: MethodEvidenceGitStateStatus::Captured,
                repository: Some(REPO.to_string()),
                branch: Some("task-20".to_string()),
                commit: Some(commit.to_string()),
                dirty: Some(dirty),
                unavailable_reason: None,
            },
            exit_status: MethodEvidenceExitStatus {
                exit_code: Some(exit_code),
                timed_out: false,
            },
            output_summary: "ok".to_string(),
            artifacts: Vec::new(),
            session: MethodEvidenceSessionMetadata {
                session_id: Some("session".to_string()),
                thread_id: Some("thread".to_string()),
                provider: Some("test".to_string()),
                model: Some("test".to_string()),
            },
            redaction_status: MethodEvidenceRedactionStatus::NotNeeded,
        }
    }

    #[test]
    fn valid_ready_pr_passes() {
        let report = validate_pr_readiness(&valid_snapshot());

        assert!(report.valid, "{report:#?}");
        assert_eq!(report.linked_issue_number, Some(20));
        assert_eq!(report.changed_file_count, 1);
    }

    #[test]
    fn missing_and_multiple_closing_refs_fail() {
        let mut missing = valid_snapshot();
        missing.pull_request.body = "No closing reference".to_string();
        let missing_report = validate_pr_readiness(&missing);
        assert_has_error(&missing_report, "missing_closing_issue_ref");

        let mut multiple = valid_snapshot();
        multiple.pull_request.body = "Fixes #20\nCloses #21".to_string();
        let multiple_report = validate_pr_readiness(&multiple);
        assert_has_error(&multiple_report, "multiple_closing_issue_refs");

        let mut duplicate = valid_snapshot();
        duplicate.pull_request.body = "Fixes #20\nCloses #20".to_string();
        let duplicate_report = validate_pr_readiness(&duplicate);
        assert_has_error(&duplicate_report, "multiple_closing_issue_refs");
    }

    #[test]
    fn scope_drift_fails_when_changed_file_is_outside_allowed_paths() {
        let mut snapshot = valid_snapshot();
        snapshot.pull_request.changed_files = vec!["docs/README.md".to_string()];

        let report = validate_pr_readiness(&snapshot);

        assert_has_error(&report, "changed_file_outside_allowed_paths");
    }

    #[test]
    fn method_state_issue_mismatch_fails() {
        let mut snapshot = valid_snapshot();
        let mut state = snapshot.method_state.take().expect("method state");
        state.linked_issue.as_mut().expect("linked issue").number = 21;
        snapshot.method_state = Some(state);

        let report = validate_pr_readiness(&snapshot);

        assert_has_error(&report, "method_state_issue_mismatch");
    }

    #[test]
    fn missing_closure_evidence_and_failed_receipt_fail() {
        let mut snapshot = valid_snapshot();
        let mut state = snapshot.method_state.take().expect("method state");
        state
            .closure
            .as_mut()
            .expect("closure")
            .evidence_ids
            .clear();
        state.evidence[0]
            .receipt
            .as_mut()
            .expect("receipt")
            .exit_status
            .exit_code = Some(1);
        snapshot.method_state = Some(state);

        let report = validate_pr_readiness(&snapshot);

        assert_has_error(&report, "invalid_closure_evidence");
        assert_has_error(&report, "missing_required_evidence");
    }

    #[test]
    fn receipt_commit_or_dirty_state_fail() {
        let mut snapshot = valid_snapshot();
        let mut state = snapshot.method_state.take().expect("method state");
        state.evidence[0].receipt =
            Some(receipt("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", true, 0));
        snapshot.method_state = Some(state);

        let report = validate_pr_readiness(&snapshot);

        assert_has_error(&report, "receipt_commit_mismatch");
        assert_has_error(&report, "receipt_dirty_git_state");
    }

    #[test]
    fn review_findings_are_severity_aware() {
        let mut snapshot = valid_snapshot();
        let mut state = snapshot.method_state.take().expect("method state");
        state.review_findings = vec![
            MethodReviewFinding {
                id: "finding:blocking".to_string(),
                summary: "Must fix".to_string(),
                severity: MethodReviewSeverity::Blocking,
                status: MethodReviewFindingStatus::Open,
                claim_ids: Vec::new(),
                evidence_ids: Vec::new(),
                reviewed_at_unix_seconds: 1,
                reviewer: Some("reviewer".to_string()),
            },
            MethodReviewFinding {
                id: "finding:info".to_string(),
                summary: "Note".to_string(),
                severity: MethodReviewSeverity::Info,
                status: MethodReviewFindingStatus::Open,
                claim_ids: Vec::new(),
                evidence_ids: Vec::new(),
                reviewed_at_unix_seconds: 1,
                reviewer: Some("reviewer".to_string()),
            },
        ];
        snapshot.method_state = Some(state);

        let report = validate_pr_readiness(&snapshot);

        assert_has_error(&report, "open_blocking_review_finding");
        assert_has_warning(&report, "open_nonblocking_review_finding");
    }

    #[test]
    fn invalid_issue_train_context_fails() {
        let mut snapshot = valid_snapshot();
        snapshot.linked_issue = Some(issue(20, "Implement PR readiness validator", Vec::new()));

        let report = validate_pr_readiness(&snapshot);

        assert_has_error(&report, "issue_train_child_missing_task_label");
        assert_has_error(&report, "issue_train_child_title_missing_task_prefix");
    }

    #[test]
    fn parses_allowed_paths_from_pr_body() {
        let body = "## Summary\n\nText\n\n## Allowed Paths\n\n- `codex-rs/core`\n- docs\n\n## Issue\n\nFixes #20\n";

        let paths = parse_allowed_paths_from_pr_body(body);

        assert_eq!(paths, vec!["codex-rs/core".to_string(), "docs".to_string()]);
    }

    fn assert_has_error(report: &PrReadinessReport, code: &str) {
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.severity == FindingSeverity::Error && finding.code == code),
            "missing error {code}: {report:#?}"
        );
    }

    fn assert_has_warning(report: &PrReadinessReport, code: &str) {
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.severity == FindingSeverity::Warning
                    && finding.code == code),
            "missing warning {code}: {report:#?}"
        );
    }
}
