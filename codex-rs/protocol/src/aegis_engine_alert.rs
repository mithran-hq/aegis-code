//! Aegis Engine alert records consumed by Aegis Code.

use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use ts_rs::TS;

pub const AEGIS_ENGINE_ALERT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct AegisEngineAlert {
    pub schema_version: u32,
    pub alert_id: String,
    pub severity: AegisEngineAlertSeverity,
    pub action: AegisEngineAlertAction,
    pub summary: String,
    pub created_at_unix_seconds: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub expires_at_unix_seconds: Option<i64>,
    pub source_event: AegisEngineAlertSourceEvent,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub candidate_guidance: Option<AegisEngineCandidateGuidance>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum AegisEngineAlertSeverity {
    Safe,
    Suspicious,
    Malicious,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum AegisEngineAlertAction {
    Observe,
    Warn,
    Block,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct AegisEngineAlertSourceEvent {
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

impl AegisEngineAlertSourceEvent {
    pub fn has_trace(&self) -> bool {
        self.event_id.is_some()
            || self.session_id.is_some()
            || self.thread_id.is_some()
            || self.turn_id.is_some()
            || self.call_id.is_some()
            || self.evidence_id.is_some()
            || self.finding_id.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct AegisEngineCandidateGuidance {
    pub guidance: String,
    #[serde(default)]
    pub falsifiers: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    fn alert(severity: AegisEngineAlertSeverity) -> AegisEngineAlert {
        AegisEngineAlert {
            schema_version: AEGIS_ENGINE_ALERT_SCHEMA_VERSION,
            alert_id: "alert-1".to_string(),
            severity,
            action: AegisEngineAlertAction::Warn,
            summary: "Suspicious tool behavior".to_string(),
            created_at_unix_seconds: 1_779_999_000,
            expires_at_unix_seconds: None,
            source_event: AegisEngineAlertSourceEvent {
                event_id: Some("event-1".to_string()),
                category: Some("tool_call".to_string()),
                session_id: None,
                thread_id: None,
                turn_id: Some("turn-1".to_string()),
                call_id: Some("call-1".to_string()),
                evidence_id: None,
                finding_id: None,
            },
            candidate_guidance: Some(AegisEngineCandidateGuidance {
                guidance: "Require issue evidence before this command.".to_string(),
                falsifiers: vec!["The command is read-only.".to_string()],
            }),
        }
    }

    #[test]
    fn alert_round_trips_for_supported_severities() {
        for severity in [
            AegisEngineAlertSeverity::Safe,
            AegisEngineAlertSeverity::Suspicious,
            AegisEngineAlertSeverity::Malicious,
        ] {
            let value = serde_json::to_value(alert(severity)).expect("serialize alert");
            let round_tripped: AegisEngineAlert =
                serde_json::from_value(value).expect("deserialize alert");
            assert_eq!(round_tripped.severity, severity);
            assert!(round_tripped.source_event.has_trace());
        }
    }

    #[test]
    fn malformed_unknown_fields_are_rejected() {
        let mut value = serde_json::to_value(alert(AegisEngineAlertSeverity::Suspicious))
            .expect("serialize alert");
        value["unknown"] = json!(true);

        let err = serde_json::from_value::<AegisEngineAlert>(value)
            .expect_err("unknown field should fail");

        assert!(err.to_string().contains("unknown field"));
    }
}
