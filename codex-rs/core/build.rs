use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("codex-rs/core should be two levels below the repository root");
    let upstream_import = repo_root.join("docs/UPSTREAM_IMPORT.md");

    println!("cargo:rerun-if-changed={}", upstream_import.display());
    println!("cargo:rerun-if-env-changed=AEGIS_SOURCE_REVISION");
    println!("cargo:rerun-if-env-changed=GITHUB_SHA");

    let import_doc = std::fs::read_to_string(&upstream_import).unwrap_or_default();
    let upstream_repository = table_value(&import_doc, "Upstream repository")
        .unwrap_or_else(|| "https://github.com/openai/codex".to_string());
    let upstream_base =
        table_value(&import_doc, "Imported commit").unwrap_or_else(|| "unknown".to_string());
    let source_revision = env::var("AEGIS_SOURCE_REVISION")
        .or_else(|_| env::var("GITHUB_SHA"))
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| git_head(repo_root))
        .unwrap_or_else(|| "unknown".to_string());

    println!("cargo:rustc-env=AEGIS_UPSTREAM_REPOSITORY={upstream_repository}");
    println!("cargo:rustc-env=AEGIS_UPSTREAM_BASE={upstream_base}");
    println!("cargo:rustc-env=AEGIS_SOURCE_REVISION={source_revision}");
}

fn table_value(markdown: &str, field: &str) -> Option<String> {
    for line in markdown.lines() {
        let mut cells = line
            .split('|')
            .map(str::trim)
            .filter(|cell| !cell.is_empty());
        if cells.next()? == field {
            let value = cells.next()?.trim_matches('`').trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn git_head(repo_root: &std::path::Path) -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_root)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let revision = String::from_utf8(output.stdout).ok()?;
    let revision = revision.trim();
    if revision.is_empty() {
        None
    } else {
        Some(revision.to_string())
    }
}
