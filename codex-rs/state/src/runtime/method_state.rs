use super::*;
use codex_protocol::method_state::METHOD_STATE_SCHEMA_VERSION;
use codex_protocol::method_state::MethodState;

impl StateRuntime {
    pub async fn get_thread_method_state(
        &self,
        thread_id: ThreadId,
    ) -> anyhow::Result<Option<crate::ThreadMethodStateRecord>> {
        let row = sqlx::query(
            r#"
SELECT
    thread_id,
    schema_version,
    state_json,
    created_at_ms,
    updated_at_ms
FROM thread_method_states
WHERE thread_id = ?
            "#,
        )
        .bind(thread_id.to_string())
        .fetch_optional(self.pool.as_ref())
        .await?;

        row.map(|row| crate::model::ThreadMethodStateRow::try_from_row(&row)?.try_into())
            .transpose()
    }

    pub async fn upsert_thread_method_state(
        &self,
        thread_id: ThreadId,
        state: &MethodState,
    ) -> anyhow::Result<crate::ThreadMethodStateRecord> {
        anyhow::ensure!(
            state.schema_version == METHOD_STATE_SCHEMA_VERSION,
            "cannot persist method state for thread {thread_id}: schema version {} is incompatible with {METHOD_STATE_SCHEMA_VERSION}",
            state.schema_version
        );
        let state_json = serde_json::to_string(state)?;
        let now_ms = datetime_to_epoch_millis(Utc::now());
        let row = sqlx::query(
            r#"
INSERT INTO thread_method_states (
    thread_id,
    schema_version,
    state_json,
    created_at_ms,
    updated_at_ms
) VALUES (?, ?, ?, ?, ?)
ON CONFLICT(thread_id) DO UPDATE SET
    schema_version = excluded.schema_version,
    state_json = excluded.state_json,
    updated_at_ms = excluded.updated_at_ms
RETURNING
    thread_id,
    schema_version,
    state_json,
    created_at_ms,
    updated_at_ms
            "#,
        )
        .bind(thread_id.to_string())
        .bind(i64::from(METHOD_STATE_SCHEMA_VERSION))
        .bind(state_json)
        .bind(now_ms)
        .bind(now_ms)
        .fetch_one(self.pool.as_ref())
        .await?;

        crate::model::ThreadMethodStateRow::try_from_row(&row)?.try_into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::test_support::test_thread_metadata;
    use crate::runtime::test_support::unique_temp_dir;
    use codex_protocol::method_state::METHOD_EVIDENCE_RECEIPT_SCHEMA_VERSION;
    use codex_protocol::method_state::MethodAssumption;
    use codex_protocol::method_state::MethodClaim;
    use codex_protocol::method_state::MethodEvidence;
    use codex_protocol::method_state::MethodEvidenceExitStatus;
    use codex_protocol::method_state::MethodEvidenceGitState;
    use codex_protocol::method_state::MethodEvidenceGitStateStatus;
    use codex_protocol::method_state::MethodEvidenceKind;
    use codex_protocol::method_state::MethodEvidenceReceipt;
    use codex_protocol::method_state::MethodEvidenceRedactionStatus;
    use codex_protocol::method_state::MethodEvidenceRequirement;
    use codex_protocol::method_state::MethodEvidenceSessionMetadata;
    use codex_protocol::method_state::MethodFalsifier;
    use codex_protocol::method_state::MethodFalsifierStatus;
    use codex_protocol::method_state::MethodGate;
    use codex_protocol::method_state::MethodGateStatus;
    use codex_protocol::method_state::MethodIntent;
    use codex_protocol::method_state::MethodIssueProvider;
    use codex_protocol::method_state::MethodIssueRef;
    use codex_protocol::method_state::MethodLinkedIssue;
    use codex_protocol::method_state::MethodProvenance;
    use codex_protocol::method_state::MethodProvenanceSource;
    use codex_protocol::method_state::MethodResumeContext;
    use codex_protocol::method_state::MethodReviewFinding;
    use codex_protocol::method_state::MethodReviewFindingStatus;
    use codex_protocol::method_state::MethodReviewSeverity;
    use codex_protocol::method_state::MethodWorkStatus;
    use pretty_assertions::assert_eq;

    fn issue() -> MethodLinkedIssue {
        MethodLinkedIssue {
            provider: MethodIssueProvider::GitHub,
            repository: "mithran-hq/aegis-code".to_string(),
            number: 9,
            title: Some("Task: Implement method-state persistence".to_string()),
            url: Some("https://github.com/mithran-hq/aegis-code/issues/9".to_string()),
        }
    }

    fn sample_state() -> MethodState {
        let issue = issue();
        MethodState {
            schema_version: METHOD_STATE_SCHEMA_VERSION,
            intent: MethodIntent {
                summary: "Persist Aegis method state".to_string(),
                success_criteria: vec!["state survives runtime restart".to_string()],
            },
            linked_issue: Some(issue.clone()),
            status: MethodWorkStatus::Incomplete,
            claims: vec![MethodClaim {
                id: "claim:persisted".to_string(),
                summary: "Method state is stored outside conversation context".to_string(),
                evidence_ids: vec!["evidence:round-trip".to_string()],
            }],
            assumptions: vec![MethodAssumption {
                id: "assumption:sqlite".to_string(),
                summary: "SQLite thread state is available for this thread".to_string(),
                falsifier_ids: vec!["falsifier:missing-storage".to_string()],
            }],
            falsifiers: vec![MethodFalsifier {
                id: "falsifier:missing-storage".to_string(),
                summary: "No durable storage row exists after resume".to_string(),
                status: MethodFalsifierStatus::Open,
                evidence_ids: Vec::new(),
            }],
            evidence_requirements: vec![MethodEvidenceRequirement {
                id: "requirement:round-trip".to_string(),
                summary: "Persistence round-trip passes".to_string(),
                required: true,
                commands: Vec::new(),
                claim_ids: Vec::new(),
                falsifier_ids: Vec::new(),
            }],
            evidence: vec![MethodEvidence {
                id: "evidence:round-trip".to_string(),
                summary: "State runtime returned the same method payload".to_string(),
                kind: MethodEvidenceKind::Test,
                requirement_ids: vec!["requirement:round-trip".to_string()],
                claim_ids: vec!["claim:persisted".to_string()],
                falsifier_ids: vec!["falsifier:missing-storage".to_string()],
                source: Some("codex-state tests".to_string()),
                captured_at_unix_seconds: 1_779_999_000,
                receipt: Some(MethodEvidenceReceipt {
                    schema_version: METHOD_EVIDENCE_RECEIPT_SCHEMA_VERSION,
                    command: vec!["cargo".to_string(), "test".to_string()],
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
                    output_summary: "state runtime returned method payload".to_string(),
                    artifacts: Vec::new(),
                    session: MethodEvidenceSessionMetadata {
                        session_id: Some("session-1".to_string()),
                        thread_id: Some("thread-1".to_string()),
                        provider: Some("test-provider".to_string()),
                        model: None,
                        sandbox_posture: None,
                    },
                    redaction_status: MethodEvidenceRedactionStatus::NotNeeded,
                }),
            }],
            gates: vec![MethodGate {
                id: "gate:local-ci".to_string(),
                name: "Local CI".to_string(),
                status: MethodGateStatus::Pending,
                evidence_requirement_ids: vec!["requirement:round-trip".to_string()],
                rationale: None,
            }],
            review_findings: vec![MethodReviewFinding {
                id: "finding:none".to_string(),
                summary: "No blocking findings".to_string(),
                severity: MethodReviewSeverity::Info,
                status: MethodReviewFindingStatus::Addressed,
                claim_ids: vec!["claim:persisted".to_string()],
                evidence_ids: vec!["evidence:round-trip".to_string()],
                reviewed_at_unix_seconds: 1_779_999_100,
                reviewer: Some("codex".to_string()),
            }],
            closure: None,
            resume_context: MethodResumeContext {
                repository: Some("mithran-hq/aegis-code".to_string()),
                branch: Some("master".to_string()),
                commit: Some("abc123".to_string()),
                linked_issue: Some(MethodIssueRef::from(&issue)),
                schema_version: Some(METHOD_STATE_SCHEMA_VERSION),
                sandbox_posture: None,
            },
            provenance: MethodProvenance {
                created_at_unix_seconds: 1_779_998_000,
                updated_at_unix_seconds: 1_779_999_100,
                source: MethodProvenanceSource::Agent,
                actor: Some("codex".to_string()),
            },
        }
    }

    async fn seeded_runtime() -> (PathBuf, Arc<StateRuntime>, ThreadId) {
        let codex_home = unique_temp_dir();
        let runtime = StateRuntime::init(codex_home.clone(), "test-provider".to_string())
            .await
            .expect("init runtime");
        let thread_id = ThreadId::new();
        let metadata = test_thread_metadata(&codex_home, thread_id, codex_home.clone());
        runtime
            .upsert_thread(&metadata)
            .await
            .expect("seed thread metadata");
        (codex_home, runtime, thread_id)
    }

    #[tokio::test]
    async fn upsert_and_get_thread_method_state_round_trips_after_runtime_restart() {
        let (codex_home, runtime, thread_id) = seeded_runtime().await;
        let state = sample_state();

        let record = runtime
            .upsert_thread_method_state(thread_id, &state)
            .await
            .expect("upsert method state");
        assert_eq!(record.thread_id, thread_id);
        assert_eq!(record.schema_version, METHOD_STATE_SCHEMA_VERSION);
        assert_eq!(record.state, state);

        drop(runtime);
        let reloaded_runtime = StateRuntime::init(codex_home, "test-provider".to_string())
            .await
            .expect("reopen runtime");
        let loaded = reloaded_runtime
            .get_thread_method_state(thread_id)
            .await
            .expect("get method state")
            .expect("method state exists");
        assert_eq!(loaded.state, state);
    }

    #[tokio::test]
    async fn corrupt_state_json_returns_error() {
        let (_, runtime, thread_id) = seeded_runtime().await;
        let now_ms = datetime_to_epoch_millis(Utc::now());
        sqlx::query(
            r#"
INSERT INTO thread_method_states (
    thread_id,
    schema_version,
    state_json,
    created_at_ms,
    updated_at_ms
) VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(thread_id.to_string())
        .bind(i64::from(METHOD_STATE_SCHEMA_VERSION))
        .bind("{not json")
        .bind(now_ms)
        .bind(now_ms)
        .execute(runtime.pool.as_ref())
        .await
        .expect("insert corrupt row");

        let err = runtime
            .get_thread_method_state(thread_id)
            .await
            .expect_err("corrupt row is an error");
        assert!(
            format!("{err:#}").contains("corrupt method state JSON"),
            "{err:#}"
        );
    }

    #[tokio::test]
    async fn incompatible_schema_version_returns_error() {
        let (_, runtime, thread_id) = seeded_runtime().await;
        let state_json = serde_json::to_string(&sample_state()).expect("serialize state");
        let now_ms = datetime_to_epoch_millis(Utc::now());
        sqlx::query(
            r#"
INSERT INTO thread_method_states (
    thread_id,
    schema_version,
    state_json,
    created_at_ms,
    updated_at_ms
) VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(thread_id.to_string())
        .bind(i64::from(METHOD_STATE_SCHEMA_VERSION) + 1)
        .bind(state_json)
        .bind(now_ms)
        .bind(now_ms)
        .execute(runtime.pool.as_ref())
        .await
        .expect("insert incompatible row");

        let err = runtime
            .get_thread_method_state(thread_id)
            .await
            .expect_err("incompatible row is an error");
        assert!(
            format!("{err:#}").contains("incompatible method state schema version"),
            "{err:#}"
        );
    }

    #[tokio::test]
    async fn upsert_rejects_incompatible_payload_schema() {
        let (_, runtime, thread_id) = seeded_runtime().await;
        let mut state = sample_state();
        state.schema_version += 1;

        let err = runtime
            .upsert_thread_method_state(thread_id, &state)
            .await
            .expect_err("incompatible payload is rejected");
        assert!(format!("{err:#}").contains("schema version"), "{err:#}");
    }
}
