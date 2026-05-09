# Local OSS Providers

Aegis Code keeps local OSS model workflows available through the built-in
`ollama` and `lmstudio` providers. Both providers use a Responses-compatible
HTTP API, require no OpenAI authentication, and are selected with `--oss`.

## Quick Start

Run Ollama with its default local endpoint:

```bash
ollama serve
aegis --oss --local-provider ollama
```

Run LM Studio with its local server:

```bash
lms server start
aegis --oss --local-provider lmstudio
```

If `--local-provider` is omitted while using `--oss`, Aegis uses
`oss_provider` from the selected profile, then the global `oss_provider`, and
then prompts in the TUI.

## Configuration

Set a default provider in `~/.aegis/config.toml`:

```toml
oss_provider = "ollama"
```

Profiles can choose a different local provider:

```toml
[profiles.local-lmstudio]
oss_provider = "lmstudio"
```

Without `--oss`, local providers can still be selected through the normal
provider precedence chain: CLI/session `model_provider`, active profile
`model_provider`, global `model_provider`, promoted context-pack
`[provider_defaults]`, then `openai`. When the effective provider is `ollama` or
`lmstudio`, Aegis applies the same local readiness checks and default model even
if the provider came from config or a context pack.

The built-in local providers have these defaults:

| Provider   | Base URL                    | Default model        |
| ---------- | --------------------------- | -------------------- |
| `ollama`   | `http://localhost:11434/v1` | `gpt-oss:20b`        |
| `lmstudio` | `http://localhost:1234/v1`  | `openai/gpt-oss-20b` |

Override the local endpoint with Aegis environment variables:

```bash
AEGIS_OSS_PORT=11435 aegis --oss --local-provider ollama
AEGIS_OSS_BASE_URL=http://localhost:1234/v1 aegis --oss --local-provider lmstudio
```

`AEGIS_OSS_BASE_URL` takes precedence over `AEGIS_OSS_PORT`. Legacy
`CODEX_OSS_BASE_URL` and `CODEX_OSS_PORT` are still accepted as fallbacks, but
the Aegis variables win when both are set.

## Readiness Checks

For Ollama, Aegis probes `/v1/models`, checks the native `/api/version`
endpoint, requires Ollama `0.13.4` or newer for Responses support, lists models
through `/api/tags`, and pulls the requested model through `/api/pull` when it
is missing.

For LM Studio, Aegis probes `/models`, lists available model ids, downloads a
missing default model with `lms get --yes`, and sends a small `/responses`
request to load the selected model.

## Diagnostics

Use `aegis doctor` with the same root provider flags to verify which provider
is active:

```bash
aegis --oss --local-provider ollama doctor
aegis --oss --local-provider lmstudio doctor --json
```

The report shows the selected provider id, display name, model, wire API, base
URL, OpenAI auth requirement, websocket support, and env-key status. Local
providers should report `wire API: responses`, `OpenAI auth: false`,
`websockets: false`, and `env key: none`.

## Limitations

Local OSS providers do not use Responses WebSockets and do not use OpenAI login
or API-key auth. Hosted OpenAI-only features, service tiers, and remote tools
may not be available through local servers. If the server is not running, Aegis
returns startup instructions for the selected provider. If Ollama is too old,
the error names the detected version and the minimum supported version.
