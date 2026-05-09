use anyhow::Context;
use anyhow::Result;
use codex_core::config::AegisEngineFailureMode;
use codex_protocol::method_state::METHOD_STATE_SCHEMA_VERSION;
use codex_protocol::method_state::MethodEvidenceRedactionStatus;
use codex_protocol::method_state::MethodEvidenceRequirement;
use codex_protocol::method_state::MethodIntent;
use codex_protocol::method_state::MethodIssueProvider;
use codex_protocol::method_state::MethodLinkedIssue;
use codex_protocol::method_state::MethodProvenance;
use codex_protocol::method_state::MethodProvenanceSource;
use codex_protocol::method_state::MethodResumeContext;
use codex_protocol::method_state::MethodState;
use codex_protocol::method_state::MethodWorkStatus;
use core_test_support::responses::ev_assistant_message;
use core_test_support::responses::ev_completed;
use core_test_support::responses::ev_local_shell_call;
use core_test_support::responses::ev_response_created;
use core_test_support::responses::mount_sse_sequence;
use core_test_support::responses::sse;
use core_test_support::test_codex::TestCodexHarness;
use core_test_support::test_codex::test_codex;
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::time::Duration;

const REPO: &str = "mithran-hq/aegis-code";
const TASK_ISSUE: u64 = 40;
const EVIDENCE_CALL_ID: &str = "security-redaction-evidence";
const EVIDENCE_SCRIPT: &str =
    "cat redaction-output.txt && printf 'build passed trace_id=trace-40 harmless-context\\n'";
const COMMAND_EVIDENCE_ID: &str = "evidence:project-command:security-redaction-evidence";
const REQUIRED_EVIDENCE_ID: &str = "requirement:security-redaction-evidence";
const SECRET_OUTPUT: &str = "\
Authorization: Bearer sk-live-redaction-secret
OPENAI_API_KEY=sk-env-redaction-secret
password=correcthorsebattery-redaction
api_key=key-redaction-secret
token=secret-token-redaction
";
const RAW_SECRETS: &[&str] = &[
    "sk-live-redaction-secret",
    "sk-env-redaction-secret",
    "correcthorsebattery-redaction",
    "key-redaction-secret",
    "secret-token-redaction",
];

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn security_redaction_scrubs_receipts_and_events_but_keeps_context() -> Result<()> {
    let harness = security_redaction_harness().await?;
    fs::write(harness.cwd().join("redaction-output.txt"), SECRET_OUTPUT)?;
    materialize_thread(&harness).await?;
    harness
        .test()
        .codex
        .replace_method_state(open_method_state())
        .await?;

    drive_evidence_command(&harness).await?;

    let state = persisted_method_state(&harness).await?;
    let state_json = serde_json::to_string(&state)?;
    assert_no_raw_secrets("persisted method state", &state_json);
    assert!(
        state_json.contains("<redacted>"),
        "persisted method state should preserve redaction markers"
    );
    assert!(
        state_json.contains("build passed trace_id=trace-40 harmless-context"),
        "persisted method state should retain harmless evidence context: {state_json}"
    );

    let evidence = state
        .evidence
        .iter()
        .find(|evidence| evidence.id == COMMAND_EVIDENCE_ID)
        .context("missing security redaction evidence")?;
    assert_eq!(evidence.requirement_ids, vec![REQUIRED_EVIDENCE_ID]);
    let receipt = evidence.receipt.as_ref().context("missing receipt")?;
    assert_eq!(
        receipt.redaction_status,
        MethodEvidenceRedactionStatus::Redacted
    );
    assert!(receipt.output_summary.contains("<redacted>"));
    assert!(receipt.output_summary.contains("harmless-context"));
    assert_eq!(receipt.exit_status.exit_code, Some(0));

    let events = read_events_until(&harness.test().config.aegis_engine.jsonl_path, |events| {
        has_context(events, "evidence_id", COMMAND_EVIDENCE_ID)
    })
    .await?;
    let events_json = serde_json::to_string(&events)?;
    assert_no_raw_secrets("Aegis Engine events", &events_json);
    assert!(events_json.contains("<redacted>"));
    assert!(events_json.contains("harmless-context"));

    Ok(())
}

async fn security_redaction_harness() -> Result<TestCodexHarness> {
    let builder = test_codex().with_config(|config| {
        config.aegis_engine.failure_mode = AegisEngineFailureMode::Require;
    });

    TestCodexHarness::with_builder(builder).await
}

async fn materialize_thread(harness: &TestCodexHarness) -> Result<()> {
    mount_sse_sequence(
        harness.server(),
        vec![sse(vec![
            ev_response_created("resp-redaction-warmup-1"),
            ev_assistant_message("msg-redaction-warmup-1", "ready"),
            ev_completed("resp-redaction-warmup-1"),
        ])],
    )
    .await;

    harness
        .submit("start the security redaction fixture")
        .await?;
    Ok(())
}

async fn drive_evidence_command(harness: &TestCodexHarness) -> Result<()> {
    mount_sse_sequence(
        harness.server(),
        vec![
            sse(vec![
                ev_response_created("resp-redaction-evidence-1"),
                ev_local_shell_call(
                    EVIDENCE_CALL_ID,
                    "completed",
                    vec!["/bin/sh", "-c", EVIDENCE_SCRIPT],
                ),
                ev_completed("resp-redaction-evidence-1"),
            ]),
            sse(vec![
                ev_assistant_message("msg-redaction-evidence-1", "evidence captured"),
                ev_completed("resp-redaction-evidence-2"),
            ]),
        ],
    )
    .await;

    harness
        .submit("run the required evidence command for security redaction")
        .await?;
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
            summary: "Add security and privacy redaction tests".to_string(),
            success_criteria: vec![
                "Receipts redact known secret patterns before persistence".to_string(),
                "Aegis Engine event logs redact known secret patterns".to_string(),
                "Harmless evidence context remains useful".to_string(),
            ],
        },
        linked_issue: Some(MethodLinkedIssue {
            provider: MethodIssueProvider::GitHub,
            repository: REPO.to_string(),
            number: TASK_ISSUE,
            title: Some("Task: Add security and privacy redaction tests".to_string()),
            url: Some(format!("https://github.com/{REPO}/issues/{TASK_ISSUE}")),
        }),
        status: MethodWorkStatus::Incomplete,
        claims: Vec::new(),
        assumptions: Vec::new(),
        falsifiers: Vec::new(),
        evidence_requirements: vec![MethodEvidenceRequirement {
            id: REQUIRED_EVIDENCE_ID.to_string(),
            summary: "Run the deterministic redaction evidence command".to_string(),
            required: true,
            commands: vec![EVIDENCE_SCRIPT.to_string()],
            claim_ids: Vec::new(),
            falsifier_ids: Vec::new(),
        }],
        evidence: Vec::new(),
        gates: Vec::new(),
        engine_alerts: Vec::new(),
        review_findings: Vec::new(),
        closure: None,
        resume_context: MethodResumeContext {
            repository: Some(REPO.to_string()),
            branch: Some("main".to_string()),
            commit: None,
            linked_issue: None,
            schema_version: Some(METHOD_STATE_SCHEMA_VERSION),
            sandbox_posture: None,
        },
        provenance: MethodProvenance {
            created_at_unix_seconds: 1,
            updated_at_unix_seconds: 1,
            source: MethodProvenanceSource::Agent,
            actor: Some("security-redaction-test".to_string()),
        },
    }
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

fn has_context(events: &[Value], key: &str, expected: &str) -> bool {
    events.iter().any(|event| {
        event
            .get("context")
            .and_then(|context| context.get(key))
            .and_then(Value::as_str)
            == Some(expected)
    })
}

fn assert_no_raw_secrets(label: &str, serialized: &str) {
    for secret in RAW_SECRETS {
        assert!(
            !serialized.contains(secret),
            "{label} leaked raw secret {secret}: {serialized}"
        );
    }
}
