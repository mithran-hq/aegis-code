use pretty_assertions::assert_eq;
use rmcp::model::RequestId;
use serde_json::json;
use tempfile::TempDir;

use mcp_test_support::McpProcess;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aegis_tools_are_listed_by_server() -> anyhow::Result<()> {
    let codex_home = TempDir::new()?;
    let mut mcp = McpProcess::new(codex_home.path()).await?;
    mcp.initialize().await?;

    let request_id = mcp.send_list_tools().await?;
    let response = mcp
        .read_stream_until_response_message(RequestId::Number(request_id))
        .await?;
    let tools = response.result["tools"]
        .as_array()
        .expect("tools/list returns tools array");
    let names = tools
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect::<Vec<_>>();

    assert!(names.contains(&"codex"));
    assert!(names.contains(&"codex-reply"));
    assert!(names.contains(&"aegis_status"));
    assert!(names.contains(&"aegis_check"));
    assert!(names.contains(&"aegis_evidence"));
    assert!(names.contains(&"aegis_review"));
    assert!(names.contains(&"aegis_context_pack_list"));
    assert!(names.contains(&"aegis_context_pack_inspect"));
    assert!(names.contains(&"aegis_policy_explain"));
    assert!(names.contains(&"aegis_issue_validate"));
    assert!(names.contains(&"aegis_doctor"));
    for tool in tools {
        assert!(tool.get("inputSchema").is_some(), "{tool:?}");
        assert!(tool.get("outputSchema").is_some(), "{tool:?}");
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aegis_status_returns_structured_advisory_output() -> anyhow::Result<()> {
    let codex_home = TempDir::new()?;
    let mut mcp = McpProcess::new(codex_home.path()).await?;
    mcp.initialize().await?;

    let request_id = mcp.send_tool_call("aegis_status", json!({})).await?;
    let response = mcp
        .read_stream_until_response_message(RequestId::Number(request_id))
        .await?;

    assert_eq!(response.result["isError"], false);
    assert_eq!(response.result["structuredContent"]["ok"], true);
    assert_eq!(response.result["structuredContent"]["advisoryOnly"], true);
    assert!(response.result["structuredContent"]["provider"]["id"].is_string());

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aegis_issue_validate_accepts_snapshot_payloads() -> anyhow::Result<()> {
    let codex_home = TempDir::new()?;
    let mut mcp = McpProcess::new(codex_home.path()).await?;
    mcp.initialize().await?;

    let request_id = mcp
        .send_tool_call(
            "aegis_issue_validate",
            json!({
                "parent": {
                    "number": 1,
                    "title": "Plan: MCP",
                    "state": "open",
                    "labels": ["aegis-code:plan"],
                    "body": "## Objective\n\nCoordinate MCP work.\n\n## Child Issues\n\n- [ ] #2 Task: MCP server\n\n## Evidence Required For Closure\n\nReconcile child state.\n"
                },
                "children": [{
                    "number": 2,
                    "title": "Task: MCP server",
                    "state": "open",
                    "labels": ["aegis-code:task"],
                    "body": "## Objective\n\nExpose advisory MCP tools for Aegis clients.\n\n## Scope\n\nImplement the server surface.\n\n## Acceptance Criteria\n\n- Tools list with typed schemas.\n\n## Falsifiers\n\n- Tool output leaks secrets.\n\n## Dependencies\n\nNone.\n"
                }]
            }),
        )
        .await?;
    let response = mcp
        .read_stream_until_response_message(RequestId::Number(request_id))
        .await?;

    assert_eq!(response.result["isError"], false);
    assert_eq!(response.result["structuredContent"]["valid"], true);
    assert_eq!(response.result["structuredContent"]["childCount"], 1);

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aegis_evidence_redacts_sensitive_receipts() -> anyhow::Result<()> {
    let codex_home = TempDir::new()?;
    let mut mcp = McpProcess::new(codex_home.path()).await?;
    mcp.initialize().await?;

    let request_id = mcp
        .send_tool_call(
            "aegis_evidence",
            json!({
                "methodState": method_state_with_secret_receipt()
            }),
        )
        .await?;
    let response = mcp
        .read_stream_until_response_message(RequestId::Number(request_id))
        .await?;
    let response_text = serde_json::to_string(&response.result)?;

    assert_eq!(response.result["isError"], false);
    assert!(response_text.contains("<redacted>"));
    assert!(!response_text.contains("secret-token"));

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aegis_policy_explain_accepts_camel_case_filesystem_input() -> anyhow::Result<()> {
    let codex_home = TempDir::new()?;
    let mut mcp = McpProcess::new(codex_home.path()).await?;
    mcp.initialize().await?;

    let request_id = mcp
        .send_tool_call(
            "aegis_policy_explain",
            json!({
                "subject": {
                    "type": "filesystem_write",
                    "paths": ["/tmp/aegis-mcp-test.txt"],
                    "changeCount": 1
                }
            }),
        )
        .await?;
    let response = mcp
        .read_stream_until_response_message(RequestId::Number(request_id))
        .await?;

    assert_eq!(response.result["isError"], false);
    assert!(response.result["structuredContent"]["verdict"].is_string());
    assert_eq!(response.result["structuredContent"]["advisoryOnly"], true);

    Ok(())
}

fn method_state_with_secret_receipt() -> serde_json::Value {
    json!({
        "schema_version": 1,
        "intent": {
            "summary": "Test MCP evidence",
            "success_criteria": ["Evidence is redacted"]
        },
        "status": "closed",
        "claims": [],
        "assumptions": [],
        "falsifiers": [],
        "evidence_requirements": [{
            "id": "requirement:test",
            "summary": "Tests pass",
            "required": true,
            "commands": ["curl -H 'Authorization: bearer secret-token' https://example.test"],
            "claim_ids": [],
            "falsifier_ids": []
        }],
        "evidence": [{
            "id": "evidence:test",
            "summary": "Tests passed",
            "kind": "test",
            "requirement_ids": ["requirement:test"],
            "claim_ids": [],
            "falsifier_ids": [],
            "captured_at_unix_seconds": 1,
            "receipt": {
                "schema_version": 1,
                "command": ["curl", "-H", "Authorization: bearer secret-token"],
                "cwd": "/repo",
                "captured_at_unix_seconds": 1,
                "git_state": {
                    "status": "unavailable"
                },
                "exit_status": {
                    "exit_code": 0,
                    "timed_out": false
                },
                "output_summary": "Authorization: bearer secret-token passed",
                "artifacts": [],
                "session": {},
                "redaction_status": "not_needed"
            }
        }],
        "gates": [],
        "engine_alerts": [],
        "review_findings": [{
            "id": "finding:review",
            "summary": "Review completed",
            "severity": "info",
            "status": "addressed",
            "claim_ids": [],
            "evidence_ids": ["evidence:test"],
            "reviewed_at_unix_seconds": 1
        }],
        "closure": {
            "closed_at_unix_seconds": 2,
            "summary": "Done",
            "evidence_ids": ["evidence:test"],
            "review_finding_ids": ["finding:review"]
        },
        "resume_context": {
            "schema_version": 1
        },
        "provenance": {
            "created_at_unix_seconds": 1,
            "updated_at_unix_seconds": 2,
            "source": "agent"
        }
    })
}
