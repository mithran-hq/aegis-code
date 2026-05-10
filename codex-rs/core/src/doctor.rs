use crate::aegis_engine_alerts::AegisEngineAlertDoctorStatus;
use crate::config::Config;
use crate::config::ConfigSelectionSource;
use crate::config::ProviderPolicyDiagnostic;
use crate::context_packs::ContextPackDiagnostic;
use crate::sandbox_policy::sandbox_policy_context;
use crate::sandbox_policy::sandbox_policy_summary;
use crate::sandbox_policy::sandbox_posture_from_context;
use codex_model_provider_info::AMAZON_BEDROCK_DEFAULT_MODEL;
use codex_model_provider_info::AMAZON_BEDROCK_PROVIDER_ID;
use codex_model_provider_info::ANTHROPIC_DEFAULT_MODEL;
use codex_model_provider_info::ANTHROPIC_PROVIDER_ID;
use codex_protocol::method_state::MethodSandboxPolicySummary;
use codex_protocol::method_state::MethodSandboxPosture;
use codex_protocol::openai_models::ModelPreset;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DoctorReport {
    pub version: String,
    pub upstream_repository: String,
    pub upstream_base: String,
    pub source_revision: String,
    pub codex_home: String,
    pub cwd: String,
    pub config_path: String,
    pub provider: ProviderDiagnostic,
    pub sandbox: SandboxDiagnostic,
    pub context_packs: Vec<ContextPackDiagnostic>,
    pub aegis_engine_alerts: AegisEngineAlertDoctorStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProviderDiagnostic {
    pub id: String,
    pub name: String,
    pub model: String,
    pub provider_source: ConfigSelectionSource,
    pub model_source: ConfigSelectionSource,
    pub provider_policy: Vec<ProviderPolicyDiagnostic>,
    pub wire_api: String,
    pub base_url: Option<String>,
    pub requires_openai_auth: bool,
    pub supports_websockets: bool,
    pub env_key: Option<String>,
    pub env_key_present: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SandboxDiagnostic {
    pub posture: MethodSandboxPosture,
    pub policy: MethodSandboxPolicySummary,
}

pub fn build_doctor_report(config: &Config) -> DoctorReport {
    let sandbox_context = sandbox_policy_context(
        config.permissions.permission_profile.get(),
        &config.config_layer_stack,
    );
    let posture = sandbox_posture_from_context(
        config.permissions.permission_profile.get(),
        config.cwd.as_path(),
        &sandbox_context,
    );
    let policy = sandbox_policy_summary(&sandbox_context);
    let version = crate::version::info();
    DoctorReport {
        version: version.release_version.to_string(),
        upstream_repository: version.upstream_repository.to_string(),
        upstream_base: version.upstream_base.to_string(),
        source_revision: version.source_revision.to_string(),
        codex_home: config.codex_home.display().to_string(),
        cwd: config.cwd.display().to_string(),
        config_path: config
            .codex_home
            .join("config.toml")
            .as_path()
            .display()
            .to_string(),
        provider: build_provider_diagnostic(config),
        sandbox: SandboxDiagnostic { posture, policy },
        context_packs: config.context_packs.diagnostics().to_vec(),
        aegis_engine_alerts: crate::aegis_engine_alerts::doctor_status(&config.aegis_engine),
    }
}

fn build_provider_diagnostic(config: &Config) -> ProviderDiagnostic {
    let env_key_present = config
        .model_provider
        .env_key
        .as_ref()
        .map(|env_key| std::env::var(env_key).is_ok_and(|value| !value.trim().is_empty()));

    ProviderDiagnostic {
        id: config.model_provider_id.clone(),
        name: config.model_provider.name.clone(),
        model: config
            .model
            .clone()
            .unwrap_or_else(|| default_model_name_for_provider(&config.model_provider_id)),
        provider_source: config.model_provider_source.clone(),
        model_source: config.model_source.clone(),
        provider_policy: config.provider_policy.clone(),
        wire_api: config.model_provider.wire_api.to_string(),
        base_url: config
            .model_provider
            .base_url
            .as_deref()
            .map(redact_provider_base_url),
        requires_openai_auth: config.model_provider.requires_openai_auth,
        supports_websockets: config.model_provider.supports_websockets,
        env_key: config.model_provider.env_key.clone(),
        env_key_present,
    }
}

fn redact_provider_base_url(raw: &str) -> String {
    let redacted_userinfo = redact_url_userinfo(raw);
    let Some((prefix, query_and_fragment)) = redacted_userinfo.split_once('?') else {
        return redacted_userinfo;
    };
    let (query, fragment) = query_and_fragment
        .split_once('#')
        .map(|(query, fragment)| (query, Some(fragment)))
        .unwrap_or((query_and_fragment, None));
    let redacted_query = query
        .split('&')
        .map(|part| {
            if let Some((name, _value)) = part.split_once('=')
                && is_sensitive_diagnostic_key(name)
            {
                return format!("{name}=<redacted>");
            }
            if is_sensitive_diagnostic_key(part) {
                return "<redacted>".to_string();
            }
            part.to_string()
        })
        .collect::<Vec<_>>()
        .join("&");
    match fragment {
        Some(fragment) => format!("{prefix}?{redacted_query}#{fragment}"),
        None => format!("{prefix}?{redacted_query}"),
    }
}

fn redact_url_userinfo(raw: &str) -> String {
    let Some(scheme_end) = raw.find("://") else {
        return raw.to_string();
    };
    let authority_start = scheme_end + "://".len();
    let authority_end = raw[authority_start..]
        .find(['/', '?', '#'])
        .map(|offset| authority_start + offset)
        .unwrap_or(raw.len());
    let authority = &raw[authority_start..authority_end];
    let Some(at_offset) = authority.rfind('@') else {
        return raw.to_string();
    };
    let at = authority_start + at_offset;
    format!("{}<redacted>{}", &raw[..authority_start], &raw[at..])
}

fn is_sensitive_diagnostic_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    lower.contains("token")
        || lower.contains("password")
        || lower.contains("secret")
        || lower.contains("authorization")
        || lower.contains("bearer")
        || lower.contains("api-key")
        || lower.contains("api_key")
        || lower.contains("apikey")
}

fn default_model_name_for_provider(provider_id: &str) -> String {
    match provider_id {
        ANTHROPIC_PROVIDER_ID => ANTHROPIC_DEFAULT_MODEL.to_string(),
        AMAZON_BEDROCK_PROVIDER_ID => AMAZON_BEDROCK_DEFAULT_MODEL.to_string(),
        _ => default_openai_catalog_model_name(),
    }
}

fn default_openai_catalog_model_name() -> String {
    let Ok(catalog) = codex_models_manager::bundled_models_response() else {
        return "default".to_string();
    };

    let mut models = catalog.models;
    models.sort_by(|a, b| a.priority.cmp(&b.priority));
    let mut presets = models
        .into_iter()
        .map(ModelPreset::from)
        .collect::<Vec<_>>();
    presets = ModelPreset::filter_by_auth(presets, /*chatgpt_mode*/ false);
    ModelPreset::mark_default_by_picker_visibility(&mut presets);
    presets
        .iter()
        .find(|model| model.is_default)
        .or_else(|| presets.first())
        .map(|model| model.model.clone())
        .unwrap_or_else(|| "default".to_string())
}

pub fn format_doctor_report_human(report: &DoctorReport) -> String {
    let mut output = String::new();
    output.push_str("Aegis Code Doctor\n");
    output.push_str(&format!("Version: {}\n", report.version));
    output.push_str(&format!(
        "Upstream base: {} {}\n",
        report.upstream_repository, report.upstream_base
    ));
    output.push_str(&format!("Source revision: {}\n", report.source_revision));
    output.push_str(&format!("Config: {}\n", report.config_path));
    output.push_str(&format!("Home: {}\n", report.codex_home));
    output.push_str(&format!("Working directory: {}\n", report.cwd));
    output.push_str("Provider:\n");
    output.push_str(&format!(
        "  selected: {} ({})\n",
        report.provider.id, report.provider.name
    ));
    output.push_str(&format!("  model: {}\n", report.provider.model));
    output.push_str(&format!(
        "  provider source: {}\n",
        selection_source_label(&report.provider.provider_source)
    ));
    output.push_str(&format!(
        "  model source: {}\n",
        selection_source_label(&report.provider.model_source)
    ));
    output.push_str(&format!("  wire API: {}\n", report.provider.wire_api));
    output.push_str(&format!(
        "  base URL: {}\n",
        report.provider.base_url.as_deref().unwrap_or("default")
    ));
    output.push_str(&format!(
        "  OpenAI auth: {}, websockets: {}\n",
        report.provider.requires_openai_auth, report.provider.supports_websockets
    ));
    match (
        report.provider.env_key.as_deref(),
        report.provider.env_key_present,
    ) {
        (Some(env_key), Some(true)) => {
            output.push_str(&format!("  env key: {env_key} (set)\n"));
        }
        (Some(env_key), Some(false)) => {
            output.push_str(&format!("  env key: {env_key} (missing)\n"));
        }
        _ => output.push_str("  env key: none\n"),
    }
    if !report.provider.provider_policy.is_empty() {
        output.push_str("  provider policy:\n");
        for policy in &report.provider.provider_policy {
            output.push_str(&format!(
                "    - {} {} {} -> {} ({})\n",
                policy.pack_id, policy.field, policy.provider_id, policy.status, policy.reason
            ));
        }
    }
    output.push_str("Sandbox:\n");
    output.push_str(&format!("  mode: {}\n", report.sandbox.posture.mode));
    output.push_str(&format!(
        "  permissions: {}\n",
        report.sandbox.posture.permission_profile
    ));
    output.push_str(&format!(
        "  enforcement: {}, network: {}\n",
        report.sandbox.posture.enforcement, report.sandbox.posture.network
    ));
    let allowed = if report.sandbox.policy.allowed_modes.is_empty() {
        "unrestricted".to_string()
    } else {
        report.sandbox.policy.allowed_modes.join(", ")
    };
    output.push_str(&format!(
        "  policy: {:?} ({})\n",
        report.sandbox.policy.status, allowed
    ));
    if let Some(source) = &report.sandbox.policy.source {
        output.push_str(&format!("  policy source: {source}\n"));
    }
    if let Some(diagnostic) = &report.sandbox.policy.diagnostic {
        output.push_str(&format!("  policy diagnostic: {diagnostic}\n"));
    }
    output.push_str("Aegis Engine alerts:\n");
    let alerts = &report.aegis_engine_alerts;
    output.push_str(&format!(
        "  enabled: {}, alerts: {}, candidate inputs: {}\n",
        alerts.enabled, alerts.alerts_path, alerts.candidate_inputs_path
    ));
    if let Some(err) = &alerts.last_read_error {
        output.push_str(&format!("  read error: {err}\n"));
    }
    output.push_str(&format!(
        "  active warnings: {}, active blocks: {}, malformed: {}, stale: {}\n",
        alerts.active_warning_count,
        alerts.active_blocking_count,
        alerts.malformed_count,
        alerts.stale_count
    ));
    output.push_str("Context packs:\n");

    if report.context_packs.is_empty() {
        output.push_str("  none configured\n");
        return output;
    }

    for pack in &report.context_packs {
        let state = if pack.active { "active" } else { "inactive" };
        let pack_id = pack.pack_id.as_deref().unwrap_or("unknown");
        let kind = pack
            .kind
            .map(|kind| format!("{kind:?}").to_ascii_lowercase())
            .unwrap_or_else(|| "unknown".to_string());
        let schema = pack
            .schema_version
            .map(|version| format!("schema v{version}"))
            .unwrap_or_else(|| "schema unknown".to_string());
        output.push_str(&format!(
            "  - {}: {} {} ({}, {}) - {}\n",
            pack.path, state, pack_id, kind, schema, pack.reason
        ));
    }

    output
}

fn selection_source_label(source: &ConfigSelectionSource) -> String {
    match source.detail.as_deref() {
        Some(detail) if !detail.is_empty() => format!("{} ({detail})", source.source),
        _ => source.source.clone(),
    }
}

#[cfg(test)]
mod tests;
