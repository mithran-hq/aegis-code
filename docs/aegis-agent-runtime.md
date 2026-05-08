# Aegis Agent Runtime Adapter

Aegis Code can optionally start an Aegis Agent Runtime subprocess and speak to
it over stdio. The integration is disabled by default and remains an
under-development boundary until the runtime event schema is finalized.

Enable the adapter with:

```toml
[features.aegis_agent_runtime]
enabled = true
command = ["aegis-agent-runtime", "stdio"]
failure_mode = "fallback"
```

`failure_mode = "fallback"` keeps native Codex execution as the backup when the
runtime cannot be spawned or initialized before a task is dispatched.
`failure_mode = "require"` makes runtime startup failures fatal. After a task is
sent to the runtime, protocol failures are surfaced as errors instead of falling
back, so Aegis Code does not duplicate side effects.

## v0 Stdio Contract

The v0 adapter uses newline-delimited JSON-RPC messages on stdin/stdout. It
reuses the existing app-server protocol envelopes for initialization,
thread/session requests, task starts, cancellation, server requests, request
resolution, notifications, and results. This keeps the Aegis Code integration
thin while the durable Aegis runtime event schema is defined separately.

The adapter also accepts an interim `aegis/runtime/checkpoint` notification:

```json
{"method":"aegis/runtime/checkpoint","params":{"threadId":"...","checkpointId":"...","label":"..."}}
```

For now, checkpoints are surfaced as user-visible warning notifications. The
stable checkpoint/event shape belongs to the runtime event schema work.
