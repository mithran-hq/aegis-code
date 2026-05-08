use crate::config::Config;
use codex_config::ConfigLayerSource;
use codex_config::ConfigLayerStackOrdering;
use codex_config::default_project_root_markers;
use codex_config::merge_toml_values;
use codex_config::project_root_markers_from_config;
use codex_utils_path::resolve_symlink_write_paths;
use codex_utils_path::write_atomically;
use serde::Serialize;
use similar::TextDiff;
use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use toml::Value as TomlValue;

pub const MANAGED_GUIDANCE_BEGIN_MARKER: &str = "<!-- BEGIN AEGIS CODE MANAGED GUIDANCE -->";
pub const MANAGED_GUIDANCE_END_MARKER: &str = "<!-- END AEGIS CODE MANAGED GUIDANCE -->";

pub const AEGIS_CODE_METHOD_GUIDANCE: &str = r#"## Aegis Code Managed Guidance

- Treat GitHub task issues as the source of truth for implementation scope.
- Work in task-sized slices with one focused commit per completed issue.
- Preserve user-authored instructions and do not remove unmanaged guidance.
- Run the repository's local checks before landing completed task work.
- Use Aegis Secret for wrapped local CLIs such as gh, aws, gcloud, kubectl, and terraform when available.
- Never add Co-Authored-By trailers to commits."#;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ManagedGuidanceAction {
    Install,
    Remove,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ManagedGuidanceStatus {
    Changed,
    Noop,
    Conflict,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ManagedGuidanceResult {
    pub path: PathBuf,
    pub action: ManagedGuidanceAction,
    pub status: ManagedGuidanceStatus,
    pub changed: bool,
    pub dry_run: bool,
    pub diff: String,
    pub diagnostics: Vec<String>,
}

pub fn apply_managed_guidance(
    path: &Path,
    action: ManagedGuidanceAction,
    guidance: &str,
    dry_run: bool,
) -> io::Result<ManagedGuidanceResult> {
    let write_paths = resolve_symlink_write_paths(path)?;
    let read_path = write_paths.read_path.as_deref().unwrap_or(path);
    let before = match fs::read_to_string(read_path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == io::ErrorKind::NotFound => String::new(),
        Err(err) => return Err(err),
    };

    let evaluated = evaluate_managed_guidance(&before, action, guidance);
    let after = match evaluated {
        Ok(after) => after,
        Err(diagnostic) => {
            return Ok(result(
                path,
                action,
                ManagedGuidanceStatus::Conflict,
                false,
                dry_run,
                String::new(),
                vec![diagnostic],
            ));
        }
    };

    if before == after {
        return Ok(result(
            path,
            action,
            ManagedGuidanceStatus::Noop,
            false,
            dry_run,
            String::new(),
            Vec::new(),
        ));
    }

    let diff = unified_diff(path, &before, &after);
    if !dry_run {
        write_atomically(&write_paths.write_path, &after)?;
    }

    Ok(result(
        path,
        action,
        ManagedGuidanceStatus::Changed,
        true,
        dry_run,
        diff,
        Vec::new(),
    ))
}

pub fn user_guidance_path(config: &Config) -> PathBuf {
    config.codex_home.join("AGENTS.md").to_path_buf()
}

pub fn repo_guidance_path(config: &Config) -> io::Result<PathBuf> {
    Ok(project_root_for_guidance(config)?.join("AGENTS.md"))
}

fn evaluate_managed_guidance(
    contents: &str,
    action: ManagedGuidanceAction,
    guidance: &str,
) -> Result<String, String> {
    let existing = find_managed_block(contents)?;
    match action {
        ManagedGuidanceAction::Install => Ok(match existing {
            Some(range) => {
                let mut updated = contents.to_string();
                updated.replace_range(range, &managed_block_without_trailing_newline(guidance));
                updated
            }
            None => append_managed_block(contents, guidance),
        }),
        ManagedGuidanceAction::Remove => Ok(match existing {
            Some(range) => {
                let mut updated = contents.to_string();
                updated.replace_range(extend_range_over_following_newline(contents, range), "");
                updated
            }
            None => contents.to_string(),
        }),
    }
}

fn find_managed_block(contents: &str) -> Result<Option<std::ops::Range<usize>>, String> {
    let starts = contents
        .match_indices(MANAGED_GUIDANCE_BEGIN_MARKER)
        .map(|(idx, _)| idx)
        .collect::<Vec<_>>();
    let ends = contents
        .match_indices(MANAGED_GUIDANCE_END_MARKER)
        .map(|(idx, _)| idx)
        .collect::<Vec<_>>();

    if starts.is_empty() && ends.is_empty() {
        return Ok(None);
    }
    if starts.len() > 1 || ends.len() > 1 {
        return Err(format!(
            "expected at most one Aegis Code managed guidance block, found {} begin markers and {} end markers",
            starts.len(),
            ends.len()
        ));
    }
    if starts.len() != ends.len() {
        return Err(format!(
            "malformed Aegis Code managed guidance markers: found {} begin markers and {} end markers",
            starts.len(),
            ends.len()
        ));
    }

    let start = starts[0];
    let end = ends[0] + MANAGED_GUIDANCE_END_MARKER.len();
    if start > ends[0] {
        return Err(
            "malformed Aegis Code managed guidance markers: end marker appears before begin marker"
                .to_string(),
        );
    }
    Ok(Some(start..end))
}

fn extend_range_over_following_newline(
    contents: &str,
    mut range: std::ops::Range<usize>,
) -> std::ops::Range<usize> {
    if contents[range.end..].starts_with("\r\n") {
        range.end += 2;
    } else if contents[range.end..].starts_with('\n') {
        range.end += 1;
    }
    range
}

fn append_managed_block(contents: &str, guidance: &str) -> String {
    let mut updated = contents.to_string();
    if !updated.is_empty() {
        if !updated.ends_with('\n') {
            updated.push('\n');
        }
        updated.push('\n');
    }
    updated.push_str(&managed_block_with_trailing_newline(guidance));
    updated
}

fn managed_block_with_trailing_newline(guidance: &str) -> String {
    format!(
        "{}\n{}\n{}\n",
        MANAGED_GUIDANCE_BEGIN_MARKER,
        guidance.trim(),
        MANAGED_GUIDANCE_END_MARKER
    )
}

fn managed_block_without_trailing_newline(guidance: &str) -> String {
    format!(
        "{}\n{}\n{}",
        MANAGED_GUIDANCE_BEGIN_MARKER,
        guidance.trim(),
        MANAGED_GUIDANCE_END_MARKER
    )
}

fn unified_diff(path: &Path, before: &str, after: &str) -> String {
    let display = path.display();
    TextDiff::from_lines(before, after)
        .unified_diff()
        .context_radius(3)
        .header(&format!("a/{display}"), &format!("b/{display}"))
        .to_string()
}

fn result(
    path: &Path,
    action: ManagedGuidanceAction,
    status: ManagedGuidanceStatus,
    changed: bool,
    dry_run: bool,
    diff: String,
    diagnostics: Vec<String>,
) -> ManagedGuidanceResult {
    ManagedGuidanceResult {
        path: path.to_path_buf(),
        action,
        status,
        changed,
        dry_run,
        diff,
        diagnostics,
    }
}

fn project_root_for_guidance(config: &Config) -> io::Result<PathBuf> {
    let mut dir = config.cwd.to_path_buf();
    if let Ok(canon) = dunce::canonicalize(&dir) {
        dir = canon;
    }

    let project_root_markers = project_root_markers(config);
    if !project_root_markers.is_empty() {
        for ancestor in dir.ancestors() {
            if project_root_markers
                .iter()
                .any(|marker| ancestor.join(marker).exists())
            {
                return Ok(ancestor.to_path_buf());
            }
        }
    }

    Ok(dir)
}

fn project_root_markers(config: &Config) -> Vec<String> {
    let mut merged = TomlValue::Table(toml::map::Map::new());
    for layer in config.config_layer_stack.get_layers(
        ConfigLayerStackOrdering::LowestPrecedenceFirst,
        /* include_disabled */ false,
    ) {
        if matches!(layer.name, ConfigLayerSource::Project { .. }) {
            continue;
        }
        merge_toml_values(&mut merged, &layer.config);
    }

    match project_root_markers_from_config(&merged) {
        Ok(Some(markers)) => markers,
        Ok(None) => default_project_root_markers(),
        Err(_) => default_project_root_markers(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use tempfile::TempDir;

    #[test]
    fn managed_guidance_install_creates_absent_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("AGENTS.md");

        let result = apply_managed_guidance(
            &path,
            ManagedGuidanceAction::Install,
            AEGIS_CODE_METHOD_GUIDANCE,
            false,
        )
        .unwrap();

        assert_eq!(result.status, ManagedGuidanceStatus::Changed);
        let written = fs::read_to_string(path).unwrap();
        assert!(written.contains(MANAGED_GUIDANCE_BEGIN_MARKER));
        assert!(written.contains("Aegis Code Managed Guidance"));
        assert!(written.contains(MANAGED_GUIDANCE_END_MARKER));
    }

    #[test]
    fn managed_guidance_reinstall_is_idempotent() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("AGENTS.md");

        apply_managed_guidance(
            &path,
            ManagedGuidanceAction::Install,
            AEGIS_CODE_METHOD_GUIDANCE,
            false,
        )
        .unwrap();
        let first = fs::read_to_string(&path).unwrap();
        let result = apply_managed_guidance(
            &path,
            ManagedGuidanceAction::Install,
            AEGIS_CODE_METHOD_GUIDANCE,
            false,
        )
        .unwrap();
        let second = fs::read_to_string(&path).unwrap();

        assert_eq!(result.status, ManagedGuidanceStatus::Noop);
        assert_eq!(first, second);
    }

    #[test]
    fn managed_guidance_install_updates_existing_block_and_preserves_unmanaged_content() {
        let original = format!(
            "before\n\n{}\nold\n{}\nafter\n",
            MANAGED_GUIDANCE_BEGIN_MARKER, MANAGED_GUIDANCE_END_MARKER
        );

        let updated = evaluate_managed_guidance(
            &original,
            ManagedGuidanceAction::Install,
            AEGIS_CODE_METHOD_GUIDANCE,
        )
        .unwrap();

        assert!(updated.starts_with("before\n\n"));
        assert!(updated.ends_with("after\n"));
        assert!(updated.contains("Aegis Code Managed Guidance"));
        assert!(!updated.contains("\nold\n"));
    }

    #[test]
    fn managed_guidance_remove_deletes_only_managed_block() {
        let original = append_managed_block("before\n\nafter\n", AEGIS_CODE_METHOD_GUIDANCE);

        let updated = evaluate_managed_guidance(
            &original,
            ManagedGuidanceAction::Remove,
            AEGIS_CODE_METHOD_GUIDANCE,
        )
        .unwrap();

        assert_eq!(updated, "before\n\nafter\n\n");
    }

    #[test]
    fn managed_guidance_remove_without_block_is_noop() {
        let original = "before\n";

        let updated = evaluate_managed_guidance(
            original,
            ManagedGuidanceAction::Remove,
            AEGIS_CODE_METHOD_GUIDANCE,
        )
        .unwrap();

        assert_eq!(updated, original);
    }

    #[test]
    fn managed_guidance_dry_run_writes_nothing_and_returns_diff() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("AGENTS.md");
        fs::write(&path, "before\n").unwrap();

        let result = apply_managed_guidance(
            &path,
            ManagedGuidanceAction::Install,
            AEGIS_CODE_METHOD_GUIDANCE,
            true,
        )
        .unwrap();

        assert_eq!(result.status, ManagedGuidanceStatus::Changed);
        assert!(result.diff.contains(MANAGED_GUIDANCE_BEGIN_MARKER));
        assert_eq!(fs::read_to_string(path).unwrap(), "before\n");
    }

    #[test]
    fn managed_guidance_multiple_blocks_conflict() {
        let original = format!(
            "{}\none\n{}\n{}\ntwo\n{}\n",
            MANAGED_GUIDANCE_BEGIN_MARKER,
            MANAGED_GUIDANCE_END_MARKER,
            MANAGED_GUIDANCE_BEGIN_MARKER,
            MANAGED_GUIDANCE_END_MARKER
        );

        let err = evaluate_managed_guidance(
            &original,
            ManagedGuidanceAction::Install,
            AEGIS_CODE_METHOD_GUIDANCE,
        )
        .unwrap_err();

        assert!(err.contains("at most one"));
    }

    #[test]
    fn managed_guidance_malformed_markers_conflict() {
        let original = format!("{MANAGED_GUIDANCE_BEGIN_MARKER}\nmissing end\n");

        let err = evaluate_managed_guidance(
            &original,
            ManagedGuidanceAction::Install,
            AEGIS_CODE_METHOD_GUIDANCE,
        )
        .unwrap_err();

        assert!(err.contains("malformed"));
    }

    #[test]
    fn managed_guidance_reversed_markers_conflict() {
        let original = format!("{MANAGED_GUIDANCE_END_MARKER}\n{MANAGED_GUIDANCE_BEGIN_MARKER}\n");

        let err = evaluate_managed_guidance(
            &original,
            ManagedGuidanceAction::Install,
            AEGIS_CODE_METHOD_GUIDANCE,
        )
        .unwrap_err();

        assert!(err.contains("end marker appears before begin marker"));
    }
}
