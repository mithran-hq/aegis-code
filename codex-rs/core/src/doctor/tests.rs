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
    assert!(output.contains("project:example"));
    assert!(output.contains("Aegis Engine alerts"));
    assert!(output.contains("active warnings: 1"));
    assert!(output.contains("schema v1"));
    assert!(output.contains("active"));
}
