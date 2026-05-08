use crate::config::AegisEngineConfig;
use codex_protocol::aegis_engine_alert::AEGIS_ENGINE_ALERT_SCHEMA_VERSION;
use codex_protocol::aegis_engine_alert::AegisEngineAlert;
use codex_protocol::aegis_engine_alert::AegisEngineAlertSeverity;
use codex_protocol::aegis_engine_alert::AegisEngineAlertSourceEvent;
use codex_protocol::aegis_engine_alert::AegisEngineCandidateGuidance;
use codex_protocol::aegis_safety_event::AegisSafetyEvent;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

#[derive(Clone)]
pub(crate) struct AegisEngineAlertIngestor {
    inner: Option<Arc<Mutex<AegisEngineAlertIngestorState>>>,
}

struct AegisEngineAlertIngestorState {
    alerts_path: PathBuf,
    candidate_inputs_path: PathBuf,
    stale_after_seconds: i64,
    seen_alert_ids: HashSet<String>,
    reported_malformed_lines: HashSet<usize>,
    observed_events: HashMap<String, AegisEngineAlertSourceEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AegisEngineAlertIngestResult {
    pub(crate) alerts: Vec<AppliedAegisEngineAlert>,
    pub(crate) diagnostics: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AppliedAegisEngineAlert {
    pub(crate) alert: AegisEngineAlert,
    pub(crate) received_at_unix_seconds: i64,
    pub(crate) candidate_input_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AegisEngineAlertDoctorStatus {
    pub enabled: bool,
    pub alerts_path: String,
    pub candidate_inputs_path: String,
    pub malformed_count: usize,
    pub stale_count: usize,
    pub active_warning_count: usize,
    pub active_blocking_count: usize,
    pub last_read_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct AegisEngineCandidatePackInput {
    schema_version: u32,
    input_id: String,
    alert_id: String,
    severity: AegisEngineAlertSeverity,
    summary: String,
    source_event: AegisEngineAlertSourceEvent,
    guidance: AegisEngineCandidateGuidance,
    created_at_unix_seconds: i64,
    received_at_unix_seconds: i64,
}

impl AegisEngineAlertIngestor {
    pub(crate) fn start(config: &AegisEngineConfig) -> Self {
        if !config.enabled {
            return Self { inner: None };
        }
        Self {
            inner: Some(Arc::new(Mutex::new(AegisEngineAlertIngestorState {
                alerts_path: config.alerts_path.clone(),
                candidate_inputs_path: config.candidate_inputs_path.clone(),
                stale_after_seconds: config.alert_stale_after_seconds,
                seen_alert_ids: HashSet::new(),
                reported_malformed_lines: HashSet::new(),
                observed_events: HashMap::new(),
            }))),
        }
    }

    pub(crate) async fn observe_event(&self, event: &AegisSafetyEvent) {
        let Some(inner) = &self.inner else {
            return;
        };
        let Some(source_event) = source_event_from_safety_event(event) else {
            return;
        };
        if let Some(event_id) = &source_event.event_id {
            inner
                .lock()
                .await
                .observed_events
                .insert(event_id.clone(), source_event);
        }
    }

    pub(crate) async fn ingest(
        &self,
        session_id: &str,
        thread_id: &str,
    ) -> AegisEngineAlertIngestResult {
        let Some(inner) = &self.inner else {
            return AegisEngineAlertIngestResult {
                alerts: Vec::new(),
                diagnostics: Vec::new(),
            };
        };
        let mut state = inner.lock().await;
        state.ingest(session_id, thread_id).await
    }
}

impl AegisEngineAlertIngestorState {
    async fn ingest(&mut self, session_id: &str, thread_id: &str) -> AegisEngineAlertIngestResult {
        let mut result = AegisEngineAlertIngestResult {
            alerts: Vec::new(),
            diagnostics: Vec::new(),
        };
        let contents = match tokio::fs::read_to_string(&self.alerts_path).await {
            Ok(contents) => contents,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return result,
            Err(err) => {
                result.diagnostics.push(format!(
                    "Aegis Engine alert ingestion failed: could not read `{}`: {err}",
                    self.alerts_path.display()
                ));
                return result;
            }
        };

        let now = now_unix_seconds();
        for (index, line) in contents.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let alert = match serde_json::from_str::<AegisEngineAlert>(trimmed) {
                Ok(alert) => alert,
                Err(err) => {
                    if self.reported_malformed_lines.insert(index + 1) {
                        result.diagnostics.push(format!(
                            "Aegis Engine alert ingestion skipped malformed alert line {} in `{}`: {err}",
                            index + 1,
                            self.alerts_path.display()
                        ));
                    }
                    continue;
                }
            };
            if !self.seen_alert_ids.insert(alert.alert_id.clone()) {
                continue;
            }
            if alert.schema_version != AEGIS_ENGINE_ALERT_SCHEMA_VERSION {
                result.diagnostics.push(format!(
                    "Aegis Engine alert `{}` has unsupported schema version {}; expected {}.",
                    alert.alert_id, alert.schema_version, AEGIS_ENGINE_ALERT_SCHEMA_VERSION
                ));
                continue;
            }
            if is_stale(&alert, now, self.stale_after_seconds) {
                result.diagnostics.push(format!(
                    "Aegis Engine alert `{}` is stale and was ignored.",
                    alert.alert_id
                ));
                continue;
            }
            if !alert.source_event.has_trace()
                || !self.correlates_with_session(&alert.source_event, session_id, thread_id)
            {
                result.diagnostics.push(format!(
                    "Aegis Engine alert `{}` could not be traced to this session and was ignored.",
                    alert.alert_id
                ));
                continue;
            }

            let candidate_input_id = if candidate_input_required(&alert) {
                match self.write_candidate_input(&alert, now).await {
                    Ok(input_id) => Some(input_id),
                    Err(err) => {
                        result.diagnostics.push(format!(
                            "Aegis Engine alert `{}` could not create candidate-pack input: {err}",
                            alert.alert_id
                        ));
                        None
                    }
                }
            } else {
                None
            };
            result.alerts.push(AppliedAegisEngineAlert {
                alert,
                received_at_unix_seconds: now,
                candidate_input_id,
            });
        }
        result
    }

    fn correlates_with_session(
        &self,
        source_event: &AegisEngineAlertSourceEvent,
        session_id: &str,
        thread_id: &str,
    ) -> bool {
        if source_event
            .session_id
            .as_ref()
            .is_some_and(|source_session_id| source_session_id != session_id)
        {
            return false;
        }
        if source_event
            .thread_id
            .as_ref()
            .is_some_and(|source_thread_id| source_thread_id != thread_id)
        {
            return false;
        }
        if source_event.session_id.as_deref() == Some(session_id)
            || source_event.thread_id.as_deref() == Some(thread_id)
        {
            return true;
        }
        if let Some(event_id) = &source_event.event_id {
            if self.observed_events.contains_key(event_id) {
                return true;
            }
        }
        self.observed_events
            .values()
            .any(|observed| source_events_match(source_event, observed))
    }

    async fn write_candidate_input(
        &self,
        alert: &AegisEngineAlert,
        received_at_unix_seconds: i64,
    ) -> io::Result<String> {
        let Some(guidance) = alert.candidate_guidance.clone() else {
            return Ok(String::new());
        };
        let input_id = format!("candidate-input:{}", alert.alert_id);
        let input = AegisEngineCandidatePackInput {
            schema_version: 1,
            input_id: input_id.clone(),
            alert_id: alert.alert_id.clone(),
            severity: alert.severity,
            summary: alert.summary.clone(),
            source_event: alert.source_event.clone(),
            guidance,
            created_at_unix_seconds: alert.created_at_unix_seconds,
            received_at_unix_seconds,
        };
        if let Some(parent) = self.candidate_inputs_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.candidate_inputs_path)
            .await?;
        let mut line = serde_json::to_vec(&input).map_err(io::Error::other)?;
        line.push(b'\n');
        file.write_all(&line).await?;
        file.flush().await?;
        Ok(input_id)
    }
}

pub fn doctor_status(config: &AegisEngineConfig) -> AegisEngineAlertDoctorStatus {
    let mut status = AegisEngineAlertDoctorStatus {
        enabled: config.enabled,
        alerts_path: config.alerts_path.display().to_string(),
        candidate_inputs_path: config.candidate_inputs_path.display().to_string(),
        malformed_count: 0,
        stale_count: 0,
        active_warning_count: 0,
        active_blocking_count: 0,
        last_read_error: None,
    };
    if !config.enabled {
        return status;
    }
    let contents = match std::fs::read_to_string(&config.alerts_path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return status,
        Err(err) => {
            status.last_read_error = Some(err.to_string());
            return status;
        }
    };
    let now = now_unix_seconds();
    for line in contents.lines().filter(|line| !line.trim().is_empty()) {
        let Ok(alert) = serde_json::from_str::<AegisEngineAlert>(line) else {
            status.malformed_count += 1;
            continue;
        };
        if alert.schema_version != AEGIS_ENGINE_ALERT_SCHEMA_VERSION {
            status.malformed_count += 1;
            continue;
        }
        if is_stale(&alert, now, config.alert_stale_after_seconds) {
            status.stale_count += 1;
            continue;
        }
        match alert.severity {
            AegisEngineAlertSeverity::Safe => {}
            AegisEngineAlertSeverity::Suspicious => status.active_warning_count += 1,
            AegisEngineAlertSeverity::Malicious => status.active_blocking_count += 1,
        }
    }
    status
}

fn source_event_from_safety_event(event: &AegisSafetyEvent) -> Option<AegisEngineAlertSourceEvent> {
    let event_id = event.event_id.clone()?;
    Some(AegisEngineAlertSourceEvent {
        event_id: Some(event_id),
        category: Some(event.category.tag_value().to_string()),
        session_id: string_context_value(&event.context, "session_id"),
        thread_id: string_context_value(&event.context, "thread_id"),
        turn_id: string_context_value(&event.context, "turn_id"),
        call_id: string_context_value(&event.context, "call_id"),
        evidence_id: string_context_value(&event.context, "evidence_id"),
        finding_id: string_context_value(&event.context, "finding_id"),
    })
}

fn string_context_value(context: &BTreeMap<String, Value>, key: &str) -> Option<String> {
    context
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn is_stale(alert: &AegisEngineAlert, now: i64, stale_after_seconds: i64) -> bool {
    if alert
        .expires_at_unix_seconds
        .is_some_and(|expires_at| expires_at < now)
    {
        return true;
    }
    now.saturating_sub(alert.created_at_unix_seconds) > stale_after_seconds
}

fn candidate_input_required(alert: &AegisEngineAlert) -> bool {
    alert.candidate_guidance.is_some()
        && matches!(
            alert.severity,
            AegisEngineAlertSeverity::Suspicious | AegisEngineAlertSeverity::Malicious
        )
}

fn source_events_match(
    alert: &AegisEngineAlertSourceEvent,
    observed: &AegisEngineAlertSourceEvent,
) -> bool {
    optional_field_matches(&alert.turn_id, &observed.turn_id)
        || optional_field_matches(&alert.call_id, &observed.call_id)
        || optional_field_matches(&alert.evidence_id, &observed.evidence_id)
        || optional_field_matches(&alert.finding_id, &observed.finding_id)
}

fn optional_field_matches(left: &Option<String>, right: &Option<String>) -> bool {
    left.as_ref()
        .is_some_and(|left| right.as_ref() == Some(left))
}

fn now_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_protocol::aegis_engine_alert::AegisEngineAlertAction;
    use codex_protocol::aegis_engine_alert::AegisEngineCandidateGuidance;
    use std::path::Path;
    use tempfile::TempDir;

    fn config(dir: &TempDir) -> AegisEngineConfig {
        AegisEngineConfig {
            enabled: true,
            jsonl_path: dir.path().join("events.jsonl"),
            alerts_path: dir.path().join("alerts.jsonl"),
            candidate_inputs_path: dir.path().join("candidate-pack-inputs.jsonl"),
            alert_stale_after_seconds: 86_400,
            buffer_capacity: 16,
            failure_mode: crate::config::AegisEngineFailureMode::BestEffort,
            mirror: crate::config::AegisEngineMirrorConfig::None,
        }
    }

    fn alert(id: &str, severity: AegisEngineAlertSeverity, created_at: i64) -> AegisEngineAlert {
        AegisEngineAlert {
            schema_version: AEGIS_ENGINE_ALERT_SCHEMA_VERSION,
            alert_id: id.to_string(),
            severity,
            action: match severity {
                AegisEngineAlertSeverity::Safe => AegisEngineAlertAction::Observe,
                AegisEngineAlertSeverity::Suspicious => AegisEngineAlertAction::Warn,
                AegisEngineAlertSeverity::Malicious => AegisEngineAlertAction::Block,
            },
            summary: format!("{severity:?} alert"),
            created_at_unix_seconds: created_at,
            expires_at_unix_seconds: None,
            source_event: AegisEngineAlertSourceEvent {
                event_id: None,
                category: Some("tool_call".to_string()),
                session_id: Some("session-1".to_string()),
                thread_id: None,
                turn_id: Some("turn-1".to_string()),
                call_id: Some("call-1".to_string()),
                evidence_id: None,
                finding_id: None,
            },
            candidate_guidance: Some(AegisEngineCandidateGuidance {
                guidance: "Prefer safer workflow.".to_string(),
                falsifiers: vec!["No workflow risk exists.".to_string()],
            }),
        }
    }

    async fn write_alerts(path: &Path, lines: &[String]) {
        tokio::fs::write(path, lines.join("\n"))
            .await
            .expect("write alerts");
    }

    #[tokio::test]
    async fn ingests_safe_suspicious_and_malicious_alerts() {
        let dir = TempDir::new().expect("tempdir");
        let config = config(&dir);
        let now = now_unix_seconds();
        write_alerts(
            &config.alerts_path,
            &[
                serde_json::to_string(&alert("safe", AegisEngineAlertSeverity::Safe, now))
                    .expect("safe"),
                serde_json::to_string(&alert(
                    "suspicious",
                    AegisEngineAlertSeverity::Suspicious,
                    now,
                ))
                .expect("suspicious"),
                serde_json::to_string(&alert(
                    "malicious",
                    AegisEngineAlertSeverity::Malicious,
                    now,
                ))
                .expect("malicious"),
            ],
        )
        .await;

        let ingestor = AegisEngineAlertIngestor::start(&config);
        let result = ingestor.ingest("session-1", "thread-1").await;

        assert_eq!(result.alerts.len(), 3);
        assert!(result.diagnostics.is_empty());
        let candidate_inputs = tokio::fs::read_to_string(&config.candidate_inputs_path)
            .await
            .expect("candidate inputs");
        assert_eq!(candidate_inputs.lines().count(), 2);
    }

    #[tokio::test]
    async fn malformed_and_stale_alerts_do_not_crash_or_apply() {
        let dir = TempDir::new().expect("tempdir");
        let config = config(&dir);
        let stale = now_unix_seconds() - 90_000;
        write_alerts(
            &config.alerts_path,
            &[
                "{not-json".to_string(),
                serde_json::to_string(&alert("stale", AegisEngineAlertSeverity::Malicious, stale))
                    .expect("stale"),
            ],
        )
        .await;

        let ingestor = AegisEngineAlertIngestor::start(&config);
        let result = ingestor.ingest("session-1", "thread-1").await;

        assert!(result.alerts.is_empty());
        assert_eq!(result.diagnostics.len(), 2);
    }

    #[tokio::test]
    async fn unmatched_alert_is_diagnostic_only() {
        let dir = TempDir::new().expect("tempdir");
        let config = config(&dir);
        let mut alert = alert(
            "unmatched",
            AegisEngineAlertSeverity::Suspicious,
            now_unix_seconds(),
        );
        alert.source_event.session_id = Some("other-session".to_string());
        write_alerts(
            &config.alerts_path,
            &[serde_json::to_string(&alert).expect("alert")],
        )
        .await;

        let ingestor = AegisEngineAlertIngestor::start(&config);
        let result = ingestor.ingest("session-1", "thread-1").await;

        assert!(result.alerts.is_empty());
        assert!(result.diagnostics[0].contains("could not be traced"));
    }
}
