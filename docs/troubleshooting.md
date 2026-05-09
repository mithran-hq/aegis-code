# Troubleshooting

Start every diagnosis with:

```bash
aegis doctor
```

Doctor reports the selected provider, model, provider source, model source,
sandbox posture, Aegis Engine alert paths, and context-pack status without
printing secret values.

## `aegis` Is Not Found

If you built from source but `aegis` is not on `PATH`, install the CLI:

```bash
cargo install --path codex-rs/cli --locked
```

Then confirm:

```bash
command -v aegis
aegis --version
```

From inside `codex-rs`, use `cargo install --path cli --locked`.

## Provider Key Is Missing

Doctor reports the configured provider env key and whether it is set. For the
built-in OpenAI provider:

```bash
export OPENAI_API_KEY="..."
aegis doctor
```

For Anthropic:

```bash
export ANTHROPIC_API_KEY="..."
aegis --profile anthropic doctor
```

Do not store literal secrets in `~/.aegis/config.toml`. Use environment
variables, `aegis login --with-api-key`, or a brokered secret flow.

## Wrong Provider Or Model

Check provider selection order in [Configuration](config.md). Explicit CLI or
session overrides win, then profile config, global config, context-pack provider
defaults, and finally the built-in `openai` provider.

Run:

```bash
aegis doctor --json
```

Use the provider and model source fields to find which layer selected the value.

## Local OSS Provider Is Not Ready

For Ollama:

```bash
ollama serve
aegis --oss --local-provider ollama doctor
```

For LM Studio:

```bash
lms server start
aegis --oss --local-provider lmstudio doctor
```

See [Local OSS Providers](local-oss-providers.md) for default endpoints,
download behavior, and readiness checks.

## Context Pack Does Not Affect Prompts

Only configured, valid, promoted context packs contribute guidance:

```bash
aegis context-pack list
aegis context-pack inspect <pack-id-or-path> --show-guidance
```

Candidate, retired, unreadable, invalid, or schema-incompatible packs are
ignored fail-closed. Promotion affects future sessions or explicit resume
boundaries, not a prompt that was already assembled.

## Sensitive Command Was Denied

Aegis preflight and Aegis Secret mediation can deny risky commands when task
scope, sandbox policy, or required evidence is missing. Read the denial message
for the required evidence id, then gather the missing task-scope or
sandbox-policy evidence before retrying.

Use the least-privileged command that satisfies the task. For GitHub operations,
prefer the Aegis Secret wrapped `gh` path when available.

## Method Closure Fails

Closure requires successful receipts for required evidence and no unresolved
blocking review findings. Re-run the required verification command and ensure
the method-state artifact is updated:

```bash
aegis exec \
  --method-state method-state.json \
  --method-state-output artifacts/method-state.json \
  --json \
  "run the required verification"
```

Then validate readiness:

```bash
aegis pr-readiness validate --method-state artifacts/method-state.json
```

## Aegis Engine Alerts Keep Warning

Check the alerts section:

```bash
aegis doctor
```

Suspicious alerts mark method state as warned. Malicious alerts mark method
state as blocked. Alerts do not change prompts directly; repeated alert guidance
must become a reviewed and promoted context pack before it affects future
sessions.

## Migrated Codex Config Looks Incomplete

The Codex import skips literal secrets and prompt file paths by design. Preview
before applying:

```bash
aegis config import-codex
```

Apply only after reviewing the skipped fields:

```bash
aegis config import-codex --apply
```

See [Migration from Codex](migration-from-codex.md).
