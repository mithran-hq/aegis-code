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
