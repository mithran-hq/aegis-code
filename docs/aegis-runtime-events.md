# Aegis Runtime Events

Aegis Code emits runtime intelligence as newline-delimited Aegis `SafetyEvent`
JSON. Version 1 deliberately uses the existing ingestion envelope so Aegis
Engine does not need a new top-level event type for Aegis Code.

Every event uses `source = "aegis-code"` and carries a compact, redacted
summary. Events must not include raw prompts, full conversation history, raw
environment maps, raw command output, or secret values.

## Envelope

The v1 envelope has these fields:

- `source`: always `aegis-code`.
- `summary`: one human-readable sentence.
- `category`: one of `method_gate`, `tool_call`, `tool_denial`, `evidence`,
  `resume`, `provider`, `sandbox`, `review`, or `runtime`.
- `severity_hint`: one of `info`, `low`, `medium`, `high`, or `critical`.
- `tags`: stable routing and aggregation strings.
- `context`: compact structured facts.
- `redactions`: field paths that were masked before emission.

## Method Gate

```json
{
  "source": "aegis-code",
  "summary": "Aegis preflight RequireConfirmation for exec_command",
  "category": "method_gate",
  "severity_hint": "medium",
  "tags": ["category:method_gate", "tool:exec_command", "verdict:require_confirmation", "risk:repository_mutation"],
  "context": {
    "call_id": "call-1",
    "turn_id": "turn-1",
    "tool_name": "exec_command",
    "verdict": "require_confirmation",
    "risk_category": "repository_mutation",
    "reason": "Repository mutation requires confirmation.",
    "required_evidence_ids": ["evidence:issue-closure"],
    "command": { "argv": ["gh", "issue", "close", "23"] }
  },
  "redactions": []
}
```

## Tool Call

```json
{
  "source": "aegis-code",
  "summary": "Aegis preflight Allow for apply_patch",
  "category": "tool_call",
  "severity_hint": "info",
  "tags": ["category:tool_call", "tool:apply_patch", "verdict:allow"],
  "context": {
    "call_id": "call-2",
    "turn_id": "turn-1",
    "tool_name": "apply_patch",
    "verdict": "allow",
    "reason": "Filesystem write is inside the current workspace.",
    "required_evidence_ids": [],
    "paths": ["/repo/codex-rs/protocol/src/aegis_safety_event.rs"]
  },
  "redactions": []
}
```

## Tool Denial

```json
{
  "source": "aegis-code",
  "summary": "Aegis preflight Block for exec_command",
  "category": "tool_denial",
  "severity_hint": "high",
  "tags": ["category:tool_denial", "tool:exec_command", "verdict:block", "risk:credential_access"],
  "context": {
    "call_id": "call-3",
    "turn_id": "turn-1",
    "tool_name": "exec_command",
    "verdict": "block",
    "risk_category": "credential_access",
    "reason": "Credential access is outside the linked task scope.",
    "required_evidence_ids": ["evidence:task-scope"],
    "command": { "argv": ["gh", "api", "--token", "<redacted>"] }
  },
  "redactions": [
    {
      "field_path": "context.command.argv[3]",
      "reason": "sensitive argv value",
      "replacement": "<redacted>"
    }
  ]
}
```

## Evidence

```json
{
  "source": "aegis-code",
  "summary": "test command completed with exit code 0: cargo test -p codex-protocol aegis_safety_event",
  "category": "evidence",
  "severity_hint": "info",
  "tags": ["category:evidence", "evidence:test", "requirement:evidence:protocol-tests"],
  "context": {
    "evidence_id": "evidence:test:call-4",
    "kind": "test",
    "requirement_ids": ["evidence:protocol-tests"],
    "claim_ids": ["claim:safety-event-contract"],
    "falsifier_ids": [],
    "captured_at_unix_seconds": 1778246400,
    "evidence_source": "harness exec_command",
    "receipt": {
      "command": ["cargo", "test", "-p", "codex-protocol", "aegis_safety_event"],
      "cwd": "/repo",
      "exit_status": { "exit_code": 0, "timed_out": false },
      "output_summary": "test result: ok",
      "artifacts": [],
      "session": { "thread_id": "thread-1", "provider": "openai", "model": "gpt-5.2" },
      "redaction_status": "not_needed"
    }
  },
  "redactions": []
}
```

## Resume

```json
{
  "source": "aegis-code",
  "summary": "Loaded persisted Aegis method state",
  "category": "resume",
  "severity_hint": "low",
  "tags": ["category:resume", "resume:stale", "resume_reason:branch_changed"],
  "context": {
    "status": "stale",
    "reasons": ["branch_changed"],
    "method_status": "incomplete",
    "linked_issue": {
      "provider": "git_hub",
      "repository": "mithran-hq/aegis-code",
      "number": 23
    }
  },
  "redactions": []
}
```

## Provider

```json
{
  "source": "aegis-code",
  "summary": "Selected model provider",
  "category": "provider",
  "severity_hint": "info",
  "tags": ["category:provider", "provider:selected"],
  "context": {
    "provider": "openai",
    "model": "gpt-5.2"
  },
  "redactions": []
}
```

## Sandbox

```json
{
  "source": "aegis-code",
  "summary": "Sandbox posture selected",
  "category": "sandbox",
  "severity_hint": "info",
  "tags": ["category:sandbox", "sandbox:posture"],
  "context": {
    "sandbox_mode": "workspace-write",
    "permission_profile": "on-request"
  },
  "redactions": []
}
```

## Review

```json
{
  "source": "aegis-code",
  "summary": "No blocking review findings",
  "category": "review",
  "severity_hint": "info",
  "tags": ["category:review", "review:info", "review_status:addressed"],
  "context": {
    "finding_id": "finding:review:turn-1:clean",
    "severity": "info",
    "status": "addressed",
    "claim_ids": [],
    "evidence_ids": ["evidence:review:turn-1"],
    "reviewed_at_unix_seconds": 1778246400,
    "reviewer": "aegis review"
  },
  "redactions": []
}
```

## Runtime

```json
{
  "source": "aegis-code",
  "summary": "Aegis Agent Runtime checkpoint",
  "category": "runtime",
  "severity_hint": "info",
  "tags": ["category:runtime", "runtime:checkpoint"],
  "context": {
    "thread_id": "thread-1",
    "checkpoint_id": "checkpoint-1",
    "label": "after tests"
  },
  "redactions": []
}
```

## Sink Configuration

Aegis Code starts the Aegis Engine sink by default. The durable baseline is a
local JSONL audit log at `$AEGIS_HOME/aegis-engine/events.jsonl`; users can opt
out with:

```toml
[aegis_engine]
enabled = false
```

The sink is asynchronous and bounded. By default, sink startup failures, write
failures, and queue overflow produce warnings but do not stop normal coding:

```toml
[aegis_engine]
buffer_capacity = 256
failure_mode = "best-effort"
```

Managed requirements can make emission mandatory for protected workflows:

```toml
[aegis_engine]
required = true
```

When required, Aegis Code forces the sink on and uses `failure_mode =
"require"`. Local JSONL remains the source of truth even when events are also
mirrored to Aegis Engine:

```toml
[aegis_engine]
mirror = "daemon-stdin"
daemon_command = ["aegis-engine", "ingest", "--stdin"]
```

Pipe mirroring is also supported for an existing writable pipe or file:

```toml
[aegis_engine]
mirror = "pipe"
pipe_path = "/tmp/aegis-engine-events.pipe"
```

## Alert Ingestion

Aegis Code also reads Aegis Engine headless alert JSONL from
`$AEGIS_HOME/aegis-engine/alerts.jsonl`. Alert ingestion is checkpoint-based:
sessions scan the file at startup and after Aegis runtime events are emitted.
Malformed alerts produce diagnostics instead of crashing the session, stale
alerts are ignored, and correlated suspicious or malicious alerts are surfaced
through the normal CLI/TUI warning channel.

Alert ingestion never mutates active prompt layers. Safe alerts are recorded as
observed, suspicious alerts mark method state as warned, and malicious alerts
mark method state as blocked. Candidate-pack input records for suspicious or
malicious alerts with guidance are appended to
`$AEGIS_HOME/aegis-engine/candidate-pack-inputs.jsonl`; a later learned-pack
compiler may consume those records, but they are not active context packs.

```jsonc
{
  "schema_version": 1,
  "alert_id": "alert-1",
  "severity": "suspicious",
  "action": "warn",
  "summary": "Tool call looks inconsistent with the linked task scope.",
  "created_at_unix_seconds": 1778246400,
  "source_event": {
    "event_id": "aegis-code:tool_call:call-1",
    "category": "tool_call",
    "turn_id": "turn-1",
    "call_id": "call-1"
  },
  "candidate_guidance": {
    "guidance": "Require issue-scope evidence before similar repository mutations.",
    "falsifiers": ["The command is read-only or explicitly requested by the task issue."]
  }
}
```

## Redaction Rules

Event context should contain identifiers, statuses, summaries, redacted argv,
path summaries, and short output summaries only. If Aegis Code masks a value, it
must add a redaction entry with the JSON field path and reason. Downstream
systems should treat `redactions` as audit metadata and should not attempt to
recover masked values.
