use codex_protocol::aegis_engine_alert::AegisEngineAlertSeverity;
use codex_protocol::aegis_engine_alert::AegisEngineAlertSourceEvent;
use codex_protocol::aegis_engine_alert::AegisEngineCandidateGuidance;
use codex_protocol::aegis_safety_event::AegisSafetyEvent;
use codex_protocol::aegis_safety_event::AegisSafetyEventCategory;
use codex_protocol::aegis_safety_event::AegisSafetySeverityHint;
use serde::Deserialize;
use serde::Serialize;
use sha1::Digest;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use toml_edit::Array;
use toml_edit::ArrayOfTables;
use toml_edit::DocumentMut;
use toml_edit::Item as TomlItem;
use toml_edit::Table;
use toml_edit::value;

const DEFAULT_REPOSITORY: &str = "aegis-code";
const MAX_SUMMARY_CHARS: usize = 160;
const PACK_ID_HASH_CHARS: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LearnedPackCompilerOptions {
    pub events_path: PathBuf,
    pub alert_inputs_path: PathBuf,
    pub output_dir: PathBuf,
    pub repository: String,
    pub min_support: usize,
    pub now: String,
    pub dry_run: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LearnedPackCompileResult {
    pub dry_run: bool,
    pub output_dir: String,
    pub min_support: usize,
    pub candidates: Vec<LearnedPackCandidate>,
    pub skipped_groups: Vec<LearnedPackSkippedGroup>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LearnedPackCandidate {
    pub pack_id: String,
    pub path: String,
    pub group_kind: LearnedPackGroupKind,
    pub support_count: usize,
    pub evidence_refs: Vec<String>,
    pub guidance: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LearnedPackSkippedGroup {
    pub group_key: String,
    pub group_kind: LearnedPackGroupKind,
    pub support_count: usize,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LearnedPackGroupKind {
    ReviewFinding,
    ToolDenial,
    StaleResume,
    AlertCandidate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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

#[derive(Debug, Clone)]
struct EvidenceItem {
    reference: String,
}

#[derive(Debug, Clone)]
struct CandidateGroup {
    kind: LearnedPackGroupKind,
    key: String,
    summary: String,
    guidance: String,
    category: String,
    rationale: String,
    expected_impact: String,
    falsifiers: Vec<String>,
    evidence: Vec<EvidenceItem>,
}

pub fn compile_learned_pack_candidates(
    options: &LearnedPackCompilerOptions,
) -> anyhow::Result<LearnedPackCompileResult> {
    if options.min_support == 0 {
        anyhow::bail!("--min-support must be at least 1");
    }
    if options.now.trim().is_empty() {
        anyhow::bail!("compiler timestamp must not be empty");
    }

    let mut diagnostics = Vec::new();
    let mut groups = BTreeMap::<String, CandidateGroup>::new();
    read_event_groups(options, &mut groups, &mut diagnostics)?;
    read_alert_candidate_groups(options, &mut groups, &mut diagnostics)?;

    let mut candidates = Vec::new();
    let mut skipped_groups = Vec::new();
    let repository = clean_text(&options.repository)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_REPOSITORY.to_string());
    let repository_slug = slugify(&repository);

    for mut group in groups.into_values() {
        dedupe_evidence(&mut group.evidence);
        let support_count = group.evidence.len();
        if support_count < options.min_support {
            skipped_groups.push(LearnedPackSkippedGroup {
                group_key: group.key,
                group_kind: group.kind,
                support_count,
                reason: format!("support below min_support {}", options.min_support),
            });
            continue;
        }

        let base_slug = slugify(&group.summary);
        let hash = short_hash(&group.key, PACK_ID_HASH_CHARS);
        let pack_slug = format!("{base_slug}-{hash}");
        let pack_id = format!("learned:{repository_slug}:{pack_slug}");
        let path = options.output_dir.join(format!("{pack_slug}.toml"));
        let evidence_refs = group
            .evidence
            .iter()
            .map(|item| item.reference.clone())
            .collect::<Vec<_>>();
        let doc = candidate_toml(&repository, &pack_id, &group, &evidence_refs, &options.now);

        if !options.dry_run {
            fs::create_dir_all(&options.output_dir).map_err(|err| {
                anyhow::anyhow!(
                    "failed to create learned candidate directory {}: {err}",
                    options.output_dir.display()
                )
            })?;
            fs::write(&path, doc.to_string()).map_err(|err| {
                anyhow::anyhow!(
                    "failed to write learned candidate {}: {err}",
                    path.display()
                )
            })?;
        }

        candidates.push(LearnedPackCandidate {
            pack_id,
            path: path.display().to_string(),
            group_kind: group.kind,
            support_count,
            evidence_refs,
            guidance: group.guidance,
        });
    }

    Ok(LearnedPackCompileResult {
        dry_run: options.dry_run,
        output_dir: options.output_dir.display().to_string(),
        min_support: options.min_support,
        candidates,
        skipped_groups,
        diagnostics,
    })
}

fn read_event_groups(
    options: &LearnedPackCompilerOptions,
    groups: &mut BTreeMap<String, CandidateGroup>,
    diagnostics: &mut Vec<String>,
) -> anyhow::Result<()> {
    let contents = read_optional_jsonl(&options.events_path, "Aegis Engine events", diagnostics)?;
    for (line_index, line) in contents.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let event = match serde_json::from_str::<AegisSafetyEvent>(trimmed) {
            Ok(event) => event,
            Err(err) => {
                diagnostics.push(format!(
                    "skipped malformed event line {} in {}: {err}",
                    line_index + 1,
                    options.events_path.display()
                ));
                continue;
            }
        };
        if let Some(group) = group_from_event(&event, line_index + 1) {
            upsert_group(groups, group);
        }
    }
    Ok(())
}

fn read_alert_candidate_groups(
    options: &LearnedPackCompilerOptions,
    groups: &mut BTreeMap<String, CandidateGroup>,
    diagnostics: &mut Vec<String>,
) -> anyhow::Result<()> {
    let contents = read_optional_jsonl(
        &options.alert_inputs_path,
        "Aegis Engine candidate-pack inputs",
        diagnostics,
    )?;
    for (line_index, line) in contents.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let input = match serde_json::from_str::<AegisEngineCandidatePackInput>(trimmed) {
            Ok(input) => input,
            Err(err) => {
                diagnostics.push(format!(
                    "skipped malformed alert candidate line {} in {}: {err}",
                    line_index + 1,
                    options.alert_inputs_path.display()
                ));
                continue;
            }
        };
        if input.schema_version != 1 {
            diagnostics.push(format!(
                "skipped alert candidate `{}` with unsupported schema version {}",
                input.input_id, input.schema_version
            ));
            continue;
        }
        upsert_group(groups, group_from_alert_input(&input, line_index + 1));
    }
    Ok(())
}

fn read_optional_jsonl(
    path: &Path,
    label: &str,
    diagnostics: &mut Vec<String>,
) -> anyhow::Result<String> {
    match fs::read_to_string(path) {
        Ok(contents) => Ok(contents),
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            diagnostics.push(format!(
                "{label} not found at {}; treating as empty",
                path.display()
            ));
            Ok(String::new())
        }
        Err(err) => Err(anyhow::anyhow!("failed to read {}: {err}", path.display())),
    }
}

fn group_from_event(event: &AegisSafetyEvent, line_number: usize) -> Option<CandidateGroup> {
    match event.category {
        AegisSafetyEventCategory::Review => Some(review_group(event, line_number)),
        AegisSafetyEventCategory::ToolDenial => Some(tool_denial_group(event, line_number)),
        AegisSafetyEventCategory::Resume
            if context_string(event, "status").as_deref() == Some("stale") =>
        {
            Some(stale_resume_group(event, line_number))
        }
        _ => None,
    }
}

fn review_group(event: &AegisSafetyEvent, line_number: usize) -> CandidateGroup {
    let summary = summary_or_default(event, "Repeated review finding");
    let severity =
        context_string(event, "severity").unwrap_or_else(|| severity_label(event.severity_hint));
    let status = context_string(event, "status").unwrap_or_else(|| "unknown".to_string());
    let key = format!(
        "review:{}:{}:{}",
        normalize_key(&summary),
        normalize_key(&severity),
        normalize_key(&status)
    );
    CandidateGroup {
        kind: LearnedPackGroupKind::ReviewFinding,
        key,
        summary: format!(
            "Recurring review finding: {}",
            truncate(&summary, MAX_SUMMARY_CHARS)
        ),
        guidance: format!(
            "Before closing similar work, explicitly check and resolve this recurring review finding: {}",
            truncate(&summary, MAX_SUMMARY_CHARS)
        ),
        category: "method".to_string(),
        rationale: "Aegis observed the same review finding more than once.".to_string(),
        expected_impact: "Future sessions should catch this class of issue before completion."
            .to_string(),
        falsifiers: default_falsifiers(),
        evidence: vec![event_evidence(event, line_number)],
    }
}

fn tool_denial_group(event: &AegisSafetyEvent, line_number: usize) -> CandidateGroup {
    let tool = context_string(event, "tool_name").unwrap_or_else(|| "unknown-tool".to_string());
    let risk =
        context_string(event, "risk_category").unwrap_or_else(|| "unspecified-risk".to_string());
    let reason = context_string(event, "reason").unwrap_or_else(|| event.summary.clone());
    let key = format!(
        "tool-denial:{}:{}:{}",
        normalize_key(&tool),
        normalize_key(&risk),
        normalize_key(&reason)
    );
    CandidateGroup {
        kind: LearnedPackGroupKind::ToolDenial,
        key,
        summary: format!("Recurring tool denial for {tool}"),
        guidance: format!(
            "Before attempting `{tool}` for `{risk}`, gather task-scope evidence that the action is required and prefer the least-privileged path."
        ),
        category: "tooling".to_string(),
        rationale: format!("Aegis repeatedly denied `{tool}` for `{risk}`."),
        expected_impact: "Future sessions should avoid repeated denied tool attempts.".to_string(),
        falsifiers: default_falsifiers(),
        evidence: vec![event_evidence(event, line_number)],
    }
}

fn stale_resume_group(event: &AegisSafetyEvent, line_number: usize) -> CandidateGroup {
    let reasons = context_string_array(event, "reasons");
    let reason_text = if reasons.is_empty() {
        "unknown stale-resume reason".to_string()
    } else {
        reasons.join(", ")
    };
    let key = format!("stale-resume:{}", normalize_key(&reason_text));
    CandidateGroup {
        kind: LearnedPackGroupKind::StaleResume,
        key,
        summary: format!("Recurring stale resume: {reason_text}"),
        guidance: format!(
            "When resuming with stale method-state reasons `{reason_text}`, revalidate repository, branch, issue, and method-state assumptions before continuing."
        ),
        category: "resume".to_string(),
        rationale: "Aegis observed repeated stale resume state.".to_string(),
        expected_impact: "Future sessions should re-ground before acting on stale state."
            .to_string(),
        falsifiers: default_falsifiers(),
        evidence: vec![event_evidence(event, line_number)],
    }
}

fn group_from_alert_input(
    input: &AegisEngineCandidatePackInput,
    line_number: usize,
) -> CandidateGroup {
    let guidance = clean_text(&input.guidance.guidance).unwrap_or_else(|| {
        format!(
            "Review repeated Aegis Engine alert before continuing: {}",
            truncate(&input.summary, MAX_SUMMARY_CHARS)
        )
    });
    let key = format!(
        "alert:{}:{}",
        severity_tag(input.severity),
        normalize_key(&guidance)
    );
    let mut falsifiers = input.guidance.falsifiers.clone();
    if falsifiers.iter().all(|item| item.trim().is_empty()) {
        falsifiers = default_falsifiers();
    }
    CandidateGroup {
        kind: LearnedPackGroupKind::AlertCandidate,
        key,
        summary: format!(
            "Recurring Aegis Engine alert: {}",
            truncate(&input.summary, MAX_SUMMARY_CHARS)
        ),
        guidance,
        category: "aegis-engine-alert".to_string(),
        rationale: format!(
            "Aegis Engine repeatedly reported {} alerts with candidate guidance.",
            severity_tag(input.severity)
        ),
        expected_impact:
            "Future sessions should reduce repeated alert-worthy behavior after review.".to_string(),
        falsifiers,
        evidence: vec![alert_evidence(input, line_number)],
    }
}

fn upsert_group(groups: &mut BTreeMap<String, CandidateGroup>, group: CandidateGroup) {
    let key = group.key.clone();
    match groups.get_mut(&key) {
        Some(existing) => existing.evidence.extend(group.evidence),
        None => {
            groups.insert(key, group);
        }
    }
}

fn dedupe_evidence(evidence: &mut Vec<EvidenceItem>) {
    let mut seen = BTreeSet::new();
    evidence.retain(|item| seen.insert(item.reference.clone()));
}

fn event_evidence(event: &AegisSafetyEvent, line_number: usize) -> EvidenceItem {
    let reference = event
        .event_id
        .as_ref()
        .map(|event_id| format!("event:{event_id}"))
        .unwrap_or_else(|| format!("event-line:{line_number}"));
    EvidenceItem { reference }
}

fn alert_evidence(input: &AegisEngineCandidatePackInput, line_number: usize) -> EvidenceItem {
    let source = input
        .source_event
        .event_id
        .as_ref()
        .map(|event_id| format!("; source event:{event_id}"))
        .unwrap_or_default();
    EvidenceItem {
        reference: format!(
            "alert:{}; candidate-input:{}{source}",
            input.alert_id, input.input_id
        ),
    }
    .with_line_fallback(line_number)
}

impl EvidenceItem {
    fn with_line_fallback(mut self, line_number: usize) -> Self {
        if self.reference.trim().is_empty() {
            self.reference = format!("alert-candidate-line:{line_number}");
        }
        self
    }
}

fn candidate_toml(
    repository: &str,
    pack_id: &str,
    group: &CandidateGroup,
    evidence_refs: &[String],
    now: &str,
) -> DocumentMut {
    let mut doc = DocumentMut::new();
    doc["schema_version"] = value(1);
    doc["pack_id"] = value(pack_id);
    doc["kind"] = value("learned");
    doc["name"] = value(group.summary.clone());
    doc["description"] = value(format!(
        "Rationale: {} Expected impact: {}",
        group.rationale, group.expected_impact
    ));

    let mut compatibility = Table::new();
    compatibility["schema"] = value("1");
    doc["compatibility"] = TomlItem::Table(compatibility);

    let mut scope = Table::new();
    scope["repositories"] = string_array([repository.to_string()]);
    scope["paths"] = string_array([".".to_string()]);
    doc["scope"] = TomlItem::Table(scope);

    let mut guidance = ArrayOfTables::new();
    let mut guidance_entry = Table::new();
    guidance_entry["id"] = value(format!("guidance:{}", slugify(&group.summary)));
    guidance_entry["category"] = value(group.category.clone());
    guidance_entry["text"] = value(group.guidance.clone());
    guidance_entry["falsifiers"] = string_array(group.falsifiers.iter().cloned());
    guidance.push(guidance_entry);
    doc["guidance"] = TomlItem::ArrayOfTables(guidance);

    let mut requirements = ArrayOfTables::new();
    let mut requirement = Table::new();
    requirement["id"] = value(format!("evidence:{}", slugify(&group.summary)));
    requirement["description"] = value(format!(
        "Review supporting refs before promotion: {}",
        evidence_refs.join(", ")
    ));
    requirements.push(requirement);
    let mut evidence = Table::new();
    evidence["requirements"] = TomlItem::ArrayOfTables(requirements);
    doc["evidence"] = TomlItem::Table(evidence);

    let mut reviewer_checks = Table::new();
    reviewer_checks["required"] = string_array([
        "Confirm cited events/alerts describe the same behavioral pattern.".to_string(),
        "Confirm falsifiers do not apply before promotion.".to_string(),
    ]);
    doc["reviewer_checks"] = TomlItem::Table(reviewer_checks);

    let mut provenance = Table::new();
    provenance["author"] = value("aegis-engine");
    provenance["source"] = value("aegis-engine-learning");
    provenance["created_at"] = value(now.to_string());
    provenance["source_refs"] = string_array(evidence_refs.iter().cloned());
    doc["provenance"] = TomlItem::Table(provenance);

    let mut promotion = Table::new();
    promotion["status"] = value("candidate");
    promotion["review_required"] = value(true);
    doc["promotion"] = TomlItem::Table(promotion);

    let mut rollback = Table::new();
    rollback["previous_pack_id"] = value("");
    rollback["reason"] = value(
        "Retire this candidate if cited evidence is false positive or conflicts with project policy.",
    );
    doc["rollback"] = TomlItem::Table(rollback);

    doc
}

fn string_array(values: impl IntoIterator<Item = String>) -> TomlItem {
    let mut array = Array::new();
    for value in values {
        array.push(value);
    }
    TomlItem::Value(array.into())
}

fn context_string(event: &AegisSafetyEvent, key: &str) -> Option<String> {
    event.context.get(key)?.as_str().map(ToOwned::to_owned)
}

fn context_string_array(event: &AegisSafetyEvent, key: &str) -> Vec<String> {
    let Some(values) = event.context.get(key).and_then(|value| value.as_array()) else {
        return Vec::new();
    };
    values
        .iter()
        .filter_map(|value| value.as_str().map(ToOwned::to_owned))
        .collect()
}

fn summary_or_default(event: &AegisSafetyEvent, fallback: &str) -> String {
    clean_text(&event.summary).unwrap_or_else(|| fallback.to_string())
}

fn clean_text(value: &str) -> Option<String> {
    let cleaned = value.split_whitespace().collect::<Vec<_>>().join(" ");
    (!cleaned.is_empty()).then_some(cleaned)
}

fn truncate(value: &str, max_chars: usize) -> String {
    let cleaned = clean_text(value).unwrap_or_default();
    if cleaned.chars().count() <= max_chars {
        return cleaned;
    }
    cleaned.chars().take(max_chars).collect::<String>()
}

fn normalize_key(value: &str) -> String {
    clean_text(value)
        .unwrap_or_default()
        .to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
}

fn slugify(value: &str) -> String {
    let slug = normalize_key(value);
    if slug.is_empty() {
        return "candidate".to_string();
    }
    slug.split('-').take(8).collect::<Vec<_>>().join("-")
}

fn short_hash(value: &str, chars: usize) -> String {
    let digest = sha1::Sha1::digest(value.as_bytes());
    format!("{digest:x}").chars().take(chars).collect()
}

fn severity_label(severity: AegisSafetySeverityHint) -> String {
    match severity {
        AegisSafetySeverityHint::Info => "info",
        AegisSafetySeverityHint::Low => "low",
        AegisSafetySeverityHint::Medium => "medium",
        AegisSafetySeverityHint::High => "high",
        AegisSafetySeverityHint::Critical => "critical",
    }
    .to_string()
}

fn severity_tag(severity: AegisEngineAlertSeverity) -> &'static str {
    match severity {
        AegisEngineAlertSeverity::Safe => "safe",
        AegisEngineAlertSeverity::Suspicious => "suspicious",
        AegisEngineAlertSeverity::Malicious => "malicious",
    }
}

fn default_falsifiers() -> Vec<String> {
    vec![
        "Human review determines the cited evidence is unrelated.".to_string(),
        "The pattern no longer appears after a policy or runtime change.".to_string(),
        "The source task explicitly authorizes the behavior.".to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_exec_server::LOCAL_FS;
    use codex_protocol::aegis_safety_event::AegisSafetyEventSource;
    use codex_utils_absolute_path::AbsolutePathBuf;
    use pretty_assertions::assert_eq;
    use serde_json::json;
    use tempfile::TempDir;

    use crate::context_packs::ContextPackDiagnosticStatus;
    use crate::context_packs::load_context_packs;

    fn options(dir: &TempDir, min_support: usize) -> LearnedPackCompilerOptions {
        LearnedPackCompilerOptions {
            events_path: dir.path().join("events.jsonl"),
            alert_inputs_path: dir.path().join("candidate-pack-inputs.jsonl"),
            output_dir: dir.path().join("learned-candidates"),
            repository: "mithran-hq/aegis-code".to_string(),
            min_support,
            now: "2026-05-08T00:00:00Z".to_string(),
            dry_run: false,
        }
    }

    fn event(
        id: &str,
        category: AegisSafetyEventCategory,
        summary: &str,
        context: serde_json::Map<String, serde_json::Value>,
    ) -> AegisSafetyEvent {
        AegisSafetyEvent {
            event_id: Some(id.to_string()),
            created_at_unix_seconds: Some(1_777_000_000),
            source: AegisSafetyEventSource::AegisCode,
            summary: summary.to_string(),
            category,
            severity_hint: AegisSafetySeverityHint::High,
            tags: Vec::new(),
            context: context.into_iter().collect(),
            redactions: Vec::new(),
        }
    }

    fn write_jsonl<T: Serialize>(path: &Path, items: &[T]) {
        let contents = items
            .iter()
            .map(|item| serde_json::to_string(item).expect("serialize item"))
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(path, format!("{contents}\n")).expect("write jsonl");
    }

    #[tokio::test]
    async fn repeated_review_findings_generate_inactive_candidate_pack() {
        let dir = TempDir::new().expect("tempdir");
        let options = options(&dir, 2);
        write_jsonl(
            &options.events_path,
            &[
                event(
                    "review-1",
                    AegisSafetyEventCategory::Review,
                    "Missing adversarial review before commit",
                    serde_json::Map::from_iter([
                        ("severity".to_string(), json!("high")),
                        ("status".to_string(), json!("open")),
                    ]),
                ),
                event(
                    "review-2",
                    AegisSafetyEventCategory::Review,
                    "Missing adversarial review before commit",
                    serde_json::Map::from_iter([
                        ("severity".to_string(), json!("high")),
                        ("status".to_string(), json!("open")),
                    ]),
                ),
            ],
        );

        let result = compile_learned_pack_candidates(&options).expect("compile candidates");

        assert_eq!(result.candidates.len(), 1);
        let candidate = &result.candidates[0];
        assert_eq!(candidate.group_kind, LearnedPackGroupKind::ReviewFinding);
        assert_eq!(
            candidate.evidence_refs,
            vec!["event:review-1".to_string(), "event:review-2".to_string()]
        );
        let path = AbsolutePathBuf::from_absolute_path(PathBuf::from(&candidate.path))
            .expect("absolute candidate path");
        let set = load_context_packs(LOCAL_FS.as_ref(), &[path]).await;
        let diagnostic = set.diagnostics().first().expect("diagnostic");
        assert_eq!(
            diagnostic.diagnostic_status(),
            ContextPackDiagnosticStatus::Candidate
        );
        assert!(!diagnostic.active);
    }

    #[test]
    fn repeated_tool_denials_generate_candidate() {
        let dir = TempDir::new().expect("tempdir");
        let options = options(&dir, 2);
        write_jsonl(
            &options.events_path,
            &[
                event(
                    "deny-1",
                    AegisSafetyEventCategory::ToolDenial,
                    "Aegis preflight Block for gh",
                    serde_json::Map::from_iter([
                        ("tool_name".to_string(), json!("gh")),
                        ("risk_category".to_string(), json!("mutation")),
                        ("reason".to_string(), json!("missing issue evidence")),
                    ]),
                ),
                event(
                    "deny-2",
                    AegisSafetyEventCategory::ToolDenial,
                    "Aegis preflight Block for gh",
                    serde_json::Map::from_iter([
                        ("tool_name".to_string(), json!("gh")),
                        ("risk_category".to_string(), json!("mutation")),
                        ("reason".to_string(), json!("missing issue evidence")),
                    ]),
                ),
            ],
        );

        let result = compile_learned_pack_candidates(&options).expect("compile candidates");

        assert_eq!(
            result.candidates[0].group_kind,
            LearnedPackGroupKind::ToolDenial
        );
        assert!(result.candidates[0].guidance.contains("gh"));
    }

    #[test]
    fn repeated_stale_resume_issues_generate_candidate() {
        let dir = TempDir::new().expect("tempdir");
        let options = options(&dir, 2);
        write_jsonl(
            &options.events_path,
            &[
                event(
                    "resume-1",
                    AegisSafetyEventCategory::Resume,
                    "Loaded persisted Aegis method state",
                    serde_json::Map::from_iter([
                        ("status".to_string(), json!("stale")),
                        ("reasons".to_string(), json!(["branch_changed"])),
                    ]),
                ),
                event(
                    "resume-2",
                    AegisSafetyEventCategory::Resume,
                    "Loaded persisted Aegis method state",
                    serde_json::Map::from_iter([
                        ("status".to_string(), json!("stale")),
                        ("reasons".to_string(), json!(["branch_changed"])),
                    ]),
                ),
            ],
        );

        let result = compile_learned_pack_candidates(&options).expect("compile candidates");

        assert_eq!(
            result.candidates[0].group_kind,
            LearnedPackGroupKind::StaleResume
        );
        assert!(result.candidates[0].guidance.contains("branch_changed"));
    }

    #[test]
    fn repeated_alert_inputs_generate_candidate_with_provided_falsifiers() {
        let dir = TempDir::new().expect("tempdir");
        let options = options(&dir, 2);
        let input = |id: &str, alert_id: &str| AegisEngineCandidatePackInput {
            schema_version: 1,
            input_id: format!("candidate-input:{alert_id}"),
            alert_id: alert_id.to_string(),
            severity: AegisEngineAlertSeverity::Suspicious,
            summary: "Suspicious repeated command".to_string(),
            source_event: AegisEngineAlertSourceEvent {
                event_id: Some(id.to_string()),
                category: Some("tool_denial".to_string()),
                session_id: None,
                thread_id: None,
                turn_id: None,
                call_id: None,
                evidence_id: None,
                finding_id: None,
            },
            guidance: AegisEngineCandidateGuidance {
                guidance: "Require issue evidence before this command.".to_string(),
                falsifiers: vec!["The command is read-only.".to_string()],
            },
            created_at_unix_seconds: 1,
            received_at_unix_seconds: 2,
        };
        write_jsonl(
            &options.alert_inputs_path,
            &[input("event-1", "alert-1"), input("event-2", "alert-2")],
        );

        let result = compile_learned_pack_candidates(&options).expect("compile candidates");

        assert_eq!(
            result.candidates[0].group_kind,
            LearnedPackGroupKind::AlertCandidate
        );
        let toml = fs::read_to_string(&result.candidates[0].path).expect("read candidate");
        assert!(toml.contains("The command is read-only."));
        assert!(toml.contains("status = \"candidate\""));
        assert!(!toml.contains("status = \"promoted\""));
    }

    #[test]
    fn noisy_false_positives_below_threshold_are_skipped() {
        let dir = TempDir::new().expect("tempdir");
        let options = options(&dir, 2);
        write_jsonl(
            &options.events_path,
            &[event(
                "review-1",
                AegisSafetyEventCategory::Review,
                "One-off review finding",
                serde_json::Map::from_iter([
                    ("severity".to_string(), json!("medium")),
                    ("status".to_string(), json!("open")),
                ]),
            )],
        );

        let result = compile_learned_pack_candidates(&options).expect("compile candidates");

        assert!(result.candidates.is_empty());
        assert_eq!(result.skipped_groups.len(), 1);
        assert!(!options.output_dir.exists());
    }
}
