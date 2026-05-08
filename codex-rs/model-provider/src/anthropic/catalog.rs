use codex_models_manager::model_info::BASE_INSTRUCTIONS;
use codex_protocol::config_types::ReasoningSummary;
use codex_protocol::openai_models::ApplyPatchToolType;
use codex_protocol::openai_models::ConfigShellToolType;
use codex_protocol::openai_models::InputModality;
use codex_protocol::openai_models::ModelInfo;
use codex_protocol::openai_models::ModelVisibility;
use codex_protocol::openai_models::ModelsResponse;
use codex_protocol::openai_models::ReasoningEffort;
use codex_protocol::openai_models::ReasoningEffortPreset;
use codex_protocol::openai_models::TruncationPolicyConfig;
use codex_protocol::openai_models::WebSearchToolType;

const CLAUDE_CONTEXT_WINDOW: i64 = 200_000;

pub(crate) fn static_model_catalog() -> ModelsResponse {
    ModelsResponse {
        models: vec![
            anthropic_model(
                "claude-sonnet-4-20250514",
                "Claude Sonnet 4",
                "Balanced Claude model for coding and agentic workflows.",
                0,
            ),
            anthropic_model(
                "claude-opus-4-1-20250805",
                "Claude Opus 4.1",
                "Most capable Claude model for complex reasoning and coding.",
                1,
            ),
            anthropic_model(
                "claude-3-5-haiku-20241022",
                "Claude Haiku 3.5",
                "Fast Claude model for lower-latency coding tasks.",
                2,
            ),
        ],
    }
}

fn anthropic_model(slug: &str, display_name: &str, description: &str, priority: i32) -> ModelInfo {
    ModelInfo {
        slug: slug.to_string(),
        display_name: display_name.to_string(),
        description: Some(description.to_string()),
        default_reasoning_level: Some(ReasoningEffort::Medium),
        supported_reasoning_levels: vec![
            reasoning_effort_preset(ReasoningEffort::Low),
            reasoning_effort_preset(ReasoningEffort::Medium),
            reasoning_effort_preset(ReasoningEffort::High),
        ],
        shell_type: ConfigShellToolType::ShellCommand,
        visibility: ModelVisibility::List,
        supported_in_api: true,
        priority,
        additional_speed_tiers: Vec::new(),
        service_tiers: Vec::new(),
        availability_nux: None,
        upgrade: None,
        base_instructions: BASE_INSTRUCTIONS.to_string(),
        model_messages: None,
        supports_reasoning_summaries: false,
        default_reasoning_summary: ReasoningSummary::None,
        support_verbosity: false,
        default_verbosity: None,
        apply_patch_tool_type: Some(ApplyPatchToolType::Function),
        web_search_tool_type: WebSearchToolType::Text,
        truncation_policy: TruncationPolicyConfig::tokens(/*limit*/ 10_000),
        supports_parallel_tool_calls: true,
        supports_image_detail_original: false,
        context_window: Some(CLAUDE_CONTEXT_WINDOW),
        max_context_window: Some(CLAUDE_CONTEXT_WINDOW),
        auto_compact_token_limit: None,
        effective_context_window_percent: 95,
        experimental_supported_tools: Vec::new(),
        input_modalities: vec![InputModality::Text, InputModality::Image],
        used_fallback_model_metadata: false,
        supports_search_tool: false,
    }
}

fn reasoning_effort_preset(effort: ReasoningEffort) -> ReasoningEffortPreset {
    ReasoningEffortPreset {
        effort,
        description: match effort {
            ReasoningEffort::None => "No reasoning",
            ReasoningEffort::Minimal => "Minimal reasoning",
            ReasoningEffort::Low => "Fast responses with lighter reasoning",
            ReasoningEffort::Medium => "Balances speed and reasoning depth for everyday tasks",
            ReasoningEffort::High => "Greater reasoning depth for complex problems",
            ReasoningEffort::XHigh => "Extra high reasoning for complex problems",
        }
        .to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn catalog_defaults_to_claude_sonnet() {
        let catalog = static_model_catalog();

        assert_eq!(catalog.models[0].slug, "claude-sonnet-4-20250514");
        assert_eq!(catalog.models[0].priority, 0);
    }
}
