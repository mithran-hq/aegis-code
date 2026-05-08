use codex_config::CONFIG_TOML_FILE;
use codex_config::config_toml::ConfigToml;
use codex_utils_path::write_atomically;
use serde::Serialize;
use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::io::ErrorKind;
use std::path::Path;
use std::path::PathBuf;
use toml::Value as TomlValue;
use toml::map::Map as TomlMap;
use toml_edit::DocumentMut;
use toml_edit::Item as TomlItem;
use toml_edit::Table as TomlEditTable;

const SAFE_TOP_LEVEL_KEYS: &[&str] = &[
    "model",
    "review_model",
    "model_provider",
    "model_context_window",
    "model_auto_compact_token_limit",
    "approval_policy",
    "approvals_reviewer",
    "include_permissions_instructions",
    "include_apps_instructions",
    "include_environment_context",
    "allow_login_shell",
    "sandbox_mode",
    "sandbox_workspace_write",
    "default_permissions",
    "permissions",
    "profile",
    "history",
    "file_opener",
    "tui",
    "hide_agent_reasoning",
    "show_raw_agent_reasoning",
    "model_reasoning_effort",
    "plan_mode_reasoning_effort",
    "model_reasoning_summary",
    "model_verbosity",
    "model_supports_reasoning_summaries",
    "personality",
    "service_tier",
    "chatgpt_base_url",
    "openai_base_url",
    "audio",
    "web_search",
    "tools",
    "tool_suggest",
    "features",
    "suppress_unstable_features_warning",
    "project_root_markers",
    "check_for_update_on_startup",
    "disable_paste_burst",
    "analytics",
    "feedback",
    "apps",
    "windows",
    "windows_wsl_setup_acknowledged",
    "experimental_use_unified_exec_tool",
    "experimental_use_freeform_apply_patch",
    "oss_provider",
];

const PROMPT_KEYS: &[&str] = &[
    "instructions",
    "developer_instructions",
    "compact_prompt",
    "experimental_realtime_ws_backend_prompt",
    "experimental_realtime_ws_startup_context",
    "experimental_realtime_start_instructions",
];

const PROMPT_FILE_KEYS: &[&str] = &[
    "model_instructions_file",
    "context_pack_paths",
    "experimental_instructions_file",
    "experimental_compact_prompt_file",
];

const SAFE_PROVIDER_KEYS: &[&str] = &[
    "name",
    "base_url",
    "env_key",
    "env_key_instructions",
    "wire_api",
    "env_http_headers",
    "request_max_retries",
    "stream_max_retries",
    "stream_idle_timeout_ms",
    "websocket_connect_timeout_ms",
    "requires_openai_auth",
    "supports_websockets",
    "aws",
];

const SECRET_PROVIDER_KEYS: &[&str] = &[
    "experimental_bearer_token",
    "auth",
    "http_headers",
    "query_params",
];

const SAFE_PROFILE_KEYS: &[&str] = &[
    "model",
    "service_tier",
    "model_provider",
    "approval_policy",
    "approvals_reviewer",
    "sandbox_mode",
    "model_reasoning_effort",
    "plan_mode_reasoning_effort",
    "model_reasoning_summary",
    "model_verbosity",
    "personality",
    "chatgpt_base_url",
    "include_apply_patch_tool",
    "include_permissions_instructions",
    "include_apps_instructions",
    "include_environment_context",
    "experimental_use_unified_exec_tool",
    "experimental_use_freeform_apply_patch",
    "tools_view_image",
    "tools",
    "web_search",
    "analytics",
    "tui",
    "windows",
    "features",
    "oss_provider",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexConfigImportOptions {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub include_prompts: bool,
    pub apply: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct CodexConfigImportReport {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub source_exists: bool,
    pub applied: bool,
    pub changed: bool,
    pub imports: Vec<ConfigImportEntry>,
    pub skipped: Vec<ConfigSkipEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConfigImportEntry {
    pub key_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConfigSkipEntry {
    pub key_path: String,
    pub reason: String,
}

#[derive(Debug, Clone)]
struct Candidate {
    path: Vec<String>,
    value: TomlValue,
}

pub fn default_codex_config_path() -> io::Result<PathBuf> {
    let mut home = dirs::home_dir()
        .ok_or_else(|| io::Error::new(ErrorKind::NotFound, "could not find home directory"))?;
    home.push(".codex");
    home.push(CONFIG_TOML_FILE);
    Ok(home)
}

pub fn preview_codex_config_import(
    options: &CodexConfigImportOptions,
) -> io::Result<CodexConfigImportReport> {
    build_codex_config_import(&CodexConfigImportOptions {
        apply: false,
        ..options.clone()
    })
}

pub fn apply_codex_config_import(
    options: &CodexConfigImportOptions,
) -> io::Result<CodexConfigImportReport> {
    build_codex_config_import(&CodexConfigImportOptions {
        apply: true,
        ..options.clone()
    })
}

fn build_codex_config_import(
    options: &CodexConfigImportOptions,
) -> io::Result<CodexConfigImportReport> {
    let mut report = CodexConfigImportReport {
        source: options.source.clone(),
        destination: options.destination.clone(),
        source_exists: false,
        applied: options.apply,
        changed: false,
        imports: Vec::new(),
        skipped: Vec::new(),
    };

    let source_raw = match fs::read_to_string(&options.source) {
        Ok(raw) => raw,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(report),
        Err(err) => return Err(err),
    };
    report.source_exists = true;

    let source = parse_toml_value(&source_raw, "source Codex config")?;
    let Some(source_table) = source.as_table() else {
        return Err(invalid_data(
            "source Codex config root must be a TOML table",
        ));
    };

    let destination_raw = read_optional_to_string(&options.destination)?;
    let mut destination_doc = parse_destination_doc(destination_raw.as_deref())?;
    let mut destination_value = match destination_raw.as_deref() {
        Some(raw) if !raw.trim().is_empty() => parse_toml_value(raw, "destination Aegis config")?,
        _ => TomlValue::Table(TomlMap::new()),
    };

    let candidates = collect_candidates(source_table, options.include_prompts, &mut report);
    for candidate in candidates {
        let key_path = join_path(&candidate.path);
        if has_value_path(&destination_value, &candidate.path) {
            report.skipped.push(ConfigSkipEntry {
                key_path,
                reason: "destination already defines this setting".to_string(),
            });
            continue;
        }
        if is_secret_path(&candidate.path) {
            report.skipped.push(ConfigSkipEntry {
                key_path,
                reason: "secret-like setting is not imported".to_string(),
            });
            continue;
        }
        set_value_path(
            &mut destination_value,
            &candidate.path,
            candidate.value.clone(),
        )?;
        set_document_path(&mut destination_doc, &candidate.path, &candidate.value)?;
        report.imports.push(ConfigImportEntry { key_path });
    }
    report.changed = !report.imports.is_empty();

    if options.apply && report.changed {
        let rendered = destination_doc.to_string();
        validate_aegis_config(&rendered)?;
        write_atomically(&options.destination, &rendered)?;
    }

    Ok(report)
}

fn collect_candidates(
    source_table: &TomlMap<String, TomlValue>,
    include_prompts: bool,
    report: &mut CodexConfigImportReport,
) -> Vec<Candidate> {
    let safe_top_level = str_set(SAFE_TOP_LEVEL_KEYS);
    let prompt_keys = str_set(PROMPT_KEYS);
    let prompt_file_keys = str_set(PROMPT_FILE_KEYS);
    let mut candidates = Vec::new();

    for (key, value) in source_table {
        if key == "model_providers" {
            collect_model_provider_candidates(value, report, &mut candidates);
        } else if key == "profiles" {
            collect_profile_candidates(value, report, &mut candidates);
        } else if safe_top_level.contains(key.as_str()) {
            flatten_value(vec![key.clone()], value, &mut candidates);
        } else if prompt_keys.contains(key.as_str()) {
            if include_prompts {
                candidates.push(Candidate {
                    path: vec![key.clone()],
                    value: value.clone(),
                });
            } else {
                report.skipped.push(ConfigSkipEntry {
                    key_path: key.clone(),
                    reason: "prompt setting requires --include-prompts".to_string(),
                });
            }
        } else if key == "auto_review" {
            if include_prompts {
                flatten_value(vec![key.clone()], value, &mut candidates);
            } else {
                report.skipped.push(ConfigSkipEntry {
                    key_path: key.clone(),
                    reason: "prompt setting requires --include-prompts".to_string(),
                });
            }
        } else if prompt_file_keys.contains(key.as_str()) {
            report.skipped.push(ConfigSkipEntry {
                key_path: key.clone(),
                reason: "prompt file paths are not imported".to_string(),
            });
        } else {
            report.skipped.push(ConfigSkipEntry {
                key_path: key.clone(),
                reason: "unsupported setting is not imported".to_string(),
            });
        }
    }

    candidates
}

fn collect_model_provider_candidates(
    value: &TomlValue,
    report: &mut CodexConfigImportReport,
    candidates: &mut Vec<Candidate>,
) {
    let Some(providers) = value.as_table() else {
        report.skipped.push(ConfigSkipEntry {
            key_path: "model_providers".to_string(),
            reason: "unsupported setting shape is not imported".to_string(),
        });
        return;
    };
    let safe_provider = str_set(SAFE_PROVIDER_KEYS);
    let secret_provider = str_set(SECRET_PROVIDER_KEYS);
    for (provider_name, provider_value) in providers {
        let Some(provider_table) = provider_value.as_table() else {
            report.skipped.push(ConfigSkipEntry {
                key_path: format!("model_providers.{provider_name}"),
                reason: "unsupported provider shape is not imported".to_string(),
            });
            continue;
        };
        for (field_name, field_value) in provider_table {
            let key_path = format!("model_providers.{provider_name}.{field_name}");
            if secret_provider.contains(field_name.as_str()) || is_secret_segment(field_name) {
                report.skipped.push(ConfigSkipEntry {
                    key_path,
                    reason: "provider secret material is not imported".to_string(),
                });
            } else if safe_provider.contains(field_name.as_str()) {
                flatten_value(
                    vec![
                        "model_providers".to_string(),
                        provider_name.clone(),
                        field_name.clone(),
                    ],
                    field_value,
                    candidates,
                );
            } else {
                report.skipped.push(ConfigSkipEntry {
                    key_path,
                    reason: "unsupported provider setting is not imported".to_string(),
                });
            }
        }
    }
}

fn collect_profile_candidates(
    value: &TomlValue,
    report: &mut CodexConfigImportReport,
    candidates: &mut Vec<Candidate>,
) {
    let Some(profiles) = value.as_table() else {
        report.skipped.push(ConfigSkipEntry {
            key_path: "profiles".to_string(),
            reason: "unsupported setting shape is not imported".to_string(),
        });
        return;
    };
    let safe_profile = str_set(SAFE_PROFILE_KEYS);
    let prompt_file = str_set(PROMPT_FILE_KEYS);
    for (profile_name, profile_value) in profiles {
        let Some(profile_table) = profile_value.as_table() else {
            report.skipped.push(ConfigSkipEntry {
                key_path: format!("profiles.{profile_name}"),
                reason: "unsupported profile shape is not imported".to_string(),
            });
            continue;
        };
        for (field_name, field_value) in profile_table {
            let key_path = format!("profiles.{profile_name}.{field_name}");
            if prompt_file.contains(field_name.as_str()) {
                report.skipped.push(ConfigSkipEntry {
                    key_path,
                    reason: "prompt file paths are not imported".to_string(),
                });
            } else if safe_profile.contains(field_name.as_str()) {
                flatten_value(
                    vec![
                        "profiles".to_string(),
                        profile_name.clone(),
                        field_name.clone(),
                    ],
                    field_value,
                    candidates,
                );
            } else {
                report.skipped.push(ConfigSkipEntry {
                    key_path,
                    reason: "unsupported profile setting is not imported".to_string(),
                });
            }
        }
    }
}

fn flatten_value(prefix: Vec<String>, value: &TomlValue, candidates: &mut Vec<Candidate>) {
    match value {
        TomlValue::Table(table) => {
            for (key, child) in table {
                let mut path = prefix.clone();
                path.push(key.clone());
                flatten_value(path, child, candidates);
            }
        }
        _ => candidates.push(Candidate {
            path: prefix,
            value: value.clone(),
        }),
    }
}

fn parse_toml_value(raw: &str, label: &str) -> io::Result<TomlValue> {
    toml::from_str::<TomlValue>(raw).map_err(|err| invalid_data(format!("invalid {label}: {err}")))
}

fn parse_destination_doc(raw: Option<&str>) -> io::Result<DocumentMut> {
    match raw {
        Some(raw) if !raw.trim().is_empty() => raw
            .parse::<DocumentMut>()
            .map_err(|err| invalid_data(format!("invalid destination Aegis config: {err}"))),
        _ => Ok(DocumentMut::new()),
    }
}

fn read_optional_to_string(path: &Path) -> io::Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(raw) => Ok(Some(raw)),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

fn validate_aegis_config(raw: &str) -> io::Result<()> {
    toml::from_str::<ConfigToml>(raw)
        .map(|_| ())
        .map_err(|err| invalid_data(format!("import would create invalid Aegis config: {err}")))
}

fn has_value_path(value: &TomlValue, path: &[String]) -> bool {
    let mut current = value;
    for segment in path {
        let Some(table) = current.as_table() else {
            return false;
        };
        let Some(next) = table.get(segment) else {
            return false;
        };
        current = next;
    }
    true
}

fn set_value_path(value: &mut TomlValue, path: &[String], incoming: TomlValue) -> io::Result<()> {
    let mut current = value;
    for segment in &path[..path.len().saturating_sub(1)] {
        let table = current
            .as_table_mut()
            .ok_or_else(|| invalid_data(format!("destination path {} is not a table", segment)))?;
        current = table
            .entry(segment.clone())
            .or_insert_with(|| TomlValue::Table(TomlMap::new()));
    }
    let Some(last) = path.last() else {
        return Err(invalid_data("empty config key path"));
    };
    let table = current
        .as_table_mut()
        .ok_or_else(|| invalid_data("destination path parent is not a table"))?;
    table.insert(last.clone(), incoming);
    Ok(())
}

fn set_document_path(doc: &mut DocumentMut, path: &[String], value: &TomlValue) -> io::Result<()> {
    let mut table = doc.as_table_mut();
    for segment in &path[..path.len().saturating_sub(1)] {
        table = ensure_child_table(table, segment)?;
    }
    let Some(last) = path.last() else {
        return Err(invalid_data("empty config key path"));
    };
    table.insert(last, toml_value_to_item(value)?);
    Ok(())
}

fn ensure_child_table<'a>(
    table: &'a mut TomlEditTable,
    segment: &str,
) -> io::Result<&'a mut TomlEditTable> {
    if !table.contains_key(segment) {
        table.insert(segment, TomlItem::Table(TomlEditTable::new()));
    }
    table
        .get_mut(segment)
        .and_then(TomlItem::as_table_mut)
        .ok_or_else(|| invalid_data(format!("destination path {segment} is not a table")))
}

fn toml_value_to_item(value: &TomlValue) -> io::Result<TomlItem> {
    let mut wrapper = TomlMap::new();
    wrapper.insert("value".to_string(), value.clone());
    let raw = toml::to_string(&TomlValue::Table(wrapper))
        .map_err(|err| invalid_data(format!("failed to serialize imported value: {err}")))?;
    let mut doc = raw
        .parse::<DocumentMut>()
        .map_err(|err| invalid_data(format!("failed to render imported value: {err}")))?;
    doc.as_table_mut()
        .remove("value")
        .ok_or_else(|| invalid_data("failed to render imported value"))
}

fn is_secret_path(path: &[String]) -> bool {
    if path.iter().any(|segment| segment == "env_http_headers") {
        return false;
    }
    path.last()
        .is_some_and(|segment| is_secret_segment(segment))
}

fn is_secret_segment(segment: &str) -> bool {
    let lower = segment.to_ascii_lowercase();
    lower.contains("password")
        || lower.contains("secret")
        || lower.contains("bearer")
        || lower == "token"
        || lower.ends_with("_token")
        || lower.contains("access_token")
        || lower.contains("refresh_token")
        || lower.contains("authorization")
        || lower == "auth"
        || lower == "http_headers"
        || lower == "query_params"
        || lower.contains("api_key")
}

fn join_path(path: &[String]) -> String {
    path.join(".")
}

fn str_set<'a>(values: &'a [&'a str]) -> BTreeSet<&'a str> {
    values.iter().copied().collect()
}

fn invalid_data(message: impl Into<String>) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn options(source: PathBuf, destination: PathBuf, apply: bool) -> CodexConfigImportOptions {
        CodexConfigImportOptions {
            source,
            destination,
            include_prompts: false,
            apply,
        }
    }

    #[test]
    fn preview_missing_codex_config_does_not_write_destination() {
        let temp = TempDir::new().expect("temp dir");
        let source = temp.path().join(".codex").join("config.toml");
        let destination = temp.path().join(".aegis").join("config.toml");

        let report =
            preview_codex_config_import(&options(source, destination.clone(), false)).unwrap();

        assert!(!report.source_exists);
        assert!(!report.changed);
        assert!(!destination.exists());
    }

    #[test]
    fn valid_config_imports_safe_settings_and_preserves_existing_values() {
        let temp = TempDir::new().expect("temp dir");
        let source = temp.path().join(".codex").join("config.toml");
        let destination = temp.path().join(".aegis").join("config.toml");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::write(
            &source,
            r#"
model = "gpt-5"
model_auto_compact_token_limit = 100000
approval_policy = "on-request"
instructions = "do not import by default"

[model_providers.safe]
name = "Safe"
base_url = "https://example.test/v1"
env_key = "SAFE_API_KEY"
wire_api = "responses"

[profiles.fast]
model = "gpt-5-mini"
sandbox_mode = "workspace-write"
model_instructions_file = "/tmp/prompt.md"
"#,
        )
        .unwrap();
        fs::write(&destination, "model = \"existing\"\n").unwrap();

        let report =
            apply_codex_config_import(&options(source, destination.clone(), true)).unwrap();

        assert!(report.changed);
        assert!(report.skipped.iter().any(|entry| entry.key_path == "model"));
        let written = fs::read_to_string(destination).unwrap();
        assert!(written.contains("model = \"existing\""));
        assert!(written.contains("model_auto_compact_token_limit = 100000"));
        assert!(written.contains("approval_policy = \"on-request\""));
        assert!(written.contains("[model_providers.safe]"));
        assert!(written.contains("env_key = \"SAFE_API_KEY\""));
        assert!(written.contains("[profiles.fast]"));
        assert!(written.contains("sandbox_mode = \"workspace-write\""));
        assert!(!written.contains("do not import by default"));
        assert!(!written.contains("prompt.md"));
    }

    #[test]
    fn unsupported_config_is_reported_and_not_imported() {
        let temp = TempDir::new().expect("temp dir");
        let source = temp.path().join("config.toml");
        let destination = temp.path().join("aegis.toml");
        fs::write(
            &source,
            r#"
model = "gpt-5"
mcp_servers = { docs = { command = "secret-command" } }
projects = { "/tmp/repo" = { trust_level = "trusted" } }
"#,
        )
        .unwrap();

        let report =
            apply_codex_config_import(&options(source, destination.clone(), true)).unwrap();

        assert!(report.changed);
        assert!(
            report
                .skipped
                .iter()
                .any(|entry| entry.key_path == "mcp_servers")
        );
        assert!(
            report
                .skipped
                .iter()
                .any(|entry| entry.key_path == "projects")
        );
        let written = fs::read_to_string(destination).unwrap();
        assert!(written.contains("model = \"gpt-5\""));
        assert!(!written.contains("mcp_servers"));
        assert!(!written.contains("trust_level"));
    }

    #[test]
    fn secrets_are_redacted_from_report_and_not_written() {
        let temp = TempDir::new().expect("temp dir");
        let source = temp.path().join("config.toml");
        let destination = temp.path().join("aegis.toml");
        fs::write(
            &source,
            r#"
[model_providers.secretful]
name = "Secretful"
base_url = "https://example.test/v1"
experimental_bearer_token = "raw-secret"
http_headers = { Authorization = "Bearer raw-secret" }
query_params = { api_key = "raw-secret" }
env_http_headers = { Authorization = "SAFE_ENV_NAME" }
"#,
        )
        .unwrap();

        let report =
            apply_codex_config_import(&options(source, destination.clone(), true)).unwrap();
        let report_json = serde_json::to_string(&report).unwrap();
        let written = fs::read_to_string(destination).unwrap();

        assert!(written.contains("base_url"));
        assert!(written.contains("env_http_headers"));
        assert!(!written.contains("raw-secret"));
        assert!(!written.contains("experimental_bearer_token"));
        assert!(!written.contains("http_headers ="));
        assert!(!report_json.contains("raw-secret"));
    }

    #[test]
    fn invalid_import_does_not_replace_existing_destination() {
        let temp = TempDir::new().expect("temp dir");
        let source = temp.path().join("config.toml");
        let destination = temp.path().join("aegis.toml");
        fs::write(&source, "model = \"gpt-5\"\n").unwrap();
        fs::write(&destination, "model = [").unwrap();

        let err = apply_codex_config_import(&options(source, destination.clone(), true))
            .expect_err("invalid destination should fail");

        assert_eq!(err.kind(), ErrorKind::InvalidData);
        assert_eq!(fs::read_to_string(destination).unwrap(), "model = [");
    }
}
