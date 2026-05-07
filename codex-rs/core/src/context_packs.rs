use codex_exec_server::ExecutorFileSystem;
use codex_utils_absolute_path::AbsolutePathBuf;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::Path;
use toml::Value as TomlValue;
use toml_edit::Array;
use toml_edit::DocumentMut;
use toml_edit::Item as TomlItem;
use toml_edit::Table as TomlTable;
use toml_edit::value;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextPackDiagnosticStatus {
    Candidate,
    Promoted,
    Retired,
    Invalid,
    Unreadable,
}

impl ContextPackDiagnostic {
    pub fn diagnostic_status(&self) -> ContextPackDiagnosticStatus {
        match self.promotion_status {
            Some(PromotionStatus::Candidate) => ContextPackDiagnosticStatus::Candidate,
            Some(PromotionStatus::Promoted) => ContextPackDiagnosticStatus::Promoted,
            Some(PromotionStatus::Retired) => ContextPackDiagnosticStatus::Retired,
            None if self.reason.starts_with("unreadable:") => {
                ContextPackDiagnosticStatus::Unreadable
            }
            _ => ContextPackDiagnosticStatus::Invalid,
        }
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContextPackInspection {
    pub path: String,
    pub pack_id: String,
    pub kind: ContextPackKind,
    pub schema_version: u64,
    pub name: String,
    pub description: Option<String>,
    pub promotion: PromotionInspection,
    pub rollback: Option<RollbackInspection>,
    pub provenance: Option<ProvenanceInspection>,
    pub evidence_requirements: Vec<EvidenceRequirementInspection>,
    pub guidance: Option<Vec<GuidanceInspection>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PromotionInspection {
    pub status: PromotionStatus,
    pub promoted_at: Option<String>,
    pub promoted_by: Option<String>,
    pub review_required: Option<bool>,
    pub source_evidence: Vec<String>,
    pub retired_at: Option<String>,
    pub retired_by: Option<String>,
    pub retire_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RollbackInspection {
    pub previous_pack_id: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProvenanceInspection {
    pub author: Option<String>,
    pub source: Option<String>,
    pub created_at: Option<String>,
    pub source_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EvidenceRequirementInspection {
    pub id: String,
    pub description: String,
    pub commands: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GuidanceInspection {
    pub id: String,
    pub category: String,
    pub text: String,
    pub falsifiers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContextPackLifecycleResult {
    pub dry_run: bool,
    pub changes: Vec<ContextPackLifecycleChange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContextPackLifecycleChange {
    pub action: ContextPackLifecycleAction,
    pub path: String,
    pub pack_id: String,
    pub from: PromotionStatus,
    pub to: PromotionStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextPackLifecycleAction {
    Promote,
    Retire,
    RollbackRestore,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContextPackLineage {
    pub pack_id: String,
    pub path: String,
    pub status: PromotionStatus,
    pub previous_pack_id: Option<String>,
    pub broken_previous_pack_id: Option<String>,
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

pub fn inspect_context_pack_path(
    path: &AbsolutePathBuf,
    show_guidance: bool,
) -> anyhow::Result<ContextPackInspection> {
    let raw = fs::read_to_string(path)
        .map_err(|err| anyhow::anyhow!("failed to read {}: {err}", path.display()))?;
    let pack = parse_context_pack_toml(path, &raw)?;
    Ok(inspection_from_pack(path, &pack, show_guidance))
}

pub fn promote_context_pack(
    paths: &[AbsolutePathBuf],
    selector: &str,
    actor: &str,
    evidence: &[String],
    reason: Option<&str>,
    now: &str,
    dry_run: bool,
) -> anyhow::Result<ContextPackLifecycleResult> {
    if evidence.is_empty() {
        anyhow::bail!("promotion requires at least one --evidence reference");
    }
    require_nonempty_lifecycle("actor", actor)?;
    require_nonempty_lifecycle("timestamp", now)?;

    let mut records = load_editable_records(paths)?;
    let target_index = resolve_record_index(&records, selector)?;
    let target_pack = &records[target_index].pack;
    if target_pack.kind != ContextPackKind::Learned {
        anyhow::bail!("only learned context packs can be promoted");
    }
    if target_pack.promotion.status != PromotionStatus::Candidate {
        anyhow::bail!(
            "context pack `{}` is {:?}, not candidate",
            target_pack.pack_id,
            target_pack.promotion.status
        );
    }

    let active_indices = records
        .iter()
        .enumerate()
        .filter(|(idx, record)| {
            *idx != target_index
                && record.pack.kind == ContextPackKind::Learned
                && record.pack.promotion.status == PromotionStatus::Promoted
        })
        .map(|(idx, _)| idx)
        .collect::<Vec<_>>();
    if active_indices.len() > 1 {
        anyhow::bail!(
            "multiple promoted learned packs are configured; cannot choose prior active pack"
        );
    }

    let mut changes = Vec::new();
    let previous_pack_id = active_indices
        .first()
        .map(|idx| records[*idx].pack.pack_id.clone())
        .unwrap_or_default();
    if let Some(active_index) = active_indices.first().copied() {
        let previous_reason = format!("Retired by promotion of {}", target_pack.pack_id);
        set_retired(&mut records[active_index].doc, actor, now, &previous_reason);
        changes.push(change(
            ContextPackLifecycleAction::Retire,
            &records[active_index],
            PromotionStatus::Promoted,
            PromotionStatus::Retired,
        ));
    }

    let target_reason = reason.unwrap_or("Promoted candidate context pack.");
    set_promoted(
        &mut records[target_index].doc,
        actor,
        now,
        evidence,
        &previous_pack_id,
        target_reason,
    );
    changes.push(change(
        ContextPackLifecycleAction::Promote,
        &records[target_index],
        PromotionStatus::Candidate,
        PromotionStatus::Promoted,
    ));

    persist_changed_records(&records, &changes, dry_run)?;
    Ok(ContextPackLifecycleResult { dry_run, changes })
}

pub fn retire_context_pack(
    paths: &[AbsolutePathBuf],
    selector: &str,
    actor: &str,
    reason: &str,
    now: &str,
    dry_run: bool,
) -> anyhow::Result<ContextPackLifecycleResult> {
    require_nonempty_lifecycle("actor", actor)?;
    require_nonempty_lifecycle("reason", reason)?;
    require_nonempty_lifecycle("timestamp", now)?;

    let mut records = load_editable_records(paths)?;
    let target_index = resolve_record_index(&records, selector)?;
    let from = records[target_index].pack.promotion.status;
    if from == PromotionStatus::Retired {
        anyhow::bail!(
            "context pack `{}` is already retired",
            records[target_index].pack.pack_id
        );
    }

    set_retired(&mut records[target_index].doc, actor, now, reason);
    let changes = vec![change(
        ContextPackLifecycleAction::Retire,
        &records[target_index],
        from,
        PromotionStatus::Retired,
    )];
    persist_changed_records(&records, &changes, dry_run)?;
    Ok(ContextPackLifecycleResult { dry_run, changes })
}

pub fn rollback_context_pack(
    paths: &[AbsolutePathBuf],
    selector: Option<&str>,
    actor: &str,
    reason: &str,
    now: &str,
    dry_run: bool,
) -> anyhow::Result<ContextPackLifecycleResult> {
    require_nonempty_lifecycle("actor", actor)?;
    require_nonempty_lifecycle("reason", reason)?;
    require_nonempty_lifecycle("timestamp", now)?;

    let mut records = load_editable_records(paths)?;
    let current_index = match selector {
        Some(selector) => resolve_record_index(&records, selector)?,
        None => {
            let active = records
                .iter()
                .enumerate()
                .filter(|(_, record)| {
                    record.pack.kind == ContextPackKind::Learned
                        && record.pack.promotion.status == PromotionStatus::Promoted
                })
                .map(|(idx, _)| idx)
                .collect::<Vec<_>>();
            match active.as_slice() {
                [idx] => *idx,
                [] => anyhow::bail!("no promoted learned context pack is configured"),
                _ => {
                    anyhow::bail!("multiple promoted learned packs are configured; pass a selector")
                }
            }
        }
    };

    if records[current_index].pack.kind != ContextPackKind::Learned {
        anyhow::bail!("rollback target must be a learned context pack");
    }
    if records[current_index].pack.promotion.status != PromotionStatus::Promoted {
        anyhow::bail!("rollback target must be promoted");
    }

    let previous_pack_id = records[current_index]
        .pack
        .rollback
        .as_ref()
        .and_then(|rollback| rollback.previous_pack_id.as_ref())
        .map(|pack_id| pack_id.trim())
        .unwrap_or_default()
        .to_string();
    if previous_pack_id.is_empty() {
        anyhow::bail!(
            "context pack `{}` has no prior active version to restore",
            records[current_index].pack.pack_id
        );
    }

    let previous_index = resolve_record_index(&records, &previous_pack_id)?;
    if previous_index == current_index {
        anyhow::bail!("rollback target points to itself");
    }
    if records[previous_index].pack.kind != ContextPackKind::Learned {
        anyhow::bail!("prior context pack `{previous_pack_id}` is not learned");
    }

    let current_pack_id = records[current_index].pack.pack_id.clone();
    set_retired(&mut records[current_index].doc, actor, now, reason);
    set_rollback_restored(
        &mut records[previous_index].doc,
        actor,
        now,
        &[format!("rollback:{current_pack_id}")],
        reason,
    );

    let changes = vec![
        change(
            ContextPackLifecycleAction::Retire,
            &records[current_index],
            PromotionStatus::Promoted,
            PromotionStatus::Retired,
        ),
        change(
            ContextPackLifecycleAction::RollbackRestore,
            &records[previous_index],
            records[previous_index].pack.promotion.status,
            PromotionStatus::Promoted,
        ),
    ];
    persist_changed_records(&records, &changes, dry_run)?;
    Ok(ContextPackLifecycleResult { dry_run, changes })
}

pub fn context_pack_lineage(
    paths: &[AbsolutePathBuf],
    selector: Option<&str>,
) -> anyhow::Result<Vec<ContextPackLineage>> {
    let records = load_editable_records(paths)?;
    let start_index = match selector {
        Some(selector) => resolve_record_index(&records, selector)?,
        None => {
            let active = records
                .iter()
                .enumerate()
                .filter(|(_, record)| {
                    record.pack.kind == ContextPackKind::Learned
                        && record.pack.promotion.status == PromotionStatus::Promoted
                })
                .map(|(idx, _)| idx)
                .collect::<Vec<_>>();
            match active.as_slice() {
                [idx] => *idx,
                [] => anyhow::bail!("no promoted learned context pack is configured"),
                _ => {
                    anyhow::bail!("multiple promoted learned packs are configured; pass a selector")
                }
            }
        }
    };

    let mut lineage = Vec::new();
    let mut seen = BTreeSet::new();
    let mut current = Some(start_index);
    while let Some(idx) = current {
        let record = &records[idx];
        if !seen.insert(record.pack.pack_id.clone()) {
            anyhow::bail!("lineage cycle detected at `{}`", record.pack.pack_id);
        }
        let previous = record
            .pack
            .rollback
            .as_ref()
            .and_then(|rollback| rollback.previous_pack_id.clone())
            .filter(|pack_id| !pack_id.trim().is_empty());
        let previous_index = previous.as_ref().and_then(|pack_id| {
            records
                .iter()
                .position(|record| record.pack.pack_id == *pack_id)
        });
        lineage.push(ContextPackLineage {
            pack_id: record.pack.pack_id.clone(),
            path: record.path.display().to_string(),
            status: record.pack.promotion.status,
            previous_pack_id: previous.clone(),
            broken_previous_pack_id: previous
                .as_ref()
                .filter(|_| previous_index.is_none())
                .cloned(),
        });
        current = previous_index;
    }

    Ok(lineage)
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

struct EditableContextPack {
    path: AbsolutePathBuf,
    doc: DocumentMut,
    pack: ContextPackToml,
}

fn load_editable_records(paths: &[AbsolutePathBuf]) -> anyhow::Result<Vec<EditableContextPack>> {
    let mut records = Vec::new();
    for path in paths {
        let raw = fs::read_to_string(path)
            .map_err(|err| anyhow::anyhow!("failed to read {}: {err}", path.display()))?;
        let doc = raw
            .parse::<DocumentMut>()
            .map_err(|err| anyhow::anyhow!("failed to parse {} as TOML: {err}", path.display()))?;
        let pack = parse_context_pack_toml(path, &raw)?;
        records.push(EditableContextPack {
            path: path.clone(),
            doc,
            pack,
        });
    }
    Ok(records)
}

fn parse_context_pack_toml(path: &AbsolutePathBuf, raw: &str) -> anyhow::Result<ContextPackToml> {
    let value = toml::from_str::<TomlValue>(raw)
        .map_err(|err| anyhow::anyhow!("failed to parse {} as TOML: {err}", path.display()))?;
    if let Some(secret_key_path) = first_secret_like_key(&value) {
        anyhow::bail!(
            "context pack {} contains secret-like key `{secret_key_path}`",
            path.display()
        );
    }
    let pack = toml::from_str::<ContextPackToml>(raw)
        .map_err(|err| anyhow::anyhow!("invalid context pack {}: {err}", path.display()))?;
    let mut errors = validate_pack(&pack);
    errors.sort();
    errors.dedup();
    if !errors.is_empty() {
        anyhow::bail!(
            "invalid context pack {}: {}",
            path.display(),
            errors.join("; ")
        );
    }
    Ok(pack)
}

fn inspection_from_pack(
    path: &AbsolutePathBuf,
    pack: &ContextPackToml,
    show_guidance: bool,
) -> ContextPackInspection {
    ContextPackInspection {
        path: path.display().to_string(),
        pack_id: pack.pack_id.clone(),
        kind: pack.kind,
        schema_version: pack.schema_version,
        name: pack.name.clone(),
        description: pack.description.clone(),
        promotion: PromotionInspection {
            status: pack.promotion.status,
            promoted_at: pack.promotion.promoted_at.clone(),
            promoted_by: pack.promotion.promoted_by.clone(),
            review_required: pack.promotion.review_required,
            source_evidence: pack.promotion.source_evidence.clone().unwrap_or_default(),
            retired_at: pack.promotion.retired_at.clone(),
            retired_by: pack.promotion.retired_by.clone(),
            retire_reason: pack.promotion.retire_reason.clone(),
        },
        rollback: pack.rollback.as_ref().map(|rollback| RollbackInspection {
            previous_pack_id: rollback.previous_pack_id.clone(),
            reason: rollback.reason.clone(),
        }),
        provenance: pack
            .provenance
            .as_ref()
            .map(|provenance| ProvenanceInspection {
                author: provenance.author.clone(),
                source: provenance.source.clone(),
                created_at: provenance.created_at.clone(),
                source_refs: provenance.source_refs.clone().unwrap_or_default(),
            }),
        evidence_requirements: pack
            .evidence
            .as_ref()
            .and_then(|evidence| evidence.requirements.clone())
            .unwrap_or_default()
            .into_iter()
            .map(|requirement| EvidenceRequirementInspection {
                id: requirement.id,
                description: requirement.description,
                commands: requirement.commands.unwrap_or_default(),
            })
            .collect(),
        guidance: show_guidance.then(|| {
            pack.guidance
                .iter()
                .map(|guidance| GuidanceInspection {
                    id: guidance.id.clone(),
                    category: guidance.category.clone(),
                    text: guidance.text.clone(),
                    falsifiers: guidance.falsifiers.clone().unwrap_or_default(),
                })
                .collect()
        }),
    }
}

fn resolve_record_index(records: &[EditableContextPack], selector: &str) -> anyhow::Result<usize> {
    let trimmed = selector.trim();
    if trimmed.is_empty() {
        anyhow::bail!("context pack selector must not be empty");
    }

    let matches = records
        .iter()
        .enumerate()
        .filter(|(_, record)| {
            record.pack.pack_id == trimmed || record.path.as_path().to_string_lossy() == trimmed
        })
        .map(|(idx, _)| idx)
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [idx] => Ok(*idx),
        [] => anyhow::bail!("no configured context pack matches `{trimmed}`"),
        _ => anyhow::bail!("multiple configured context packs match `{trimmed}`"),
    }
}

fn change(
    action: ContextPackLifecycleAction,
    record: &EditableContextPack,
    from: PromotionStatus,
    to: PromotionStatus,
) -> ContextPackLifecycleChange {
    ContextPackLifecycleChange {
        action,
        path: record.path.display().to_string(),
        pack_id: record.pack.pack_id.clone(),
        from,
        to,
    }
}

fn persist_changed_records(
    records: &[EditableContextPack],
    changes: &[ContextPackLifecycleChange],
    dry_run: bool,
) -> anyhow::Result<()> {
    if dry_run {
        return Ok(());
    }

    for change in changes {
        let Some(record) = records.iter().find(|record| {
            record.pack.pack_id == change.pack_id
                && record.path.display().to_string() == change.path
        }) else {
            anyhow::bail!(
                "internal error: changed pack `{}` was not loaded",
                change.pack_id
            );
        };
        fs::write(&record.path, record.doc.to_string())
            .map_err(|err| anyhow::anyhow!("failed to write {}: {err}", record.path.display()))?;
    }
    Ok(())
}

fn set_promoted(
    doc: &mut DocumentMut,
    actor: &str,
    now: &str,
    evidence: &[String],
    previous_pack_id: &str,
    reason: &str,
) {
    let promotion = ensure_table(doc, "promotion");
    promotion["status"] = value("promoted");
    promotion["promoted_at"] = value(now.to_string());
    promotion["promoted_by"] = value(actor.to_string());
    promotion["source_evidence"] = string_array(evidence.iter().cloned());
    promotion.remove("retired_at");
    promotion.remove("retired_by");
    promotion.remove("retire_reason");

    let rollback = ensure_table(doc, "rollback");
    rollback["previous_pack_id"] = value(previous_pack_id.to_string());
    rollback["reason"] = value(reason.to_string());
}

fn set_retired(doc: &mut DocumentMut, actor: &str, now: &str, reason: &str) {
    let promotion = ensure_table(doc, "promotion");
    promotion["status"] = value("retired");
    promotion["retired_at"] = value(now.to_string());
    promotion["retired_by"] = value(actor.to_string());
    promotion["retire_reason"] = value(reason.to_string());
}

fn set_rollback_restored(
    doc: &mut DocumentMut,
    actor: &str,
    now: &str,
    evidence: &[String],
    reason: &str,
) {
    let promotion = ensure_table(doc, "promotion");
    promotion["status"] = value("promoted");
    promotion["promoted_at"] = value(now.to_string());
    promotion["promoted_by"] = value(actor.to_string());
    promotion["source_evidence"] = string_array(evidence.iter().cloned());
    promotion.remove("retired_at");
    promotion.remove("retired_by");
    promotion.remove("retire_reason");

    let rollback = ensure_table(doc, "rollback");
    if !rollback.contains_key("previous_pack_id") {
        rollback["previous_pack_id"] = value("");
    }
    rollback["reason"] = value(reason.to_string());
}

fn ensure_table<'a>(doc: &'a mut DocumentMut, key: &str) -> &'a mut TomlTable {
    if !doc.as_table().contains_key(key) || !doc[key].is_table() {
        doc[key] = TomlItem::Table(TomlTable::new());
    }
    doc[key].as_table_mut().expect("table inserted")
}

fn string_array(values: impl IntoIterator<Item = String>) -> TomlItem {
    let mut array = Array::new();
    for value in values {
        array.push(value);
    }
    TomlItem::Value(array.into())
}

fn require_nonempty_lifecycle(field: &str, value: &str) -> anyhow::Result<()> {
    if value.trim().is_empty() {
        anyhow::bail!("{field} must not be empty");
    }
    Ok(())
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
                    if rollback.previous_pack_id.is_none() {
                        errors.push("rollback.previous_pack_id is required".to_string());
                    }
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
    source_evidence: Option<Vec<String>>,
    retired_at: Option<String>,
    retired_by: Option<String>,
    retire_reason: Option<String>,
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
