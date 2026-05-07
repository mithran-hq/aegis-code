use super::*;
use crate::agents_md::ProjectAgentsMdLayer;
use crate::state::MethodStatePersistenceStatus;
use pretty_assertions::assert_eq;

fn abs(path: &str) -> AbsolutePathBuf {
    AbsolutePathBuf::try_from(std::path::PathBuf::from(path)).expect("absolute path")
}

#[test]
fn static_prompt_layer_order_is_fixed() {
    let codex_home = abs("/tmp/aegis-home");
    let cwd = abs("/tmp/project");
    let layers = AgentsMdInstructionLayers {
        user: Some("user guidance".to_string()),
        project: Some(ProjectAgentsMdLayer {
            contents: "project guidance".to_string(),
            sources: vec![abs("/tmp/project/AGENTS.md")],
        }),
        child_agents_md_enabled: false,
    };

    let diagnostics =
        build_static_prompt_layer_diagnostics(&codex_home, &cwd, "base guidance", &layers);

    assert_eq!(
        diagnostics
            .iter()
            .map(|layer| layer.kind)
            .collect::<Vec<_>>(),
        vec![
            PromptLayerKind::BuiltInBase,
            PromptLayerKind::UserPack,
            PromptLayerKind::ProjectPack,
            PromptLayerKind::PromotedLearnedPack,
        ]
    );
    assert_eq!(
        diagnostics
            .iter()
            .map(|layer| layer.order)
            .collect::<Vec<_>>(),
        vec![0, 1, 2, 3]
    );
}

#[test]
fn diagnostics_redact_sources_and_not_bodies() {
    let codex_home = abs("/tmp/aegis-home");
    let cwd = abs("/tmp/project");
    let layers = AgentsMdInstructionLayers {
        user: Some("SECRET_TOKEN=abc".to_string()),
        project: Some(ProjectAgentsMdLayer {
            contents: "PASSWORD=abc".to_string(),
            sources: vec![abs("/tmp/project/sub/AGENTS.md")],
        }),
        child_agents_md_enabled: false,
    };

    let diagnostics =
        build_static_prompt_layer_diagnostics(&codex_home, &cwd, "base guidance", &layers);
    let json = serde_json::to_string(&diagnostics).expect("serialize diagnostics");

    assert!(json.contains("$CWD/sub/AGENTS.md"));
    assert!(!json.contains("SECRET_TOKEN"));
    assert!(!json.contains("PASSWORD"));
    assert!(!json.contains("/tmp/project"));
}

#[test]
fn promoted_learned_pack_is_inactive_until_later_issues() {
    let codex_home = abs("/tmp/aegis-home");
    let cwd = abs("/tmp/project");
    let layers = AgentsMdInstructionLayers {
        user: None,
        project: None,
        child_agents_md_enabled: false,
    };

    let diagnostics =
        build_static_prompt_layer_diagnostics(&codex_home, &cwd, "base guidance", &layers);
    let learned = diagnostics
        .iter()
        .find(|layer| layer.kind == PromptLayerKind::PromotedLearnedPack)
        .expect("learned layer exists");

    assert!(!learned.active);
    assert_eq!(learned.source, "context_pack_loader_pending");
}

#[test]
fn current_task_facts_are_user_context() {
    let facts = CurrentTaskFacts::new(
        ThreadId::new(),
        abs("/tmp/project"),
        MethodStatePersistenceStatus::Missing,
    );

    assert_eq!(CurrentTaskFacts::ROLE, "user");
    assert!(facts.render().contains("<current_task_facts>"));
    assert!(facts.render().contains("Method state persistence: missing"));
    assert!(facts.render().contains("do not override safety"));
}
