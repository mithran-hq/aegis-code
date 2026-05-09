//! Versioned Aegis SafetyEvent contract emitted by Aegis Code.

use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use ts_rs::TS;

pub const AEGIS_SAFETY_EVENT_SOURCE_TAG: &str = "aegis-code";

pub type AegisSafetyEventContext = BTreeMap<String, Value>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct AegisSafetyEvent {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub event_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub created_at_unix_seconds: Option<i64>,
    pub source: AegisSafetyEventSource,
    pub summary: String,
    pub category: AegisSafetyEventCategory,
    pub severity_hint: AegisSafetySeverityHint,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub context: AegisSafetyEventContext,
    #[serde(default)]
    pub redactions: Vec<AegisSafetyRedactionRule>,
}

impl AegisSafetyEvent {
    pub fn new(
        category: AegisSafetyEventCategory,
        severity_hint: AegisSafetySeverityHint,
        summary: impl Into<String>,
        tags: Vec<String>,
        context: AegisSafetyEventContext,
        redactions: Vec<AegisSafetyRedactionRule>,
    ) -> Self {
        let mut tags = tags;
        let category_tag = format!("category:{}", category.tag_value());
        if !tags.iter().any(|tag| tag == &category_tag) {
            tags.insert(0, category_tag);
        }

        Self {
            event_id: None,
            created_at_unix_seconds: None,
            source: AegisSafetyEventSource::AegisCode,
            summary: summary.into(),
            category,
            severity_hint,
            tags,
            context,
            redactions,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "kebab-case")]
pub enum AegisSafetyEventSource {
    AegisCode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum AegisSafetyEventCategory {
    MethodGate,
    ToolCall,
    ToolDenial,
    Evidence,
    Resume,
    Provider,
    Sandbox,
    Review,
    Runtime,
}

impl AegisSafetyEventCategory {
    pub fn tag_value(self) -> &'static str {
        match self {
            Self::MethodGate => "method_gate",
            Self::ToolCall => "tool_call",
            Self::ToolDenial => "tool_denial",
            Self::Evidence => "evidence",
            Self::Resume => "resume",
            Self::Provider => "provider",
            Self::Sandbox => "sandbox",
            Self::Review => "review",
            Self::Runtime => "runtime",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum AegisSafetySeverityHint {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct AegisSafetyRedactionRule {
    pub field_path: String,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub replacement: Option<String>,
}

impl AegisSafetyRedactionRule {
    pub fn new(field_path: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            field_path: field_path.into(),
            reason: reason.into(),
            replacement: Some("<redacted>".to_string()),
        }
    }
}

pub fn redact_safety_event_argv(
    argv: &[String],
    field_path_prefix: &str,
) -> (Vec<String>, Vec<AegisSafetyRedactionRule>) {
    let mut redactions = Vec::new();
    let mut redact_next = false;
    let mut redacted = Vec::with_capacity(argv.len());

    for (index, arg) in argv.iter().enumerate() {
        if redact_next {
            redactions.push(AegisSafetyRedactionRule::new(
                format!("{field_path_prefix}[{index}]"),
                "sensitive argv value",
            ));
            redacted.push("<redacted>".to_string());
            redact_next = false;
            continue;
        }

        if !is_sensitive_token(arg) {
            redacted.push(arg.clone());
            continue;
        }

        if let Some((name, _)) = arg.split_once('=') {
            redactions.push(AegisSafetyRedactionRule::new(
                format!("{field_path_prefix}[{index}]"),
                "sensitive argv assignment",
            ));
            redacted.push(format!("{name}=<redacted>"));
        } else if arg.starts_with('-') {
            redacted.push(arg.clone());
            redact_next = true;
        } else {
            redactions.push(AegisSafetyRedactionRule::new(
                format!("{field_path_prefix}[{index}]"),
                "sensitive argv token",
            ));
            redacted.push("<redacted>".to_string());
        }
    }

    (redacted, redactions)
}

pub fn redact_safety_event_text(
    text: &str,
    field_path: &str,
) -> (String, Vec<AegisSafetyRedactionRule>) {
    let mut redacted_any = false;
    let mut redact_next = false;
    let text = text
        .split_whitespace()
        .map(|token| {
            if redact_next {
                redacted_any = true;
                redact_next = false;
                return "<redacted>";
            }

            if is_sensitive_token(token) {
                redacted_any = true;
                let lower = token.to_ascii_lowercase();
                redact_next = lower.contains("authorization")
                    || lower.contains("bearer")
                    || lower.contains("token")
                    || lower.contains("password")
                    || lower.contains("secret")
                    || lower.contains("api-key")
                    || lower.contains("apikey")
                    || lower == "-k"
                    || lower == "--key";
                "<redacted>"
            } else {
                token
            }
        })
        .collect::<Vec<_>>()
        .join(" ");

    let redactions = if redacted_any {
        vec![AegisSafetyRedactionRule::new(
            field_path,
            "sensitive text token",
        )]
    } else {
        Vec::new()
    };

    (text, redactions)
}

fn is_sensitive_token(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("token")
        || lower.contains("password")
        || lower.contains("secret")
        || lower.contains("authorization")
        || lower.contains("bearer")
        || lower.contains("api-key")
        || lower.contains("api_key")
        || lower.contains("apikey")
        || lower == "-k"
        || lower == "--key"
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    #[test]
    fn event_round_trips_with_aegis_code_source() {
        let event = AegisSafetyEvent::new(
            AegisSafetyEventCategory::ToolDenial,
            AegisSafetySeverityHint::High,
            "Blocked repository mutation",
            vec!["tool:exec_command".to_string(), "verdict:block".to_string()],
            BTreeMap::from([("tool_name".to_string(), json!("exec_command"))]),
            Vec::new(),
        );

        let value = serde_json::to_value(&event).expect("serialize event");
        assert_eq!(value["source"], AEGIS_SAFETY_EVENT_SOURCE_TAG);
        assert_eq!(value["category"], "tool_denial");
        assert_eq!(value["severity_hint"], "high");
        assert!(
            value["tags"]
                .as_array()
                .expect("tags")
                .contains(&json!("category:tool_denial"))
        );

        let round_tripped: AegisSafetyEvent =
            serde_json::from_value(value).expect("deserialize event");
        assert_eq!(round_tripped, event);
    }

    #[test]
    fn rejects_non_aegis_code_source() {
        let value = json!({
            "source": "other",
            "summary": "bad source",
            "category": "runtime",
            "severity_hint": "info",
            "tags": [],
            "context": {},
            "redactions": []
        });

        assert!(serde_json::from_value::<AegisSafetyEvent>(value).is_err());
    }

    #[test]
    fn redacts_sensitive_argv_values() {
        let argv = vec![
            "gh".to_string(),
            "api".to_string(),
            "--token".to_string(),
            "secret-value".to_string(),
            "password=hunter2".to_string(),
            "OPENAI_API_KEY=sk-redaction-test".to_string(),
        ];

        let (redacted, redactions) = redact_safety_event_argv(&argv, "context.command.argv");

        assert_eq!(
            redacted,
            vec![
                "gh",
                "api",
                "--token",
                "<redacted>",
                "password=<redacted>",
                "OPENAI_API_KEY=<redacted>"
            ]
        );
        assert_eq!(redactions.len(), 3);
        assert_eq!(redactions[0].field_path, "context.command.argv[3]");
        assert_eq!(redactions[1].field_path, "context.command.argv[4]");
        assert_eq!(redactions[2].field_path, "context.command.argv[5]");
    }

    #[test]
    fn redacts_sensitive_text_but_keeps_harmless_context() {
        let (redacted, redactions) = redact_safety_event_text(
            "Authorization: Bearer secret-value api_key=sk-redaction-test build passed trace_id=40",
            "context.receipt.output_summary",
        );

        assert_eq!(
            redacted,
            "<redacted> <redacted> <redacted> <redacted> build passed trace_id=40"
        );
        assert_eq!(redactions.len(), 1);
        assert_eq!(redactions[0].field_path, "context.receipt.output_summary");
    }

    #[test]
    fn documentation_examples_deserialize() {
        let docs = include_str!("../../../docs/aegis-runtime-events.md");
        let json_blocks = extract_json_blocks(docs);
        assert!(
            json_blocks.len() >= 9,
            "expected examples for each event family"
        );

        for block in json_blocks {
            serde_json::from_str::<AegisSafetyEvent>(&block)
                .expect("SafetyEvent example should deserialize");
        }
    }

    fn extract_json_blocks(markdown: &str) -> Vec<String> {
        let mut blocks = Vec::new();
        let mut in_json = false;
        let mut current = Vec::new();

        for line in markdown.lines() {
            if line.trim() == "```json" {
                in_json = true;
                current.clear();
                continue;
            }

            if line.trim() == "```" && in_json {
                blocks.push(current.join("\n"));
                in_json = false;
                continue;
            }

            if in_json {
                current.push(line);
            }
        }

        blocks
    }
}
