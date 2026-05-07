use std::path::Path;
use std::path::PathBuf;

use anyhow::Result;
use predicates::str::contains;
use pretty_assertions::assert_eq;
use serde_json::Value;
use tempfile::TempDir;

fn codex_command(codex_home: &Path) -> Result<assert_cmd::Command> {
    let mut cmd = assert_cmd::Command::new(codex_utils_cargo_bin::cargo_bin("aegis")?);
    cmd.env("AEGIS_HOME", codex_home);
    Ok(cmd)
}

fn learned_pack(pack_id: &str, status: &str, guidance: &str, previous_pack_id: &str) -> String {
    format!(
        r#"
schema_version = 1
pack_id = "{pack_id}"
kind = "learned"
name = "{pack_id}"

[compatibility]
schema = "1"

[[guidance]]
id = "guidance:{pack_id}"
category = "method"
text = "{guidance}"
falsifiers = ["new evidence invalidates this"]

[provenance]
author = "test"
source = "integration-test"
created_at = "2026-05-07T00:00:00Z"

[promotion]
status = "{status}"

[rollback]
previous_pack_id = "{previous_pack_id}"
reason = "rollback metadata"
"#
    )
}

fn write_config(codex_home: &Path, paths: &[PathBuf]) -> Result<()> {
    let encoded_paths = paths
        .iter()
        .map(|path| format!("{:?}", path.display().to_string()))
        .collect::<Vec<_>>()
        .join(", ");
    std::fs::write(
        codex_home.join("config.toml"),
        format!("context_pack_paths = [{encoded_paths}]\n"),
    )?;
    Ok(())
}

#[tokio::test]
async fn context_pack_promote_and_rollback_update_configured_packs() -> Result<()> {
    let codex_home = TempDir::new()?;
    let pack_dir = TempDir::new()?;
    let active = pack_dir.path().join("active.toml");
    let candidate = pack_dir.path().join("candidate.toml");
    std::fs::write(
        &active,
        learned_pack("learned:active", "promoted", "Active guidance", ""),
    )?;
    std::fs::write(
        &candidate,
        learned_pack("learned:candidate", "candidate", "Candidate guidance", ""),
    )?;
    write_config(codex_home.path(), &[active.clone(), candidate.clone()])?;

    let mut list = codex_command(codex_home.path())?;
    let list_output = list
        .args(["context-pack", "list", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let diagnostics: Value = serde_json::from_slice(&list_output)?;
    assert_eq!(diagnostics.as_array().expect("array").len(), 2);

    let mut inspect = codex_command(codex_home.path())?;
    let inspect_output = inspect
        .args(["context-pack", "inspect", "learned:candidate", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let inspection: Value = serde_json::from_slice(&inspect_output)?;
    assert_eq!(inspection["pack_id"], "learned:candidate");
    assert_eq!(inspection["promotion"]["status"], "candidate");

    let mut promote = codex_command(codex_home.path())?;
    promote
        .args([
            "context-pack",
            "promote",
            "learned:candidate",
            "--actor",
            "Test Actor",
            "--evidence",
            "issue:13",
            "--reason",
            "reviewed",
        ])
        .assert()
        .success()
        .stdout(contains("promote learned:candidate: candidate -> promoted"));

    let candidate_toml = std::fs::read_to_string(&candidate)?;
    assert!(candidate_toml.contains(r#"status = "promoted""#));
    assert!(candidate_toml.contains(r#"promoted_by = "Test Actor""#));
    assert!(candidate_toml.contains(r#"source_evidence = ["issue:13"]"#));
    assert!(candidate_toml.contains(r#"previous_pack_id = "learned:active""#));

    let mut lineage = codex_command(codex_home.path())?;
    let lineage_output = lineage
        .args(["context-pack", "lineage", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let lineage: Value = serde_json::from_slice(&lineage_output)?;
    assert_eq!(lineage[0]["pack_id"], "learned:candidate");
    assert_eq!(lineage[0]["previous_pack_id"], "learned:active");

    let mut rollback = codex_command(codex_home.path())?;
    rollback
        .args([
            "context-pack",
            "rollback",
            "--actor",
            "Test Actor",
            "--reason",
            "undo",
        ])
        .assert()
        .success()
        .stdout(contains(
            "rollback-restore learned:active: retired -> promoted",
        ));

    let active_toml = std::fs::read_to_string(&active)?;
    assert!(active_toml.contains(r#"status = "promoted""#));
    assert!(active_toml.contains(r#"source_evidence = ["rollback:learned:candidate"]"#));

    let candidate_toml = std::fs::read_to_string(&candidate)?;
    assert!(candidate_toml.contains(r#"status = "retired""#));
    assert!(candidate_toml.contains(r#"retire_reason = "undo""#));

    let mut retire = codex_command(codex_home.path())?;
    retire
        .args([
            "context-pack",
            "retire",
            "learned:active",
            "--actor",
            "Test Actor",
            "--reason",
            "manual retirement",
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout(contains("Dry run; no files changed."));

    Ok(())
}

#[tokio::test]
async fn context_pack_promote_requires_evidence_flag() -> Result<()> {
    let codex_home = TempDir::new()?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.args(["context-pack", "promote", "learned:candidate"])
        .assert()
        .failure()
        .stderr(contains("--evidence"));

    Ok(())
}
