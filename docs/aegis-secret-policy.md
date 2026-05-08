# Aegis Secret Policy Contract

Aegis Code uses this contract when it asks Aegis Secret or another compatible
broker to evaluate a risky local command before execution. Version 1 is a JSON
contract carried by typed Rust protocol structs in
`codex_protocol::aegis_secret_policy`.

The contract is intentionally context-only. It does not contain raw secrets, full
prompts, full conversation history, environment maps, or command output. Aegis
Code sends the smallest useful summary of the command, task, method state,
sandbox posture, risk reason, and expected evidence.

## Versioning

Every request and response has `contract_version = 1`. Brokers must reject
unsupported versions with a denial or explain-only response rather than guessing
at field meaning.

## Request

```json
{
  "contract_version": 1,
  "command": {
    "command_name": "gh",
    "argv": ["gh", "issue", "close", "15"],
    "cwd": "/repo",
    "argv_redacted": false
  },
  "task": {
    "repository": "mithran-hq/aegis-code",
    "branch": "main",
    "commit": "abc123",
    "linked_issue": {
      "provider": "git_hub",
      "repository": "mithran-hq/aegis-code",
      "number": 15
    },
    "issue_title": "Task: Define sensitive command policy contract",
    "goal_summary": "Define the broker policy contract"
  },
  "method_state": {
    "available": true,
    "schema_version": 1,
    "status": "incomplete",
    "intent_summary": "Define policy broker inputs",
    "claim_ids": ["claim:contract"],
    "open_falsifier_ids": ["falsifier:ambiguous-verdict"],
    "required_evidence_ids": ["evidence:protocol-tests"],
    "gate_ids": ["gate:local-ci"]
  },
  "sandbox": {
    "sandbox_mode": "none",
    "permission_profile": "danger-full-access",
    "sandbox_permissions": "require_escalated",
    "network_policy": "enabled",
    "remote_environment": false
  },
  "risk": {
    "category": "repository_mutation",
    "summary": "GitHub issue mutation",
    "matched_policy_ids": ["policy:github-mutation"]
  },
  "expected_evidence": [
    {
      "id": "evidence:issue-closure",
      "summary": "Issue closure links to landed work",
      "required": true
    }
  ],
  "redactions": []
}
```

`command.argv` is the redacted argv that the broker may inspect. If Aegis Code
removes or masks values, it sets `argv_redacted = true`, adds a short
`argv_redaction_summary`, and records one or more redaction rules.

`task` identifies the repository and linked issue scope when available. It must
not include full issue bodies, comments, private prompt text, or unrelated
conversation history.

`method_state` is a summary of the durable method record. It carries status and
IDs that let the broker tell whether the command is inside a scoped task with
evidence expectations. It is not the full persisted method state.

`sandbox` describes the current execution posture as observed by Aegis Code.
Brokers should treat it as policy input, not as a guarantee that Aegis Secret
itself enforced that sandbox.

## Response

```json
{
  "contract_version": 1,
  "verdict": "require_confirmation",
  "rationale": "The command mutates GitHub issue state and needs explicit user confirmation.",
  "user_message": "Aegis Secret requires confirmation before closing issue #15.",
  "confirmation_prompt": "Close mithran-hq/aegis-code#15 after verifying the landed commit?",
  "evidence_requirements": [
    {
      "id": "evidence:issue-closure",
      "summary": "Record issue closure evidence",
      "required": true
    }
  ],
  "redactions": [
    {
      "field_path": "command.argv[3]",
      "reason": "example redaction",
      "replacement": "<redacted>"
    }
  ]
}
```

Verdicts are:

- `allow`: Aegis Code may continue with the mediated command.
- `deny`: Aegis Code must not run the command.
- `require_confirmation`: Aegis Code must obtain explicit user confirmation
  before continuing.
- `explain_only`: The broker provides guidance without authorizing or blocking;
  later preflight policy decides how to apply it.

`rationale` is broker-facing and should be specific enough for audit. The
optional `user_message` and `confirmation_prompt` are safe to show to a user.

## Redaction Rules

Redaction rules identify fields that were masked or that the broker wants Aegis
Code to keep masked in downstream events. `field_path` uses dotted JSON field
paths with optional array indexes, such as `command.argv[3]`. `replacement`
defaults to an implementation-defined placeholder when omitted.
