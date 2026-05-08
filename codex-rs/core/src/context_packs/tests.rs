use super::*;
use codex_exec_server::LOCAL_FS;
use pretty_assertions::assert_eq;
use tempfile::TempDir;

fn abs(path: impl Into<std::path::PathBuf>) -> AbsolutePathBuf {
    AbsolutePathBuf::try_from(path.into()).expect("absolute path")
}

fn write_pack(dir: &TempDir, name: &str, contents: &str) -> AbsolutePathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, contents).expect("write pack");
    abs(path)
}

fn read_pack(path: &AbsolutePathBuf) -> String {
    std::fs::read_to_string(path).expect("read pack")
}

async fn load(paths: &[AbsolutePathBuf]) -> ContextPackSet {
    load_context_packs(LOCAL_FS.as_ref(), paths).await
}

fn pack(kind: &str, status: &str, guidance_text: &str) -> String {
    let rollback = if kind == "learned" && status == "promoted" {
        r#"
[rollback]
previous_pack_id = "learned:previous"
reason = "reviewed replacement"
"#
    } else {
        ""
    };
    let falsifiers = if kind == "learned" {
        r#"falsifiers = ["new evidence disproves this"]"#
    } else {
        ""
    };

    format!(
        r#"
schema_version = 1
pack_id = "{kind}:example"
kind = "{kind}"
name = "{kind} example"

[compatibility]
schema = "1"

[[guidance]]
id = "guidance:one"
category = "method"
text = "{guidance_text}"
{falsifiers}

[provenance]
author = "tester"
source = "unit-test"
created_at = "2026-05-07T00:00:00Z"

[promotion]
status = "{status}"
{rollback}
"#
    )
}

#[tokio::test]
async fn promoted_user_project_and_learned_packs_are_active() {
    let dir = tempfile::tempdir().expect("tempdir");
    let user = write_pack(
        &dir,
        "user.toml",
        &pack("user", "promoted", "User guidance"),
    );
    let project = write_pack(
        &dir,
        "project.toml",
        &pack("project", "promoted", "Project guidance"),
    );
    let learned = write_pack(
        &dir,
        "learned.toml",
        &pack("learned", "promoted", "Learned guidance"),
    );

    let set = load(&[user, project, learned]).await;

    assert!(set.diagnostics().iter().all(|diag| diag.active));
    assert_eq!(
        set.guidance_layer(ContextPackKind::User)
            .expect("user layer")
            .contents,
        "--- context-pack:user:example ---\n\nUser guidance"
    );
    assert!(
        set.guidance_layer(ContextPackKind::Project)
            .expect("project layer")
            .contents
            .contains("Project guidance")
    );
    assert!(
        set.guidance_layer(ContextPackKind::Learned)
            .expect("learned layer")
            .contents
            .contains("Learned guidance")
    );
}

#[tokio::test]
async fn invalid_pack_fails_closed() {
    let dir = tempfile::tempdir().expect("tempdir");
    let invalid = write_pack(
        &dir,
        "invalid.toml",
        r#"
schema_version = 1
pack_id = ""
kind = "user"
name = ""

[compatibility]
schema = "1"

[promotion]
status = "promoted"
"#,
    );

    let set = load(&[invalid]).await;

    assert_eq!(set.active_packs.len(), 0);
    assert!(!set.diagnostics()[0].active);
    assert!(set.diagnostics()[0].reason.contains("invalid:"));
    assert!(set.guidance_layer(ContextPackKind::User).is_none());
}

#[tokio::test]
async fn candidate_pack_is_visible_but_inactive() {
    let dir = tempfile::tempdir().expect("tempdir");
    let candidate = write_pack(
        &dir,
        "candidate.toml",
        &pack("project", "candidate", "Candidate guidance"),
    );

    let set = load(&[candidate]).await;

    assert_eq!(set.diagnostics().len(), 1);
    assert_eq!(set.diagnostics()[0].reason, "promotion_status_candidate");
    assert!(!set.diagnostics()[0].active);
    assert!(set.guidance_layer(ContextPackKind::Project).is_none());
}

#[tokio::test]
async fn promoted_pack_exposes_provider_default_candidates_in_path_order() {
    let dir = tempfile::tempdir().expect("tempdir");
    let first = write_pack(
        &dir,
        "first.toml",
        &pack("project", "promoted", "Project guidance").replace(
            "[provenance]",
            r#"[provider_defaults]
preferred = "missing-provider"
fallbacks = ["anthropic"]

[provenance]"#,
        ),
    );
    let second = write_pack(
        &dir,
        "second.toml",
        &pack("user", "promoted", "User guidance").replace(
            "[provenance]",
            r#"[provider_defaults]
preferred = "openai"
fallbacks = ["ollama"]

[provenance]"#,
        ),
    );

    let set = load(&[first, second]).await;
    let candidates = set.active_provider_default_candidates();

    assert_eq!(
        candidates
            .iter()
            .map(|candidate| (
                candidate.pack_id.as_str(),
                candidate.provider_id.as_str(),
                candidate.field.as_str()
            ))
            .collect::<Vec<_>>(),
        vec![
            ("project:example", "missing-provider", "preferred"),
            ("project:example", "anthropic", "fallback"),
            ("user:example", "openai", "preferred"),
            ("user:example", "ollama", "fallback"),
        ]
    );
}

#[tokio::test]
async fn empty_provider_default_values_make_pack_inactive() {
    let dir = tempfile::tempdir().expect("tempdir");
    let invalid = write_pack(
        &dir,
        "invalid.toml",
        &pack("project", "promoted", "Project guidance").replace(
            "[provenance]",
            r#"[provider_defaults]
preferred = ""
fallbacks = ["ollama", " "]

[provenance]"#,
        ),
    );

    let set = load(&[invalid]).await;

    assert!(!set.diagnostics()[0].active);
    assert!(
        set.diagnostics()[0]
            .reason
            .contains("provider_defaults.preferred must not be empty")
    );
    assert!(
        set.diagnostics()[0]
            .reason
            .contains("provider_defaults.fallbacks must not be empty")
    );
    assert!(set.active_provider_default_candidates().is_empty());
}

#[tokio::test]
async fn incompatible_schema_is_inactive() {
    let dir = tempfile::tempdir().expect("tempdir");
    let incompatible = write_pack(
        &dir,
        "incompatible.toml",
        &pack("user", "promoted", "User guidance").replace("schema = \"1\"", "schema = \"2\""),
    );

    let set = load(&[incompatible]).await;

    assert!(!set.diagnostics()[0].active);
    assert!(
        set.diagnostics()[0]
            .reason
            .contains("unsupported compatibility.schema")
    );
}

#[tokio::test]
async fn promoted_learned_pack_requires_rollback() {
    let dir = tempfile::tempdir().expect("tempdir");
    let learned = write_pack(
        &dir,
        "learned.toml",
        &pack("learned", "promoted", "Learned guidance")
            .replace("[rollback]\nprevious_pack_id = \"learned:previous\"\nreason = \"reviewed replacement\"\n", ""),
    );

    let set = load(&[learned]).await;

    assert!(!set.diagnostics()[0].active);
    assert!(
        set.diagnostics()[0]
            .reason
            .contains("promoted learned packs require rollback")
    );
}

#[tokio::test]
async fn learned_pack_requires_provenance_and_falsifiers() {
    let dir = tempfile::tempdir().expect("tempdir");
    let learned = write_pack(
        &dir,
        "learned.toml",
        &pack("learned", "candidate", "Learned guidance")
            .replace("falsifiers = [\"new evidence disproves this\"]", "")
            .replace(
                r#"
[provenance]
author = "tester"
source = "unit-test"
created_at = "2026-05-07T00:00:00Z"
"#,
                "",
            ),
    );

    let set = load(&[learned]).await;

    assert!(!set.diagnostics()[0].active);
    assert!(set.diagnostics()[0].reason.contains("require provenance"));
    assert!(
        set.diagnostics()[0]
            .reason
            .contains("must include at least one falsifier")
    );
}

#[tokio::test]
async fn retired_pack_is_visible_but_inactive() {
    let dir = tempfile::tempdir().expect("tempdir");
    let retired = write_pack(
        &dir,
        "retired.toml",
        &pack("user", "retired", "Retired guidance"),
    );

    let set = load(&[retired]).await;

    assert_eq!(set.diagnostics()[0].reason, "promotion_status_retired");
    assert!(!set.diagnostics()[0].active);
    assert!(set.guidance_layer(ContextPackKind::User).is_none());
}

#[test]
fn promote_candidate_records_audit_and_retires_prior_active_pack() {
    let dir = tempfile::tempdir().expect("tempdir");
    let active = write_pack(
        &dir,
        "active.toml",
        &pack("learned", "promoted", "Active guidance")
            .replace("learned:example", "learned:active"),
    );
    let candidate = write_pack(
        &dir,
        "candidate.toml",
        &pack("learned", "candidate", "Candidate guidance")
            .replace("learned:example", "learned:candidate"),
    );

    let result = promote_context_pack(
        &[active.clone(), candidate.clone()],
        "learned:candidate",
        "Tester <tester@example.com>",
        &["issue:13".to_string()],
        Some("Reviewed evidence"),
        "2026-05-07T12:00:00Z",
        false,
    )
    .expect("promote");

    assert!(!result.dry_run);
    assert_eq!(result.changes.len(), 2);
    let active_toml = read_pack(&active);
    assert!(active_toml.contains(r#"status = "retired""#));
    assert!(active_toml.contains(r#"retired_by = "Tester <tester@example.com>""#));
    assert!(active_toml.contains(r#"retire_reason = "Retired by promotion of learned:candidate""#));

    let candidate_toml = read_pack(&candidate);
    assert!(candidate_toml.contains(r#"status = "promoted""#));
    assert!(candidate_toml.contains(r#"promoted_at = "2026-05-07T12:00:00Z""#));
    assert!(candidate_toml.contains(r#"promoted_by = "Tester <tester@example.com>""#));
    assert!(candidate_toml.contains(r#"source_evidence = ["issue:13"]"#));
    assert!(candidate_toml.contains(r#"previous_pack_id = "learned:active""#));
    assert!(candidate_toml.contains(r#"reason = "Reviewed evidence""#));
}

#[test]
fn promote_requires_source_evidence() {
    let dir = tempfile::tempdir().expect("tempdir");
    let candidate = write_pack(
        &dir,
        "candidate.toml",
        &pack("learned", "candidate", "Candidate guidance"),
    );

    let err = promote_context_pack(
        &[candidate],
        "learned:example",
        "Tester",
        &[],
        Some("Reviewed"),
        "2026-05-07T12:00:00Z",
        false,
    )
    .expect_err("promotion without evidence should fail");

    assert!(err.to_string().contains("requires at least one --evidence"));
}

#[test]
fn rollback_restores_prior_promoted_pack() {
    let dir = tempfile::tempdir().expect("tempdir");
    let current = write_pack(
        &dir,
        "current.toml",
        &pack("learned", "promoted", "Current guidance")
            .replace("learned:example", "learned:current")
            .replace("learned:previous", "learned:previous"),
    );
    let previous = write_pack(
        &dir,
        "previous.toml",
        &pack("learned", "promoted", "Previous guidance")
            .replace(
                "pack_id = \"learned:example\"",
                "pack_id = \"learned:previous\"",
            )
            .replace("status = \"promoted\"", "status = \"retired\"")
            .replace(
                "previous_pack_id = \"learned:previous\"",
                "previous_pack_id = \"learned:earlier\"",
            ),
    );

    let result = rollback_context_pack(
        &[current.clone(), previous.clone()],
        None,
        "Tester",
        "Rollback after review",
        "2026-05-07T12:30:00Z",
        false,
    )
    .expect("rollback");

    assert_eq!(result.changes.len(), 2);
    let current_toml = read_pack(&current);
    assert!(current_toml.contains(r#"status = "retired""#));
    assert!(current_toml.contains(r#"retire_reason = "Rollback after review""#));

    let previous_toml = read_pack(&previous);
    assert!(previous_toml.contains(r#"status = "promoted""#));
    assert!(previous_toml.contains(r#"promoted_by = "Tester""#));
    assert!(previous_toml.contains(r#"source_evidence = ["rollback:learned:current"]"#));
    assert!(previous_toml.contains(r#"previous_pack_id = "learned:earlier""#));
    assert!(previous_toml.contains(r#"reason = "Rollback after review""#));
}

#[test]
fn rollback_fails_when_active_pack_has_no_prior_version() {
    let dir = tempfile::tempdir().expect("tempdir");
    let active = write_pack(
        &dir,
        "active.toml",
        &pack("learned", "promoted", "Active guidance").replace(
            "previous_pack_id = \"learned:previous\"",
            "previous_pack_id = \"\"",
        ),
    );

    let err = rollback_context_pack(
        &[active],
        None,
        "Tester",
        "Rollback",
        "2026-05-07T12:30:00Z",
        false,
    )
    .expect_err("rollback without prior pack should fail");

    assert!(err.to_string().contains("has no prior active version"));
}

#[test]
fn retire_records_actor_timestamp_and_reason() {
    let dir = tempfile::tempdir().expect("tempdir");
    let user = write_pack(
        &dir,
        "user.toml",
        &pack("user", "promoted", "User guidance"),
    );

    retire_context_pack(
        &[user.clone()],
        "user:example",
        "Tester",
        "Superseded",
        "2026-05-07T13:00:00Z",
        false,
    )
    .expect("retire");

    let user_toml = read_pack(&user);
    assert!(user_toml.contains(r#"status = "retired""#));
    assert!(user_toml.contains(r#"retired_at = "2026-05-07T13:00:00Z""#));
    assert!(user_toml.contains(r#"retired_by = "Tester""#));
    assert!(user_toml.contains(r#"retire_reason = "Superseded""#));
}

#[test]
fn lineage_reports_broken_prior_pack_id() {
    let dir = tempfile::tempdir().expect("tempdir");
    let current = write_pack(
        &dir,
        "current.toml",
        &pack("learned", "promoted", "Current guidance")
            .replace("learned:example", "learned:current")
            .replace("learned:previous", "learned:missing"),
    );

    let lineage = context_pack_lineage(&[current], None).expect("lineage");

    assert_eq!(lineage.len(), 1);
    assert_eq!(lineage[0].pack_id, "learned:current");
    assert_eq!(
        lineage[0].broken_previous_pack_id.as_deref(),
        Some("learned:missing")
    );
}

#[tokio::test]
async fn loaded_context_pack_set_does_not_reload_after_promotion() {
    let dir = tempfile::tempdir().expect("tempdir");
    let active = write_pack(
        &dir,
        "active.toml",
        &pack("learned", "promoted", "Active guidance")
            .replace("learned:example", "learned:active"),
    );
    let candidate = write_pack(
        &dir,
        "candidate.toml",
        &pack("learned", "candidate", "Candidate guidance")
            .replace("learned:example", "learned:candidate"),
    );
    let paths = vec![active, candidate];
    let loaded = load(&paths).await;

    promote_context_pack(
        &paths,
        "learned:candidate",
        "Tester",
        &["issue:13".to_string()],
        Some("Reviewed"),
        "2026-05-07T12:00:00Z",
        false,
    )
    .expect("promote");

    assert!(
        loaded
            .guidance_layer(ContextPackKind::Learned)
            .expect("loaded learned layer")
            .contents
            .contains("Active guidance")
    );

    let reloaded = load(&paths).await;
    let reloaded_guidance = reloaded
        .guidance_layer(ContextPackKind::Learned)
        .expect("reloaded learned layer")
        .contents;
    assert!(reloaded_guidance.contains("Candidate guidance"));
    assert!(!reloaded_guidance.contains("Active guidance"));
}

#[tokio::test]
async fn secret_like_key_is_rejected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let secret = write_pack(
        &dir,
        "secret.toml",
        &format!(
            r#"
{}
api_key = "sk-test"
"#,
            pack("user", "promoted", "User guidance")
        ),
    );

    let set = load(&[secret]).await;

    assert!(!set.diagnostics()[0].active);
    assert!(set.diagnostics()[0].reason.contains("secret-like key"));
    assert!(set.guidance_layer(ContextPackKind::User).is_none());
}
