//! Versioned Aegis Secret policy contract shared by Aegis Code and brokers.

use crate::method_state::MethodIssueRef;
use crate::method_state::MethodWorkStatus;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use ts_rs::TS;

pub const AEGIS_SECRET_POLICY_CONTRACT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct AegisSecretPolicyRequest {
    pub contract_version: u32,
    pub command: AegisSecretCommandContext,
    pub task: AegisSecretTaskContext,
    pub method_state: AegisSecretMethodStateSummary,
    pub sandbox: AegisSecretSandboxContext,
    pub risk: AegisSecretRiskReason,
    #[serde(default)]
    pub expected_evidence: Vec<AegisSecretExpectedEvidence>,
    #[serde(default)]
    pub redactions: Vec<AegisSecretRedactionRule>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct AegisSecretCommandContext {
    pub command_name: String,
    pub argv: Vec<String>,
    pub cwd: String,
    #[serde(default)]
    pub argv_redacted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub argv_redaction_summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct AegisSecretTaskContext {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub repository: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub commit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub linked_issue: Option<MethodIssueRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub issue_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub goal_summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct AegisSecretMethodStateSummary {
    pub available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub schema_version: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub status: Option<MethodWorkStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub intent_summary: Option<String>,
    #[serde(default)]
    pub claim_ids: Vec<String>,
    #[serde(default)]
    pub open_falsifier_ids: Vec<String>,
    #[serde(default)]
    pub required_evidence_ids: Vec<String>,
    #[serde(default)]
    pub gate_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct AegisSecretSandboxContext {
    pub sandbox_mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub permission_profile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub sandbox_permissions: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub network_policy: Option<String>,
    #[serde(default)]
    pub remote_environment: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct AegisSecretRiskReason {
    pub category: AegisSecretRiskCategory,
    pub summary: String,
    #[serde(default)]
    pub matched_policy_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum AegisSecretRiskCategory {
    SensitiveCommand,
    CloudMutation,
    RepositoryMutation,
    CredentialAccess,
    DestructiveAction,
    SandboxEscape,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct AegisSecretExpectedEvidence {
    pub id: String,
    pub summary: String,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct AegisSecretPolicyResponse {
    pub contract_version: u32,
    pub verdict: AegisSecretPolicyVerdict,
    pub rationale: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub user_message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub confirmation_prompt: Option<String>,
    #[serde(default)]
    pub evidence_requirements: Vec<AegisSecretExpectedEvidence>,
    #[serde(default)]
    pub redactions: Vec<AegisSecretRedactionRule>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum AegisSecretPolicyVerdict {
    Allow,
    Deny,
    RequireConfirmation,
    ExplainOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct AegisSecretRedactionRule {
    pub field_path: String,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub replacement: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    fn request() -> AegisSecretPolicyRequest {
        AegisSecretPolicyRequest {
            contract_version: AEGIS_SECRET_POLICY_CONTRACT_VERSION,
            command: AegisSecretCommandContext {
                command_name: "gh".to_string(),
                argv: vec![
                    "gh".to_string(),
                    "issue".to_string(),
                    "close".to_string(),
                    "15".to_string(),
                ],
                cwd: "/repo".to_string(),
                argv_redacted: false,
                argv_redaction_summary: None,
            },
            task: AegisSecretTaskContext {
                repository: Some("mithran-hq/aegis-code".to_string()),
                branch: Some("master".to_string()),
                commit: Some("abc123".to_string()),
                linked_issue: Some(MethodIssueRef {
                    provider: crate::method_state::MethodIssueProvider::GitHub,
                    repository: "mithran-hq/aegis-code".to_string(),
                    number: 15,
                }),
                issue_title: Some("Task: Define sensitive command policy contract".to_string()),
                goal_summary: Some("Define the broker policy contract".to_string()),
            },
            method_state: AegisSecretMethodStateSummary {
                available: true,
                schema_version: Some(crate::method_state::METHOD_STATE_SCHEMA_VERSION),
                status: Some(MethodWorkStatus::Incomplete),
                intent_summary: Some("Define policy broker inputs".to_string()),
                claim_ids: vec!["claim:contract".to_string()],
                open_falsifier_ids: vec!["falsifier:ambiguous-verdict".to_string()],
                required_evidence_ids: vec!["evidence:protocol-tests".to_string()],
                gate_ids: vec!["gate:local-ci".to_string()],
            },
            sandbox: AegisSecretSandboxContext {
                sandbox_mode: "none".to_string(),
                permission_profile: Some("danger-full-access".to_string()),
                sandbox_permissions: Some("require_escalated".to_string()),
                network_policy: Some("enabled".to_string()),
                remote_environment: false,
            },
            risk: AegisSecretRiskReason {
                category: AegisSecretRiskCategory::RepositoryMutation,
                summary: "GitHub issue mutation".to_string(),
                matched_policy_ids: vec!["policy:github-mutation".to_string()],
            },
            expected_evidence: vec![AegisSecretExpectedEvidence {
                id: "evidence:issue-closure".to_string(),
                summary: "Issue closure links to landed work".to_string(),
                required: true,
            }],
            redactions: Vec::new(),
        }
    }

    fn response(verdict: AegisSecretPolicyVerdict) -> AegisSecretPolicyResponse {
        AegisSecretPolicyResponse {
            contract_version: AEGIS_SECRET_POLICY_CONTRACT_VERSION,
            verdict,
            rationale: "Task context is present".to_string(),
            user_message: Some("Aegis Secret allowed gh.".to_string()),
            confirmation_prompt: None,
            evidence_requirements: vec![AegisSecretExpectedEvidence {
                id: "evidence:issue-closure".to_string(),
                summary: "Record issue closure evidence".to_string(),
                required: true,
            }],
            redactions: vec![AegisSecretRedactionRule {
                field_path: "command.argv[3]".to_string(),
                reason: "example redaction".to_string(),
                replacement: Some("<redacted>".to_string()),
            }],
        }
    }

    #[test]
    fn request_round_trips_json() {
        let request = request();
        let json = serde_json::to_string(&request).expect("serialize request");
        let round_tripped: AegisSecretPolicyRequest =
            serde_json::from_str(&json).expect("deserialize request");

        assert_eq!(round_tripped, request);
    }

    #[test]
    fn response_round_trips_all_verdicts() {
        for (verdict, expected_json) in [
            (AegisSecretPolicyVerdict::Allow, "allow"),
            (AegisSecretPolicyVerdict::Deny, "deny"),
            (
                AegisSecretPolicyVerdict::RequireConfirmation,
                "require_confirmation",
            ),
            (AegisSecretPolicyVerdict::ExplainOnly, "explain_only"),
        ] {
            let response = response(verdict);
            let value = serde_json::to_value(&response).expect("serialize response");
            assert_eq!(value["verdict"], expected_json);
            let round_tripped: AegisSecretPolicyResponse =
                serde_json::from_value(value).expect("deserialize response");
            assert_eq!(round_tripped, response);
        }
    }

    #[test]
    fn request_rejects_unknown_fields() {
        let mut value = serde_json::to_value(request()).expect("serialize request");
        value["full_prompt"] = json!("must not be accepted");

        assert!(serde_json::from_value::<AegisSecretPolicyRequest>(value).is_err());
    }

    #[test]
    fn documentation_examples_deserialize() {
        let docs = include_str!("../../../docs/aegis-secret-policy.md");
        let json_blocks = extract_json_blocks(docs);
        assert_eq!(
            json_blocks.len(),
            2,
            "expected request and response examples"
        );

        serde_json::from_str::<AegisSecretPolicyRequest>(&json_blocks[0])
            .expect("request example should deserialize");
        serde_json::from_str::<AegisSecretPolicyResponse>(&json_blocks[1])
            .expect("response example should deserialize");
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
