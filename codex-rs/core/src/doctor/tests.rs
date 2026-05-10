use super::*;
use crate::context_packs::ContextPackDiagnostic;
use crate::context_packs::ContextPackKind;
use crate::context_packs::PromotionStatus;
use codex_model_provider_info::ANTHROPIC_DEFAULT_MODEL;
use codex_model_provider_info::ANTHROPIC_PROVIDER_ID;
use codex_protocol::method_state::MethodSandboxPolicyStatus;

fn selection_source(source: &str, detail: Option<&str>) -> ConfigSelectionSource {
    ConfigSelectionSource {
        source: source.to_string(),
        detail: detail.map(str::to_string),
    }
}

fn sandbox_diagnostic() -> SandboxDiagnostic {
    SandboxDiagnostic {
        posture: MethodSandboxPosture {
            mode: "workspace-write".to_string(),
            permission_profile: "workspace-write [workdir]".to_string(),
            enforcement: "managed".to_string(),
            network: "restricted".to_string(),
            policy: Some(MethodSandboxPolicySummary {
                status: MethodSandboxPolicyStatus::Allowed,
                allowed_modes: vec!["read-only".to_string(), "workspace-write".to_string()],
                source: Some("/tmp/requirements.toml".to_string()),
                diagnostic: Some(
                    "active sandbox mode `workspace-write` is allowed by policy".to_string(),
                ),
            }),
        },
        policy: MethodSandboxPolicySummary {
            status: MethodSandboxPolicyStatus::Allowed,
            allowed_modes: vec!["read-only".to_string(), "workspace-write".to_string()],
            source: Some("/tmp/requirements.toml".to_string()),
            diagnostic: Some(
                "active sandbox mode `workspace-write` is allowed by policy".to_string(),
            ),
        },
    }
}

#[test]
fn provider_catalog_default_model_uses_selected_provider_catalog() {
    assert_eq!(
        default_model_name_for_provider(ANTHROPIC_PROVIDER_ID),
        ANTHROPIC_DEFAULT_MODEL
    );
}

#[test]
fn human_report_lists_context_pack_status() {
    let report = DoctorReport {
        version: "0.1.0".to_string(),
        upstream_repository: "https://github.com/openai/codex".to_string(),
        upstream_base: "f7e8ff8e5026f92fc4b0be1478bf98f7ffcdd781".to_string(),
        source_revision: "1111111111111111111111111111111111111111".to_string(),
        codex_home: "/tmp/aegis-home".to_string(),
        cwd: "/tmp/project".to_string(),
        config_path: "/tmp/aegis-home/config.toml".to_string(),
        provider: ProviderDiagnostic {
            id: "openai-compatible".to_string(),
            name: "OpenAI compatible".to_string(),
            model: "gpt-5.4".to_string(),
            provider_source: selection_source("global_config", Some("model_provider")),
            model_source: selection_source("global_config", Some("model")),
            provider_policy: vec![ProviderPolicyDiagnostic {
                pack_id: "project:example".to_string(),
                kind: "project".to_string(),
                path: "/tmp/project/pack.toml".to_string(),
                provider_id: "openai-compatible".to_string(),
                field: "preferred".to_string(),
                status: "skipped_higher_precedence".to_string(),
                reason: "provider selected by global config".to_string(),
            }],
            wire_api: "responses".to_string(),
            base_url: Some("https://api.example.com/v1".to_string()),
            requires_openai_auth: false,
            supports_websockets: true,
            env_key: Some("OPENAI_API_KEY".to_string()),
            env_key_present: Some(true),
        },
        sandbox: sandbox_diagnostic(),
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
    assert!(output.contains("Version: 0.1.0"));
    assert!(output.contains("Upstream base: https://github.com/openai/codex f7e8ff8e"));
    assert!(output.contains("Source revision: 1111111111111111111111111111111111111111"));
    assert!(output.contains("Provider:"));
    assert!(output.contains("selected: openai-compatible (OpenAI compatible)"));
    assert!(output.contains("model: gpt-5.4"));
    assert!(output.contains("provider source: global_config (model_provider)"));
    assert!(output.contains("model source: global_config (model)"));
    assert!(output.contains("provider policy:"));
    assert!(output.contains("Sandbox:"));
    assert!(output.contains("mode: workspace-write"));
    assert!(output.contains("policy diagnostic: active sandbox mode"));
    assert!(output.contains("project:example preferred openai-compatible"));
    assert!(output.contains("wire API: responses"));
    assert!(output.contains("env key: OPENAI_API_KEY (set)"));
    assert!(output.contains("project:example"));
    assert!(output.contains("Aegis Engine alerts"));
    assert!(output.contains("active warnings: 1"));
    assert!(output.contains("schema v1"));
    assert!(output.contains("active"));
}

#[test]
fn human_report_makes_local_provider_endpoint_and_auth_clear() {
    let report = DoctorReport {
        version: "0.1.0".to_string(),
        upstream_repository: "https://github.com/openai/codex".to_string(),
        upstream_base: "f7e8ff8e5026f92fc4b0be1478bf98f7ffcdd781".to_string(),
        source_revision: "1111111111111111111111111111111111111111".to_string(),
        codex_home: "/tmp/aegis-home".to_string(),
        cwd: "/tmp/project".to_string(),
        config_path: "/tmp/aegis-home/config.toml".to_string(),
        provider: ProviderDiagnostic {
            id: "ollama".to_string(),
            name: "gpt-oss".to_string(),
            model: "gpt-oss:20b".to_string(),
            provider_source: selection_source("session_override", Some("model_provider override")),
            model_source: selection_source("local_provider_default", Some("ollama")),
            provider_policy: Vec::new(),
            wire_api: "responses".to_string(),
            base_url: Some("http://localhost:11434/v1".to_string()),
            requires_openai_auth: false,
            supports_websockets: false,
            env_key: None,
            env_key_present: None,
        },
        sandbox: sandbox_diagnostic(),
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

    let output = format_doctor_report_human(&report);

    assert!(output.contains("selected: ollama (gpt-oss)"));
    assert!(output.contains("model: gpt-oss:20b"));
    assert!(output.contains("wire API: responses"));
    assert!(output.contains("base URL: http://localhost:11434/v1"));
    assert!(output.contains("OpenAI auth: false, websockets: false"));
    assert!(output.contains("env key: none"));
}

#[test]
fn provider_report_serializes_env_key_presence_without_secret_value() {
    let report = DoctorReport {
        version: "0.1.0".to_string(),
        upstream_repository: "https://github.com/openai/codex".to_string(),
        upstream_base: "f7e8ff8e5026f92fc4b0be1478bf98f7ffcdd781".to_string(),
        source_revision: "1111111111111111111111111111111111111111".to_string(),
        codex_home: "/tmp/aegis-home".to_string(),
        cwd: "/tmp/project".to_string(),
        config_path: "/tmp/aegis-home/config.toml".to_string(),
        provider: ProviderDiagnostic {
            id: "openai-custom".to_string(),
            name: "OpenAI custom".to_string(),
            model: "gpt-5.4".to_string(),
            provider_source: selection_source("global_config", Some("model_provider")),
            model_source: selection_source("global_config", Some("model")),
            provider_policy: Vec::new(),
            wire_api: "responses".to_string(),
            base_url: Some("https://api.openai.com/v1".to_string()),
            requires_openai_auth: false,
            supports_websockets: false,
            env_key: Some("OPENAI_API_KEY".to_string()),
            env_key_present: Some(true),
        },
        sandbox: sandbox_diagnostic(),
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
    assert!(json.contains("\"upstream_base\":\"f7e8ff8e5026f92fc4b0be1478bf98f7ffcdd781\""));
    assert!(json.contains("\"source_revision\":\"1111111111111111111111111111111111111111\""));
    assert!(!json.contains("sk-"));
}

#[test]
fn provider_report_redacts_secret_base_url_material() {
    let report = DoctorReport {
        version: "0.1.0".to_string(),
        upstream_repository: "https://github.com/openai/codex".to_string(),
        upstream_base: "f7e8ff8e5026f92fc4b0be1478bf98f7ffcdd781".to_string(),
        source_revision: "1111111111111111111111111111111111111111".to_string(),
        codex_home: "/tmp/aegis-home".to_string(),
        cwd: "/tmp/project".to_string(),
        config_path: "/tmp/aegis-home/config.toml".to_string(),
        provider: ProviderDiagnostic {
            id: "openai-compatible".to_string(),
            name: "OpenAI compatible".to_string(),
            model: "gpt-5.4".to_string(),
            provider_source: selection_source("global_config", Some("model_provider")),
            model_source: selection_source("global_config", Some("model")),
            provider_policy: Vec::new(),
            wire_api: "responses".to_string(),
            base_url: Some(super::redact_provider_base_url(
                "https://user:secret-password@example.test/v1?api_key=sk-redaction-test&region=us#frag",
            )),
            requires_openai_auth: false,
            supports_websockets: false,
            env_key: Some("OPENAI_API_KEY".to_string()),
            env_key_present: Some(true),
        },
        sandbox: sandbox_diagnostic(),
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
    let human = format_doctor_report_human(&report);

    for rendered in [json.as_str(), human.as_str()] {
        assert!(!rendered.contains("secret-password"));
        assert!(!rendered.contains("sk-redaction-test"));
        assert!(rendered.contains("<redacted>"));
        assert!(rendered.contains("region=us"));
    }
}
