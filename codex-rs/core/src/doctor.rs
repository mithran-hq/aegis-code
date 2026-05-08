use crate::aegis_engine_alerts::AegisEngineAlertDoctorStatus;
use crate::config::Config;
use crate::context_packs::ContextPackDiagnostic;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DoctorReport {
    pub version: String,
    pub codex_home: String,
    pub cwd: String,
    pub config_path: String,
    pub context_packs: Vec<ContextPackDiagnostic>,
    pub aegis_engine_alerts: AegisEngineAlertDoctorStatus,
}

pub fn build_doctor_report(config: &Config) -> DoctorReport {
    DoctorReport {
        version: env!("CARGO_PKG_VERSION").to_string(),
        codex_home: config.codex_home.display().to_string(),
        cwd: config.cwd.display().to_string(),
        config_path: config
            .codex_home
            .join("config.toml")
            .as_path()
            .display()
            .to_string(),
        context_packs: config.context_packs.diagnostics().to_vec(),
        aegis_engine_alerts: crate::aegis_engine_alerts::doctor_status(&config.aegis_engine),
    }
}

pub fn format_doctor_report_human(report: &DoctorReport) -> String {
    let mut output = String::new();
    output.push_str("Aegis Code Doctor\n");
    output.push_str(&format!("Version: {}\n", report.version));
    output.push_str(&format!("Config: {}\n", report.config_path));
    output.push_str(&format!("Home: {}\n", report.codex_home));
    output.push_str(&format!("Working directory: {}\n", report.cwd));
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

#[cfg(test)]
mod tests;
