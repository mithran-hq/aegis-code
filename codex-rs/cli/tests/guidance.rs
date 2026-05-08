use anyhow::Result;
use predicates::str::contains;
use serde_json::Value;
use std::path::Path;
use tempfile::TempDir;

fn codex_command(codex_home: &Path) -> Result<assert_cmd::Command> {
    let mut cmd = assert_cmd::Command::new(codex_utils_cargo_bin::cargo_bin("aegis")?);
    cmd.env("AEGIS_HOME", codex_home);
    Ok(cmd)
}

fn marker_count(contents: &str, marker: &str) -> usize {
    contents.matches(marker).count()
}

#[test]
fn guidance_install_user_is_idempotent_and_preserves_unmanaged_content() -> Result<()> {
    let codex_home = TempDir::new()?;
    let agents = codex_home.path().join("AGENTS.md");
    std::fs::write(&agents, "user note\n")?;

    let mut install = codex_command(codex_home.path())?;
    install
        .args(["guidance", "install", "--target", "user"])
        .assert()
        .success()
        .stdout(contains("Installed Aegis-managed guidance"));

    let written = std::fs::read_to_string(&agents)?;
    assert!(written.contains("user note\n"));
    assert_eq!(
        marker_count(&written, "<!-- BEGIN AEGIS CODE MANAGED GUIDANCE -->"),
        1
    );

    let mut reinstall = codex_command(codex_home.path())?;
    reinstall
        .args(["guidance", "install", "--target", "user"])
        .assert()
        .success()
        .stdout(contains("No Aegis-managed guidance changes needed"));

    let rewritten = std::fs::read_to_string(&agents)?;
    assert_eq!(written, rewritten);
    Ok(())
}

#[test]
fn guidance_repo_dry_run_prints_diff_and_writes_nothing() -> Result<()> {
    let codex_home = TempDir::new()?;
    let repo = TempDir::new()?;
    std::fs::create_dir(repo.path().join(".git"))?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.current_dir(repo.path())
        .args(["guidance", "install", "--target", "repo", "--dry-run"])
        .assert()
        .success()
        .stdout(contains("Dry run"))
        .stdout(contains("+<!-- BEGIN AEGIS CODE MANAGED GUIDANCE -->"));

    assert!(!repo.path().join("AGENTS.md").exists());
    Ok(())
}

#[test]
fn guidance_remove_user_deletes_only_managed_block() -> Result<()> {
    let codex_home = TempDir::new()?;
    let agents = codex_home.path().join("AGENTS.md");
    std::fs::write(&agents, "before\n")?;

    let mut install = codex_command(codex_home.path())?;
    install
        .args(["guidance", "install", "--target", "user"])
        .assert()
        .success();

    let mut remove = codex_command(codex_home.path())?;
    remove
        .args(["guidance", "remove", "--target", "user"])
        .assert()
        .success()
        .stdout(contains("Removed Aegis-managed guidance"));

    let written = std::fs::read_to_string(&agents)?;
    assert!(written.contains("before\n"));
    assert!(!written.contains("Aegis Code Managed Guidance"));
    Ok(())
}

#[test]
fn guidance_conflict_returns_json_diagnostics_and_nonzero_exit() -> Result<()> {
    let codex_home = TempDir::new()?;
    let agents = codex_home.path().join("AGENTS.md");
    std::fs::write(
        &agents,
        "<!-- BEGIN AEGIS CODE MANAGED GUIDANCE -->\nmissing end\n",
    )?;

    let mut cmd = codex_command(codex_home.path())?;
    let output = cmd
        .args(["guidance", "install", "--target", "user", "--json"])
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output)?;

    assert_eq!(json["results"][0]["status"], "conflict");
    assert!(
        json["results"][0]["diagnostics"][0]
            .as_str()
            .expect("diagnostic")
            .contains("malformed")
    );
    assert_eq!(
        std::fs::read_to_string(&agents)?,
        "<!-- BEGIN AEGIS CODE MANAGED GUIDANCE -->\nmissing end\n"
    );
    Ok(())
}

#[test]
fn guidance_all_conflict_does_not_partially_write_other_targets() -> Result<()> {
    let codex_home = TempDir::new()?;
    let repo = TempDir::new()?;
    std::fs::create_dir(repo.path().join(".git"))?;
    std::fs::write(
        repo.path().join("AGENTS.md"),
        "<!-- BEGIN AEGIS CODE MANAGED GUIDANCE -->\nmissing end\n",
    )?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.current_dir(repo.path())
        .args(["guidance", "install", "--target", "all"])
        .assert()
        .failure()
        .stdout(contains("Conflict in"));

    assert!(!codex_home.path().join("AGENTS.md").exists());
    Ok(())
}
