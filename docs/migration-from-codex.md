# Migration From Codex

Aegis Code is derived from Codex, but it keeps its own home directory, config,
method records, Aegis Engine logs, context packs, and policy posture.

## Home Directory

Codex uses `~/.codex`. Aegis Code uses `$AEGIS_HOME`, defaulting to:

```text
~/.aegis
```

Aegis Code does not write to `~/.codex/config.toml` during normal startup.

## Import Safe Settings

Preview a Codex config import:

```bash
aegis config import-codex
```

Apply it after reviewing the preview:

```bash
aegis config import-codex --apply
```

Use explicit paths for scripted migrations:

```bash
aegis config import-codex \
  --from ~/.codex/config.toml \
  --to ~/.aegis/config.toml \
  --json
```

## What Imports

The importer can copy safe preferences such as model selection, provider
references, sanitized custom provider definitions, profiles, sandbox and
permission preferences, and UI or tool preferences.

Provider environment variable names such as `env_key` may be imported because
they are references, not secret values.

## What Does Not Import

The importer skips literal secrets, bearer tokens, command-backed auth blocks,
static HTTP headers, query parameters, and secret-looking key paths.

Prompt text is skipped by default. Use `--include-prompts` only after reviewing
the preview. Prompt file paths are not imported.

## Behavior Changes To Expect

Aegis Code adds controls that Codex did not require in the same way:

- Child GitHub task issues are treated as implementation scope.
- Method state records intent, claims, assumptions, falsifiers, evidence,
  gates, review, and closure.
- Evidence receipts are durable and redacted.
- Sensitive command mediation can require confirmation or deny commands.
- Aegis Engine emits local runtime events and can ingest alerts.
- Learned prompt behavior must become a promoted context pack before it affects
  a future session.

These controls are part of the Aegis Code product boundary. They are not live
autonomous prompt mutation.

## Validation

After migration, run:

```bash
aegis doctor
```

Confirm the selected provider, model, sandbox posture, Aegis Engine paths, and
context-pack status before starting real work.
