# Aegis Engine Integration

Aegis Code emits compact runtime intelligence for Aegis Engine and can ingest
headless alerts produced from those events. This integration is audit and
guidance infrastructure; it does not silently mutate active prompts.

## Event Sink

Aegis Engine event emission is enabled by default. The durable local log is:

```text
$AEGIS_HOME/aegis-engine/events.jsonl
```

The default config is best-effort:

```toml
[aegis_engine]
enabled = true
failure_mode = "best-effort"
buffer_capacity = 256
```

Best-effort mode warns on sink startup failures, write failures, or queue
overflow, but it does not stop normal coding. Protected workflows can require
event emission:

```toml
[aegis_engine]
failure_mode = "require"
```

Managed requirements may also force the sink on and require emission even if a
local config tries to disable it.

## Mirroring

Local JSONL remains the source of truth. You can also mirror events to a daemon
over stdin:

```toml
[aegis_engine]
mirror = "daemon-stdin"
daemon_command = ["aegis-engine", "ingest", "--stdin"]
```

Or to an existing writable pipe or file:

```toml
[aegis_engine]
mirror = "pipe"
pipe_path = "/tmp/aegis-engine-events.pipe"
```

## Alerts

Aegis Code reads alert JSONL from:

```text
$AEGIS_HOME/aegis-engine/alerts.jsonl
```

Alert ingestion happens at startup and after Aegis runtime events are emitted.
Malformed alerts produce diagnostics instead of crashing the session, stale
alerts are ignored, and alerts must correlate with the current session or
thread before they are applied.

Alert effects are intentionally limited:

- Safe alerts are recorded as observed.
- Suspicious alerts warn and mark method state as warned.
- Malicious alerts warn and mark method state as blocked.
- Alerts with candidate guidance can append inactive candidate-pack inputs.

Alerts never change the active prompt directly.

## Candidate Context Packs

Suspicious or malicious alerts with guidance can write candidate inputs to:

```text
$AEGIS_HOME/aegis-engine/candidate-pack-inputs.jsonl
```

Compile repeated events and alert inputs into learned candidate packs:

```bash
aegis context-pack compile-candidates --min-support 2
```

Generated learned packs start as candidates. Inspect and promote them
explicitly before they affect a future session:

```bash
aegis context-pack list --kind learned --status candidate
aegis context-pack inspect learned:example --show-guidance
aegis context-pack promote learned:example \
  --evidence issue:41 \
  --reason "reviewed repeated alert guidance"
```

Use `--dry-run` on lifecycle commands when you need to preview changes.

## Diagnostics

Run:

```bash
aegis doctor
```

The Aegis Engine section reports whether alerts are enabled, the alerts path,
the candidate-inputs path, active warning and blocking counts, malformed alerts,
and stale alerts.

The detailed event and alert wire contract is in
[Aegis Runtime Events](aegis-runtime-events.md).
