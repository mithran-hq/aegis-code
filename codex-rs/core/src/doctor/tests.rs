use super::*;
use crate::context_packs::ContextPackDiagnostic;
use crate::context_packs::ContextPackKind;
use crate::context_packs::PromotionStatus;

#[test]
fn human_report_lists_context_pack_status() {
    let report = DoctorReport {
        version: "0.1.0".to_string(),
        codex_home: "/tmp/aegis-home".to_string(),
        cwd: "/tmp/project".to_string(),
        config_path: "/tmp/aegis-home/config.toml".to_string(),
        provider: ProviderDiagnostic {
            id: "openai-compatible".to_string(),
            name: "OpenAI compatible".to_string(),
            model: "gpt-5.4".to_string(),
            wire_api: "responses".to_string(),
            base_url: Some("https://api.example.com/v1".to_string()),
            requires_openai_auth: false,
            supports_websockets: true,
            env_key: Some("OPENAI_API_KEY".to_string()),
            env_key_present: Some(true),
        },
        aegis_engine_alerts: crate::aegis_engine_alerts::AegisEngineAlertDoctorStatus {
            enabled: true,
            alerts_path: "/tmp/aegis-home/aegis-engine/alerts.jsonl".to_string(),
            candidate_inputs_path: "/tmp/aegis-home/aegis-engine/candidate-pack-inputs.jsonl"
                .to_string(),
            malformed_count: 0,
            stale_count: 0,
            active_warning_count: 1,
            active_blocking_count: 0,
            last_read_error: None,
        },
        context_packs: vec![ContextPackDiagnostic {
            path: "/tmp/project/pack.toml".to_string(),
            pack_id: Some("project:example".to_string()),
            kind: Some(ContextPackKind::Project),
            schema_version: Some(1),
            promotion_status: Some(PromotionStatus::Promoted),
            active: true,
            reason: "active".to_string(),
        }],
    };

    let output = format_doctor_report_human(&report);

    assert!(output.contains("Aegis Code Doctor"));
    assert!(output.contains("Provider:"));
    assert!(output.contains("selected: openai-compatible (OpenAI compatible)"));
    assert!(output.contains("model: gpt-5.4"));
    assert!(output.contains("wire API: responses"));
    assert!(output.contains("env key: OPENAI_API_KEY (set)"));
    assert!(output.contains("project:example"));
    assert!(output.contains("Aegis Engine alerts"));
    assert!(output.contains("active warnings: 1"));
    assert!(output.contains("schema v1"));
    assert!(output.contains("active"));
}

#[test]
fn provider_report_serializes_env_key_presence_without_secret_value() {
    let report = DoctorReport {
        version: "0.1.0".to_string(),
        codex_home: "/tmp/aegis-home".to_string(),
        cwd: "/tmp/project".to_string(),
        config_path: "/tmp/aegis-home/config.toml".to_string(),
        provider: ProviderDiagnostic {
            id: "openai-custom".to_string(),
            name: "OpenAI custom".to_string(),
            model: "gpt-5.4".to_string(),
            wire_api: "responses".to_string(),
            base_url: Some("https://api.openai.com/v1".to_string()),
            requires_openai_auth: false,
            supports_websockets: false,
            env_key: Some("OPENAI_API_KEY".to_string()),
            env_key_present: Some(true),
        },
        aegis_engine_alerts: crate::aegis_engine_alerts::AegisEngineAlertDoctorStatus {
            enabled: false,
            alerts_path: "/tmp/aegis-home/aegis-engine/alerts.jsonl".to_string(),
            candidate_inputs_path: "/tmp/aegis-home/aegis-engine/candidate-pack-inputs.jsonl"
                .to_string(),
            malformed_count: 0,
            stale_count: 0,
            active_warning_count: 0,
            active_blocking_count: 0,
            last_read_error: None,
        },
        context_packs: Vec::new(),
    };

    let json = serde_json::to_string(&report).expect("serialize report");

    assert!(json.contains("\"env_key\":\"OPENAI_API_KEY\""));
    assert!(json.contains("\"env_key_present\":true"));
    assert!(!json.contains("sk-"));
}
