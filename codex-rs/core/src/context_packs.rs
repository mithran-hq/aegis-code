use codex_exec_server::ExecutorFileSystem;
use codex_utils_absolute_path::AbsolutePathBuf;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeSet;
use std::io;
use std::path::Path;
use toml::Value as TomlValue;

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct ContextPackSet {
    diagnostics: Vec<ContextPackDiagnostic>,
    #[serde(skip)]
    active_packs: Vec<LoadedContextPack>,
}

impl ContextPackSet {
    pub fn diagnostics(&self) -> &[ContextPackDiagnostic] {
        &self.diagnostics
    }

    pub(crate) fn guidance_layer(&self, kind: ContextPackKind) -> Option<ContextPackGuidanceLayer> {
        let mut parts = Vec::new();
        let mut sources = Vec::new();
        let mut pack_ids = Vec::new();

        for pack in self.active_packs.iter().filter(|pack| pack.kind == kind) {
            let guidance = pack.guidance_text();
            if guidance.trim().is_empty() {
                continue;
            }

            pack_ids.push(pack.pack_id.clone());
            sources.push(pack.path.clone());
            parts.push(format!(
                "--- context-pack:{} ---\n\n{}",
                pack.pack_id, guidance
            ));
        }

        (!parts.is_empty()).then_some(ContextPackGuidanceLayer {
            contents: parts.join("\n\n"),
            sources,
            pack_ids,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContextPackKind {
    User,
    Project,
    Learned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PromotionStatus {
    Candidate,
    Promoted,
    Retired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContextPackDiagnostic {
    pub path: String,
    pub pack_id: Option<String>,
    pub kind: Option<ContextPackKind>,
    pub schema_version: Option<u64>,
    pub promotion_status: Option<PromotionStatus>,
    pub active: bool,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ContextPackGuidanceLayer {
    pub(crate) contents: String,
    pub(crate) sources: Vec<AbsolutePathBuf>,
    pub(crate) pack_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LoadedContextPack {
    path: AbsolutePathBuf,
    pack_id: String,
    kind: ContextPackKind,
    guidance: Vec<GuidanceToml>,
}

impl LoadedContextPack {
    fn guidance_text(&self) -> String {
        self.guidance
            .iter()
            .map(|entry| entry.text.trim())
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

pub async fn load_context_packs(
    fs: &dyn ExecutorFileSystem,
    paths: &[AbsolutePathBuf],
) -> ContextPackSet {
    let mut diagnostics = Vec::new();
    let mut active_packs = Vec::new();

    for path in paths {
        match load_one_context_pack(fs, path).await {
            Ok(LoadedPackOutcome::Active { diagnostic, pack }) => {
                diagnostics.push(diagnostic);
                active_packs.push(pack);
            }
            Ok(LoadedPackOutcome::Inactive { diagnostic }) => diagnostics.push(diagnostic),
            Err(err) => diagnostics.push(ContextPackDiagnostic {
                path: path.display().to_string(),
                pack_id: None,
                kind: None,
                schema_version: None,
                promotion_status: None,
                active: false,
                reason: format!("unreadable: {err}"),
            }),
        }
    }

    ContextPackSet {
        diagnostics,
        active_packs,
    }
}

enum LoadedPackOutcome {
    Active {
        diagnostic: ContextPackDiagnostic,
        pack: LoadedContextPack,
    },
    Inactive {
        diagnostic: ContextPackDiagnostic,
    },
}

async fn load_one_context_pack(
    fs: &dyn ExecutorFileSystem,
    path: &AbsolutePathBuf,
) -> io::Result<LoadedPackOutcome> {
    let contents = fs.read_file_text(path, /*sandbox*/ None).await?;
    let parsed_value = match toml::from_str::<TomlValue>(&contents) {
        Ok(value) => value,
        Err(err) => {
            return Ok(LoadedPackOutcome::Inactive {
                diagnostic: invalid_diagnostic(path, None, None, None, None, err.to_string()),
            });
        }
    };

    if let Some(secret_key_path) = first_secret_like_key(&parsed_value) {
        return Ok(LoadedPackOutcome::Inactive {
            diagnostic: invalid_diagnostic(
                path,
                None,
                None,
                None,
                None,
                format!("secret-like key `{secret_key_path}` is not allowed"),
            ),
        });
    }

    let pack = match toml::from_str::<ContextPackToml>(&contents) {
        Ok(pack) => pack,
        Err(err) => {
            return Ok(LoadedPackOutcome::Inactive {
                diagnostic: invalid_diagnostic(path, None, None, None, None, err.to_string()),
            });
        }
    };

    let mut errors = validate_pack(&pack);
    let pack_id = Some(pack.pack_id.clone());
    let kind = Some(pack.kind);
    let schema_version = Some(pack.schema_version);
    let promotion_status = Some(pack.promotion.status);

    if !errors.is_empty() {
        errors.sort();
        errors.dedup();
        return Ok(LoadedPackOutcome::Inactive {
            diagnostic: invalid_diagnostic(
                path,
                pack_id,
                kind,
                schema_version,
                promotion_status,
                errors.join("; "),
            ),
        });
    }

    match pack.promotion.status {
        PromotionStatus::Promoted => {
            let loaded = LoadedContextPack {
                path: path.clone(),
                pack_id: pack.pack_id.clone(),
                kind: pack.kind,
                guidance: pack.guidance.clone(),
            };
            Ok(LoadedPackOutcome::Active {
                diagnostic: ContextPackDiagnostic {
                    path: path.display().to_string(),
                    pack_id,
                    kind,
                    schema_version,
                    promotion_status,
                    active: true,
                    reason: "active".to_string(),
                },
                pack: loaded,
            })
        }
        PromotionStatus::Candidate => Ok(LoadedPackOutcome::Inactive {
            diagnostic: ContextPackDiagnostic {
                path: path.display().to_string(),
                pack_id,
                kind,
                schema_version,
                promotion_status,
                active: false,
                reason: "promotion_status_candidate".to_string(),
            },
        }),
        PromotionStatus::Retired => Ok(LoadedPackOutcome::Inactive {
            diagnostic: ContextPackDiagnostic {
                path: path.display().to_string(),
                pack_id,
                kind,
                schema_version,
                promotion_status,
                active: false,
                reason: "promotion_status_retired".to_string(),
            },
        }),
    }
}

fn invalid_diagnostic(
    path: &AbsolutePathBuf,
    pack_id: Option<String>,
    kind: Option<ContextPackKind>,
    schema_version: Option<u64>,
    promotion_status: Option<PromotionStatus>,
    detail: String,
) -> ContextPackDiagnostic {
    ContextPackDiagnostic {
        path: path.display().to_string(),
        pack_id,
        kind,
        schema_version,
        promotion_status,
        active: false,
        reason: format!("invalid: {detail}"),
    }
}

fn validate_pack(pack: &ContextPackToml) -> Vec<String> {
    let mut errors = Vec::new();

    if pack.schema_version != 1 {
        errors.push(format!(
            "unsupported schema_version {}",
            pack.schema_version
        ));
    }
    if pack.compatibility.schema.trim() != "1" {
        errors.push(format!(
            "unsupported compatibility.schema `{}`",
            pack.compatibility.schema
        ));
    }
    if let Some(aegis_code) = &pack.compatibility.aegis_code {
        require_nonempty("compatibility.aegis_code", aegis_code, &mut errors);
    }
    require_nonempty("pack_id", &pack.pack_id, &mut errors);
    require_nonempty("name", &pack.name, &mut errors);
    if let Some(description) = &pack.description {
        require_nonempty("description", description, &mut errors);
    }

    if pack.guidance.is_empty() {
        errors.push("guidance must contain at least one entry".to_string());
    }
    for guidance in &pack.guidance {
        require_nonempty("guidance.id", &guidance.id, &mut errors);
        require_nonempty("guidance.category", &guidance.category, &mut errors);
        require_nonempty("guidance.text", &guidance.text, &mut errors);

        if pack.kind == ContextPackKind::Learned {
            match &guidance.falsifiers {
                Some(falsifiers) if falsifiers.iter().any(|item| !item.trim().is_empty()) => {}
                _ => errors.push(format!(
                    "learned guidance `{}` must include at least one falsifier",
                    guidance.id
                )),
            }
        }
    }

    if pack.kind == ContextPackKind::Learned {
        match &pack.provenance {
            Some(provenance) => {
                require_optional_nonempty("provenance.source", &provenance.source, &mut errors);
                require_optional_nonempty(
                    "provenance.created_at",
                    &provenance.created_at,
                    &mut errors,
                );
            }
            None => errors.push("learned packs require provenance".to_string()),
        }

        if pack.promotion.status == PromotionStatus::Promoted {
            match &pack.rollback {
                Some(rollback) => {
                    require_optional_nonempty(
                        "rollback.previous_pack_id",
                        &rollback.previous_pack_id,
                        &mut errors,
                    );
                    require_optional_nonempty("rollback.reason", &rollback.reason, &mut errors);
                }
                None => errors.push("promoted learned packs require rollback metadata".to_string()),
            }
        }
    }

    errors
}

fn require_nonempty(field: &str, value: &str, errors: &mut Vec<String>) {
    if value.trim().is_empty() {
        errors.push(format!("{field} must not be empty"));
    }
}

fn require_optional_nonempty(field: &str, value: &Option<String>, errors: &mut Vec<String>) {
    match value {
        Some(value) => require_nonempty(field, value, errors),
        None => errors.push(format!("{field} is required")),
    }
}

fn first_secret_like_key(value: &TomlValue) -> Option<String> {
    fn visit(value: &TomlValue, prefix: &str, seen: &mut BTreeSet<String>) -> Option<String> {
        match value {
            TomlValue::Table(table) => {
                for (key, value) in table {
                    let path = if prefix.is_empty() {
                        key.to_string()
                    } else {
                        format!("{prefix}.{key}")
                    };
                    if is_secret_like_key(key) {
                        return Some(path);
                    }
                    if seen.insert(path.clone())
                        && let Some(found) = visit(value, &path, seen)
                    {
                        return Some(found);
                    }
                }
                None
            }
            TomlValue::Array(items) => {
                for (idx, item) in items.iter().enumerate() {
                    let path = format!("{prefix}[{idx}]");
                    if let Some(found) = visit(item, &path, seen) {
                        return Some(found);
                    }
                }
                None
            }
            _ => None,
        }
    }

    visit(value, "", &mut BTreeSet::new())
}

fn is_secret_like_key(key: &str) -> bool {
    let normalized = key.to_ascii_lowercase().replace('-', "_");
    matches!(
        normalized.as_str(),
        "api_key"
            | "apikey"
            | "password"
            | "token"
            | "access_token"
            | "refresh_token"
            | "private_key"
            | "secret_key"
            | "client_secret"
    )
}

pub(crate) fn redact_context_pack_sources(
    sources: &[AbsolutePathBuf],
    codex_home: &AbsolutePathBuf,
    cwd: &AbsolutePathBuf,
) -> String {
    sources
        .iter()
        .map(|source| redact_context_pack_path(source, codex_home, cwd))
        .collect::<Vec<_>>()
        .join(",")
}

fn redact_context_pack_path(
    path: &AbsolutePathBuf,
    codex_home: &AbsolutePathBuf,
    cwd: &AbsolutePathBuf,
) -> String {
    let path = path.as_path();
    if let Some(rest) = path
        .strip_prefix(cwd.as_path())
        .ok()
        .filter(|rest| !rest.as_os_str().is_empty())
    {
        return format!("$CWD/{}", display_path(rest));
    }
    if path == cwd.as_path() {
        return "$CWD".to_string();
    }
    if let Some(rest) = path
        .strip_prefix(codex_home.as_path())
        .ok()
        .filter(|rest| !rest.as_os_str().is_empty())
    {
        return format!("$CODEX_HOME/{}", display_path(rest));
    }
    if path == codex_home.as_path() {
        return "$CODEX_HOME".to_string();
    }
    "context_pack_outside_roots".to_string()
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct ContextPackToml {
    schema_version: u64,
    pack_id: String,
    kind: ContextPackKind,
    name: String,
    description: Option<String>,
    compatibility: CompatibilityToml,
    scope: Option<ScopeToml>,
    guidance: Vec<GuidanceToml>,
    evidence: Option<EvidenceToml>,
    tool_policy: Option<ToolPolicyToml>,
    reviewer_checks: Option<ReviewerChecksToml>,
    provider_defaults: Option<ProviderDefaultsToml>,
    provenance: Option<ProvenanceToml>,
    promotion: PromotionToml,
    rollback: Option<RollbackToml>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct CompatibilityToml {
    aegis_code: Option<String>,
    schema: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct ScopeToml {
    repositories: Option<Vec<String>>,
    paths: Option<Vec<String>>,
    users: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct GuidanceToml {
    id: String,
    category: String,
    text: String,
    falsifiers: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct EvidenceToml {
    requirements: Option<Vec<EvidenceRequirementToml>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct EvidenceRequirementToml {
    id: String,
    description: String,
    commands: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct ToolPolicyToml {
    sensitive_commands: Option<Vec<String>>,
    secret_broker: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct ReviewerChecksToml {
    required: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct ProviderDefaultsToml {
    preferred: Option<String>,
    fallbacks: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct ProvenanceToml {
    author: Option<String>,
    source: Option<String>,
    created_at: Option<String>,
    source_refs: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct PromotionToml {
    status: PromotionStatus,
    promoted_at: Option<String>,
    promoted_by: Option<String>,
    review_required: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct RollbackToml {
    previous_pack_id: Option<String>,
    reason: Option<String>,
}

#[cfg(test)]
mod tests;
