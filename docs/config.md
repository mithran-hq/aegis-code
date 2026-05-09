# Configuration

Aegis Code reads `$AEGIS_HOME/config.toml`, defaulting to
`~/.aegis/config.toml`. It keeps Aegis configuration separate from Codex
configuration so migration can be previewed and audited.

## Minimal Config

Create a minimal OpenAI-compatible setup:

```toml
model_provider = "openai"
model = "gpt-5.4"
```

Then authenticate with either a stored key or an environment variable:

```bash
printenv OPENAI_API_KEY | aegis login --with-api-key
# or
export OPENAI_API_KEY="..."
```

Run diagnostics after changing config:

```bash
aegis doctor
aegis doctor --json
```

Doctor reports provider selection, model selection, sandbox posture, Aegis
Engine alert paths, and context-pack status. It reports whether an environment
key is present, but it does not print secret values.

See [Sample configuration](example-config.md) for a complete starter file.

## Provider Selection

Provider selection precedence is:

1. CLI or session `model_provider` override.
2. Active profile `model_provider`.
3. Global `model_provider`.
4. The first active context-pack provider default in `context_pack_paths` order.
5. The built-in `openai` provider.

Context-pack provider defaults must use concrete provider ids such as `openai`,
`anthropic`, `ollama`, `lmstudio`, or a custom key from `model_providers`.
Family aliases such as `local` are not provider ids.

## Importing Codex Config

To migrate safe settings from an existing Codex config, preview the import:

```bash
aegis config import-codex
```

The command reads `~/.codex/config.toml` by default and prints the settings it
would add to `~/.aegis/config.toml`. It does not write anything unless
`--apply` is provided:

```bash
aegis config import-codex --apply
```

Only limited preferences are imported: model selection, provider references,
sanitized custom provider definitions, profiles, sandbox and permission
preferences, UI/tool preferences, and similar non-secret settings. Literal
secrets are skipped, including bearer tokens, command-backed auth blocks, static
HTTP headers, query parameters, and secret-looking key paths. Provider
environment variable names such as `env_key` may be imported because they do not
contain the secret value.

Prompt text is skipped by default. Use `--include-prompts` to import literal
prompt strings after reviewing the preview. Prompt file paths are not imported.
Use `--from <PATH>`, `--to <PATH>`, or `--json` for scripted migrations and
tests.

See [Migration from Codex](migration-from-codex.md).

## Context Packs

Context packs are loaded only from explicit TOML paths configured in
`~/.aegis/config.toml`:

```toml
context_pack_paths = [
  "/Users/bruno/.aegis/context-packs/user-method.toml",
  "/Users/bruno/src/project/.aegis/project-policy.toml",
]
```

Only valid packs with `promotion.status = "promoted"` contribute
`guidance.text` to prompt assembly. Candidate, retired, unreadable, invalid, or
schema-incompatible packs are ignored fail-closed and reported by
`aegis doctor`.

Candidate and retired packs never affect active prompts. Learned behavior must
be inspected and explicitly promoted before it can affect a future session or an
explicit resume boundary.

Promoted packs can also suggest provider defaults with `[provider_defaults]`.
CLI and config choices always override context-pack policy, and `aegis doctor`
reports the selected provider, selected model, source of each selection, and the
provider policy entries that were applied or skipped.

## Anthropic Provider

Aegis Code can use Anthropic directly through the built-in native
`anthropic` provider:

```toml
model_provider = "anthropic"
model = "claude-sonnet-4-20250514"
```

Set `ANTHROPIC_API_KEY` in the environment before starting Aegis. See
[Native Anthropic Provider](anthropic-provider.md) for supported models and
current limits.

## OpenAI-Compatible Providers

Aegis Code preserves OpenAI Responses-compatible provider support for the
built-in `openai` provider and custom providers configured with
`wire_api = "responses"`. See
[OpenAI-Compatible Providers](openai-compatible-provider.md) for built-in
OpenAI auth, custom provider TOML, env vars, streaming behavior, and
`aegis doctor` diagnostics.

## Local OSS Providers

Aegis Code also preserves local OSS model workflows for Ollama and LM Studio.
See [Local OSS Providers](local-oss-providers.md) for `--oss`,
`--local-provider`, `oss_provider`, endpoint env vars, default models,
readiness checks, limitations, and `aegis doctor` troubleshooting.

## Sandbox Policy

Runtime sandbox settings are inherited from the Codex configuration model, while
Aegis adds a managed allow-list policy named `allowed_sandbox_modes`. Supported
values are `read-only`, `workspace-write`, `danger-full-access`, and
`external-sandbox`.

```toml
allowed_sandbox_modes = ["read-only", "workspace-write"]
```

The allow-list is not an ordered minimum. If the active mode is missing from the
list, protected risky workflows are blocked. See [Sandbox policy](sandbox.md).

## Aegis Engine

Aegis Engine event emission is enabled by default and writes local JSONL to
`$AEGIS_HOME/aegis-engine/events.jsonl`.

```toml
[aegis_engine]
enabled = true
failure_mode = "best-effort"
buffer_capacity = 256
```

Set `failure_mode = "require"` only when event emission must be available for
the workflow. Optional mirroring to an engine daemon or pipe is documented in
[Aegis Engine](aegis-engine.md).

## Aegis Agent Runtime

The Aegis Agent Runtime adapter is optional and disabled by default:

```toml
[features.aegis_agent_runtime]
enabled = true
command = ["aegis-agent-runtime", "stdio"]
failure_mode = "fallback"
```

`failure_mode = "fallback"` keeps native execution as the startup fallback.
`failure_mode = "require"` makes runtime startup failures fatal. See
[Aegis Agent Runtime](aegis-agent-runtime.md).
