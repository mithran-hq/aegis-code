use anyhow::Result;
use predicates::str::contains;
use serde_json::Value as JsonValue;
use std::path::Path;
use tempfile::TempDir;

fn codex_command(codex_home: &Path) -> Result<assert_cmd::Command> {
    let mut cmd = assert_cmd::Command::new(codex_utils_cargo_bin::cargo_bin("aegis")?);
    cmd.env("AEGIS_HOME", codex_home);
    Ok(cmd)
}

#[test]
fn config_import_codex_preview_does_not_write_aegis_config() -> Result<()> {
    let temp = TempDir::new()?;
    let codex_home = temp.path().join("codex");
    let aegis_home = temp.path().join("aegis");
    std::fs::create_dir_all(&codex_home)?;
    std::fs::create_dir_all(&aegis_home)?;
    let source = codex_home.join("config.toml");
    let destination = aegis_home.join("config.toml");
    std::fs::write(&source, "model = \"gpt-5\"\n")?;

    let mut cmd = codex_command(&aegis_home)?;
    cmd.args([
        "config",
        "import-codex",
        "--from",
        source.to_str().expect("utf-8 path"),
        "--to",
        destination.to_str().expect("utf-8 path"),
    ])
    .assert()
    .success()
    .stdout(contains("Preview only"));

    assert!(!destination.exists());
    assert_eq!(std::fs::read_to_string(source)?, "model = \"gpt-5\"\n");
    Ok(())
}

#[test]
fn config_import_codex_apply_writes_only_aegis_config() -> Result<()> {
    let temp = TempDir::new()?;
    let codex_home = temp.path().join("codex");
    let aegis_home = temp.path().join("aegis");
    std::fs::create_dir_all(&codex_home)?;
    std::fs::create_dir_all(&aegis_home)?;
    let source = codex_home.join("config.toml");
    let destination = aegis_home.join("config.toml");
    std::fs::write(
        &source,
        r#"
model = "gpt-5"
instructions = "skip me"

[model_providers.safe]
name = "Safe"
base_url = "https://example.test/v1"
env_key = "SAFE_API_KEY"
wire_api = "responses"
"#,
    )?;

    let mut cmd = codex_command(&aegis_home)?;
    cmd.args([
        "config",
        "import-codex",
        "--apply",
        "--from",
        source.to_str().expect("utf-8 path"),
        "--to",
        destination.to_str().expect("utf-8 path"),
    ])
    .assert()
    .success()
    .stdout(contains("Applied import"));

    let written = std::fs::read_to_string(destination)?;
    assert!(written.contains("model = \"gpt-5\""));
    assert!(written.contains("[model_providers.safe]"));
    assert!(!written.contains("skip me"));
    assert!(std::fs::read_to_string(source)?.contains("instructions = \"skip me\""));
    Ok(())
}

#[test]
fn config_import_codex_json_redacts_secret_values() -> Result<()> {
    let temp = TempDir::new()?;
    let codex_home = temp.path().join("codex");
    let aegis_home = temp.path().join("aegis");
    std::fs::create_dir_all(&codex_home)?;
    std::fs::create_dir_all(&aegis_home)?;
    let source = codex_home.join("config.toml");
    let destination = aegis_home.join("config.toml");
    std::fs::write(
        &source,
        r#"
[model_providers.secretful]
name = "Secretful"
base_url = "https://example.test/v1"
experimental_bearer_token = "raw-secret"
http_headers = { Authorization = "Bearer raw-secret" }
"#,
    )?;

    let mut cmd = codex_command(&aegis_home)?;
    let output = cmd
        .args([
            "config",
            "import-codex",
            "--json",
            "--from",
            source.to_str().expect("utf-8 path"),
            "--to",
            destination.to_str().expect("utf-8 path"),
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8(output)?;
    let json: JsonValue = serde_json::from_str(&stdout)?;

    assert_eq!(json["applied"], false);
    assert!(!stdout.contains("raw-secret"));
    assert!(!destination.exists());
    Ok(())
}
