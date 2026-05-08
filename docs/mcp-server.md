# Aegis MCP Server

Aegis Code can run as an MCP server over stdio:

```bash
aegis mcp-server
```

The server keeps the inherited Codex tools:

- `codex`
- `codex-reply`

It also exposes Aegis advisory tools:

- `aegis_status`
- `aegis_check`
- `aegis_evidence`
- `aegis_review`
- `aegis_context_pack_list`
- `aegis_context_pack_inspect`
- `aegis_policy_explain`
- `aegis_issue_validate`
- `aegis_doctor`

These Aegis tools are read-only. They explain status, evidence, review findings,
context-pack diagnostics, policy decisions, issue-train validity, and doctor
state, but they do not promote context packs, mutate policy, close issues, run
GitHub commands, or change repository files.

## Client Setup

For MCP clients that accept JSON server definitions, configure Aegis as a stdio
server. Use the installed `aegis` binary on `PATH`:

```json
{
  "mcpServers": {
    "aegis-code": {
      "command": "aegis",
      "args": ["mcp-server"]
    }
  }
}
```

If the client needs an absolute binary path, replace `aegis` with the full path
reported by:

```bash
command -v aegis
```

The same stdio command shape applies to Codex-compatible and Claude-compatible
MCP clients. Store client-specific JSON in the location required by that client.

## Tool Inputs

The Aegis tools publish JSON schemas through `tools/list` and return structured
JSON through `structuredContent`.

Method-state tools accept exactly one of:

```json
{
  "methodState": {}
}
```

or:

```json
{
  "methodStatePath": "/absolute/path/to/method-state.json"
}
```

`aegis_issue_validate` accepts a supplied parent issue snapshot and child issue
snapshots. It intentionally does not fetch GitHub itself, so MCP clients do not
need GitHub credentials for validation.

`aegis_context_pack_inspect` requires a context-pack path and does not expose
guidance text. `includeGuidance` is rejected by the MCP advisory surface.

`aegis_policy_explain` accepts either a command subject:

```json
{
  "subject": {
    "type": "command",
    "command": ["gh", "pr", "merge", "123"]
  }
}
```

or a filesystem-write subject:

```json
{
  "subject": {
    "type": "filesystem_write",
    "paths": ["/repo/src/lib.rs"],
    "changeCount": 1
  }
}
```

## Redaction

MCP outputs are redacted before they leave the server. Evidence commands and
output summaries use the same redaction helpers as method-state receipts.
Context-pack guidance text is not included. Doctor output reports whether an
environment key is present, but it does not include environment values.
