use anyhow::Context;
use anyhow::Result;
use codex_core::config::AegisEngineFailureMode;
use codex_core::issue_train::IssueSnapshot;
use codex_core::issue_train::IssueState;
use codex_core::pr_readiness::PrReadinessSnapshot;
use codex_core::pr_readiness::PullRequestSnapshot;
use codex_core::pr_readiness::validate_pr_readiness;
use codex_protocol::method_state::METHOD_STATE_SCHEMA_VERSION;
use codex_protocol::method_state::MethodClosureState;
use codex_protocol::method_state::MethodEvidenceGitStateStatus;
use codex_protocol::method_state::MethodEvidenceRequirement;
use codex_protocol::method_state::MethodFalsifier;
use codex_protocol::method_state::MethodFalsifierStatus;
use codex_protocol::method_state::MethodIntent;
use codex_protocol::method_state::MethodIssueProvider;
use codex_protocol::method_state::MethodLinkedIssue;
use codex_protocol::method_state::MethodProvenance;
use codex_protocol::method_state::MethodProvenanceSource;
use codex_protocol::method_state::MethodResumeContext;
use codex_protocol::method_state::MethodReviewFindingStatus;
use codex_protocol::method_state::MethodState;
use codex_protocol::method_state::MethodWorkStatus;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::Op;
use codex_protocol::protocol::ReviewRequest;
use codex_protocol::protocol::ReviewTarget;
use core_test_support::responses::ev_apply_patch_call;
use core_test_support::responses::ev_assistant_message;
use core_test_support::responses::ev_completed;
use core_test_support::responses::ev_local_shell_call;
use core_test_support::responses::ev_response_created;
use core_test_support::responses::mount_sse_sequence;
use core_test_support::responses::sse;
use core_test_support::skip_if_no_network;
use core_test_support::test_codex::ApplyPatchModelOutput;
use core_test_support::test_codex::TestCodexHarness;
use core_test_support::test_codex::test_codex;
use core_test_support::wait_for_event;
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

const REPO: &str = "mithran-hq/aegis-code";
const TASK_ISSUE: u64 = 39;
const PARENT_ISSUE: u64 = 1;
const CONTEXT_PACK_GUIDANCE: &str =
    "Golden path pack guidance: receipts, review findings, and closure gates are mandatory.";
const EVIDENCE_CALL_ID: &str = "golden-evidence";
const EVIDENCE_SCRIPT: &str =
    "git add src/lib.rs && git commit -m golden-path-change && git rev-parse HEAD";
const COMMAND_EVIDENCE_ID: &str = "evidence:project-command:golden-evidence";
const REQUIRED_EVIDENCE_ID: &str = "requirement:golden-path-evidence";
const FALSIFIER_ID: &str = "falsifier:golden-path-can-pass-without-receipts";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn golden_path_closes_method_loop_and_rejects_broken_gate() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let harness = golden_path_harness().await?;
    init_fixture_repo(harness.cwd())?;
    materialize_thread(&harness).await?;
    harness
        .test()
        .codex
        .replace_method_state(open_method_state())
        .await?;

    drive_code_change(&harness).await?;
    assert_eq!(
        harness.read_file_text("src/lib.rs").await?,
        "pub fn answer() -> i32 { 42 }\n"
    );
    assert_model_saw_context_pack(&harness).await?;

    drive_evidence_command(&harness).await?;
    let state_after_evidence = persisted_method_state(&harness).await?;
    let command_evidence = state_after_evidence
        .evidence
        .iter()
        .find(|evidence| evidence.id == COMMAND_EVIDENCE_ID)
        .context("missing command evidence receipt")?;
    let command_receipt = command_evidence
        .receipt
        .as_ref()
        .context("command evidence missing receipt")?;
    assert_eq!(command_receipt.exit_status.exit_code, Some(0));
    assert_eq!(
        command_receipt.git_state.status,
        MethodEvidenceGitStateStatus::Captured
    );
    assert_eq!(command_receipt.git_state.dirty, Some(false));
    assert_eq!(
        command_evidence.requirement_ids,
        vec![REQUIRED_EVIDENCE_ID.to_string()]
    );

    drive_review(&harness).await?;
    let mut closed_state = persisted_method_state(&harness).await?;
    let review_finding = closed_state
        .review_findings
        .iter()
        .find(|finding| finding.status == MethodReviewFindingStatus::Addressed)
        .context("review did not record an addressed finding")?
        .clone();
    let review_evidence_id = review_finding
        .evidence_ids
        .first()
        .context("review finding missing backing evidence")?
        .clone();
    let pr_head = git_output(harness.cwd(), &["rev-parse", "HEAD"])?;
    close_method_state(
        &mut closed_state,
        vec![COMMAND_EVIDENCE_ID.to_string(), review_evidence_id],
        review_finding.id.clone(),
    );
    harness
        .test()
        .codex
        .replace_method_state(closed_state.clone())
        .await?;

    let snapshot = pr_snapshot(pr_head.trim(), closed_state);
    let report = validate_pr_readiness(&snapshot);
    assert!(report.valid, "golden path should be PR-ready: {report:#?}");

    let mut broken_snapshot = snapshot.clone();
    broken_snapshot.pull_request.head_sha = "0000000000000000000000000000000000000000".to_string();
    let broken_report = validate_pr_readiness(&broken_snapshot);
    assert!(
        !broken_report.valid,
        "broken fixture must fail PR readiness validation"
    );
    assert!(
        broken_report
            .findings
            .iter()
            .any(|finding| finding.code == "receipt_commit_mismatch"),
        "broken fixture should name the broken gate: {broken_report:#?}"
    );

    assert_aegis_engine_events(&harness, COMMAND_EVIDENCE_ID, &review_finding.id).await?;

    Ok(())
}

async fn golden_path_harness() -> Result<TestCodexHarness> {
    let builder = test_codex()
        .with_pre_build_hook(|home| {
            let pack_path = home.join("golden-path-pack.toml");
            fs::write(&pack_path, context_pack_toml()).expect("write context pack");
            fs::write(
                home.join("config.toml"),
                format!(
                    "context_pack_paths = [{}]\n",
                    toml::Value::String(pack_path.to_string_lossy().to_string())
                ),
            )
            .expect("write config");
        })
        .with_config(|config| {
            config.include_apply_patch_tool = true;
            config.aegis_engine.failure_mode = AegisEngineFailureMode::Require;
        });

    TestCodexHarness::with_builder(builder).await
}

async fn materialize_thread(harness: &TestCodexHarness) -> Result<()> {
    mount_sse_sequence(
        harness.server(),
        vec![sse(vec![
            ev_response_created("resp-warmup-1"),
            ev_assistant_message("msg-warmup-1", "ready"),
            ev_completed("resp-warmup-1"),
        ])],
    )
    .await;

    harness.submit("start the golden-path fixture").await?;
    Ok(())
}

fn context_pack_toml() -> &'static str {
    r#"
schema_version = 1
pack_id = "user:golden-path"
kind = "user"
name = "Golden Path"

[compatibility]
schema = "1"

[[guidance]]
id = "guidance:golden-path"
category = "method"
text = "Golden path pack guidance: receipts, review findings, and closure gates are mandatory."

[promotion]
status = "promoted"
"#
}

fn init_fixture_repo(cwd: &Path) -> Result<()> {
    fs::create_dir_all(cwd.join("src"))?;
    fs::write(cwd.join("src/lib.rs"), "pub fn answer() -> i32 { 40 }\n")?;
    git(cwd, &["init"])?;
    git(cwd, &["checkout", "-b", "task-39"])?;
    git(cwd, &["config", "user.name", "Aegis Test"])?;
    git(cwd, &["config", "user.email", "aegis-test@example.com"])?;
    git(cwd, &["add", "src/lib.rs"])?;
    git(cwd, &["commit", "-m", "initial"])?;
    Ok(())
}

async fn drive_code_change(harness: &TestCodexHarness) -> Result<()> {
    let patch = "*** Begin Patch\n*** Update File: src/lib.rs\n@@\n-pub fn answer() -> i32 { 40 }\n+pub fn answer() -> i32 { 42 }\n*** End Patch";
    mount_sse_sequence(
        harness.server(),
        vec![
            sse(vec![
                ev_response_created("resp-code-1"),
                ev_apply_patch_call("golden-apply-patch", patch, ApplyPatchModelOutput::Function),
                ev_completed("resp-code-1"),
            ]),
            sse(vec![
                ev_assistant_message("msg-code-1", "code change applied"),
                ev_completed("resp-code-2"),
            ]),
        ],
    )
    .await;

    harness.submit("apply the golden-path code change").await?;
    Ok(())
}

async fn assert_model_saw_context_pack(harness: &TestCodexHarness) -> Result<()> {
    let bodies = harness.request_bodies().await;
    let serialized = serde_json::to_string(&bodies)?;
    assert!(
        serialized.contains(CONTEXT_PACK_GUIDANCE),
        "model-visible request omitted promoted context-pack guidance"
    );

    let summary = harness.test().codex.method_status_summary().await;
    assert!(
        summary.context_packs.active > 0,
        "method status did not expose active context packs"
    );

    Ok(())
}

async fn drive_evidence_command(harness: &TestCodexHarness) -> Result<()> {
    mount_sse_sequence(
        harness.server(),
        vec![
            sse(vec![
                ev_response_created("resp-evidence-1"),
                ev_local_shell_call(
                    EVIDENCE_CALL_ID,
                    "completed",
                    vec!["/bin/sh", "-c", EVIDENCE_SCRIPT],
                ),
                ev_completed("resp-evidence-1"),
            ]),
            sse(vec![
                ev_assistant_message("msg-evidence-1", "evidence captured"),
                ev_completed("resp-evidence-2"),
            ]),
        ],
    )
    .await;

    harness
        .submit("run the required evidence command for the golden path")
        .await?;
    Ok(())
}

async fn drive_review(harness: &TestCodexHarness) -> Result<()> {
    let review_json = serde_json::json!({
        "findings": [],
        "overall_correctness": "good",
        "overall_explanation": "No actionable findings; receipts and closure gates are present.",
        "overall_confidence_score": 0.99
    })
    .to_string();
    mount_sse_sequence(
        harness.server(),
        vec![sse(vec![
            ev_response_created("resp-review-1"),
            serde_json::json!({
                "type": "response.output_item.done",
                "item": {
                    "type": "message",
                    "role": "assistant",
                    "id": "msg-review-1",
                    "content": [{"type": "output_text", "text": review_json}]
                }
            }),
            ev_completed("resp-review-1"),
        ])],
    )
    .await;

    harness
        .test()
        .codex
        .submit(Op::Review {
            review_request: ReviewRequest {
                target: ReviewTarget::Custom {
                    instructions: "Review the golden-path task change.".to_string(),
                },
                user_facing_hint: None,
            },
        })
        .await?;

    wait_for_event(&harness.test().codex, |event| {
        matches!(event, EventMsg::TurnComplete(_))
    })
    .await;
    Ok(())
}

async fn persisted_method_state(harness: &TestCodexHarness) -> Result<MethodState> {
    let db = harness
        .test()
        .codex
        .state_db()
        .context("state db enabled")?;
    let record = db
        .get_thread_method_state(harness.test().session_configured.thread_id)
        .await?
        .context("method state persisted")?;
    Ok(record.state)
}

fn open_method_state() -> MethodState {
    MethodState {
        schema_version: METHOD_STATE_SCHEMA_VERSION,
        intent: MethodIntent {
            summary: "Add golden-path integration tests".to_string(),
            success_criteria: vec![
                "Golden path runs in CI".to_string(),
                "Passing and failing fixtures assert receipts, review, events, and closure"
                    .to_string(),
            ],
        },
        linked_issue: Some(MethodLinkedIssue {
            provider: MethodIssueProvider::GitHub,
            repository: REPO.to_string(),
            number: TASK_ISSUE,
            title: Some("Task: Add golden-path integration tests".to_string()),
            url: Some(format!("https://github.com/{REPO}/issues/{TASK_ISSUE}")),
        }),
        status: MethodWorkStatus::Incomplete,
        claims: Vec::new(),
        assumptions: Vec::new(),
        falsifiers: vec![MethodFalsifier {
            id: FALSIFIER_ID.to_string(),
            summary: "Golden path can pass without evidence receipts".to_string(),
            status: MethodFalsifierStatus::Open,
            evidence_ids: Vec::new(),
        }],
        evidence_requirements: vec![MethodEvidenceRequirement {
            id: REQUIRED_EVIDENCE_ID.to_string(),
            summary: "Run the deterministic evidence command after the code change".to_string(),
            required: true,
            commands: vec![EVIDENCE_SCRIPT.to_string()],
            claim_ids: Vec::new(),
            falsifier_ids: vec![FALSIFIER_ID.to_string()],
        }],
        evidence: Vec::new(),
        gates: Vec::new(),
        engine_alerts: Vec::new(),
        review_findings: Vec::new(),
        closure: None,
        resume_context: MethodResumeContext {
            repository: Some(REPO.to_string()),
            branch: Some("task-39".to_string()),
            commit: None,
            linked_issue: None,
            schema_version: Some(METHOD_STATE_SCHEMA_VERSION),
            sandbox_posture: None,
        },
        provenance: MethodProvenance {
            created_at_unix_seconds: 1,
            updated_at_unix_seconds: 1,
            source: MethodProvenanceSource::Agent,
            actor: Some("golden-path-test".to_string()),
        },
    }
}

fn close_method_state(
    state: &mut MethodState,
    closure_evidence_ids: Vec<String>,
    review_finding_id: String,
) {
    state.status = MethodWorkStatus::Closed;
    for falsifier in &mut state.falsifiers {
        if falsifier.id == FALSIFIER_ID {
            falsifier.status = MethodFalsifierStatus::Disproved;
            falsifier.evidence_ids = vec![COMMAND_EVIDENCE_ID.to_string()];
        }
    }
    state.closure = Some(MethodClosureState {
        closed_at_unix_seconds: 2,
        summary: "Golden-path fixture is ready for PR validation.".to_string(),
        evidence_ids: closure_evidence_ids,
        review_finding_ids: vec![review_finding_id],
        closed_by: Some("golden-path-test".to_string()),
    });
    state.provenance.updated_at_unix_seconds = 2;
}

fn pr_snapshot(head_sha: &str, method_state: MethodState) -> PrReadinessSnapshot {
    PrReadinessSnapshot {
        repository: REPO.to_string(),
        pull_request: PullRequestSnapshot {
            number: 3900,
            title: "Task: Add golden-path integration tests".to_string(),
            body: "## Summary\n\nAdds golden-path integration coverage.\n\nFixes #39\n\n## Allowed Paths\n\n- src\n".to_string(),
            head_sha: head_sha.to_string(),
            base_ref: "main".to_string(),
            head_ref: "task-39".to_string(),
            changed_files: vec!["src/lib.rs".to_string()],
        },
        linked_issue: Some(task_issue()),
        parent_issue: Some(parent_issue()),
        child_issues: vec![task_issue()],
        method_state: Some(method_state),
        allowed_paths: vec!["src".to_string()],
    }
}

fn task_issue() -> IssueSnapshot {
    IssueSnapshot {
        number: TASK_ISSUE,
        title: "Task: Add golden-path integration tests".to_string(),
        state: IssueState::Open,
        body: "## Objective\n\nProve the whole method loop works end to end.\n\n## Scope\n\nCreate fixture workflows for linked issue, code change, evidence receipt, adversarial review, PR readiness, event emission, context-pack visibility, and closure.\n\n## Acceptance Criteria\n\n- Golden path runs in CI.\n- Fixture includes at least one passing and one failing task.\n- Evidence receipts and events are asserted.\n- Test failures identify the broken gate.\n\n## Falsifiers\n\n- Only unit tests exist for isolated pieces.\n- Golden path can pass without receipts.\n- Failing workflow does not fail the test.\n\n## Dependencies\n\nDepends on method gates, receipts, review, and event sink.\n".to_string(),
        labels: vec!["aegis-code:task".to_string()],
    }
}

fn parent_issue() -> IssueSnapshot {
    IssueSnapshot {
        number: PARENT_ISSUE,
        title: "Plan: Aegis Code".to_string(),
        state: IssueState::Open,
        body: "## Objective\n\nCoordinate Aegis Code delivery.\n\n## Child Issues\n\n- [ ] #39 Task: Add golden-path integration tests\n\n## Evidence For Closure\n\nClose child tasks only after landed evidence is reconciled.\n".to_string(),
        labels: vec!["aegis-code:plan".to_string()],
    }
}

async fn assert_aegis_engine_events(
    harness: &TestCodexHarness,
    evidence_id: &str,
    finding_id: &str,
) -> Result<()> {
    let path = harness.test().config.aegis_engine.jsonl_path.clone();
    let events = read_events_until(&path, |events| {
        has_category(events, "resume")
            && has_context(events, "evidence_id", evidence_id)
            && has_context(events, "finding_id", finding_id)
    })
    .await?;

    assert!(
        has_category(&events, "resume"),
        "missing resume/status event"
    );
    assert!(
        has_context(&events, "evidence_id", evidence_id),
        "missing evidence receipt event"
    );
    assert!(
        has_context(&events, "finding_id", finding_id),
        "missing review-finding event"
    );
    Ok(())
}

async fn read_events_until(
    path: &Path,
    predicate: impl Fn(&[Value]) -> bool,
) -> Result<Vec<Value>> {
    for _ in 0..50 {
        let events = read_events(path)?;
        if predicate(&events) {
            return Ok(events);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let events = read_events(path)?;
    anyhow::bail!("timed out waiting for Aegis Engine events: {events:#?}");
}

fn read_events(path: &Path) -> Result<Vec<Value>> {
    let Ok(raw) = fs::read_to_string(path) else {
        return Ok(Vec::new());
    };
    raw.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).context("parse safety event jsonl"))
        .collect()
}

fn has_category(events: &[Value], category: &str) -> bool {
    events
        .iter()
        .any(|event| event.get("category").and_then(Value::as_str) == Some(category))
}

fn has_context(events: &[Value], key: &str, expected: &str) -> bool {
    events.iter().any(|event| {
        event
            .get("context")
            .and_then(|context| context.get(key))
            .and_then(Value::as_str)
            == Some(expected)
    })
}

fn git(cwd: &Path, args: &[&str]) -> Result<()> {
    let output = Command::new("git").args(args).current_dir(cwd).output()?;
    anyhow::ensure!(
        output.status.success(),
        "git {} failed\nstdout:\n{}\nstderr:\n{}",
        args.join(" "),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

fn git_output(cwd: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git").args(args).current_dir(cwd).output()?;
    anyhow::ensure!(
        output.status.success(),
        "git {} failed\nstdout:\n{}\nstderr:\n{}",
        args.join(" "),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?)
}
