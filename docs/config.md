# Configuration

For basic configuration instructions, see [this documentation](https://developers.openai.com/codex/config-basic).

For advanced configuration instructions, see [this documentation](https://developers.openai.com/codex/config-advanced).

For a full configuration reference, see [this documentation](https://developers.openai.com/codex/config-reference).

## Importing Codex config

Aegis Code uses `$AEGIS_HOME/config.toml`, defaulting to
`~/.aegis/config.toml`. It does not write to `~/.codex/config.toml` during
normal startup.

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

## Context packs

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
schema-incompatible packs are ignored fail-closed and reported by `aegis doctor`.

Promoted packs can also suggest provider defaults with `[provider_defaults]`.
Provider selection precedence is: CLI/session `model_provider` override, active
profile `model_provider`, global `model_provider`, the first available active
context-pack provider default in `context_pack_paths` order, then the built-in
`openai` provider. Context-pack provider defaults must use concrete provider ids
such as `openai`, `anthropic`, `ollama`, `lmstudio`, or a custom
`model_providers` key. CLI and config choices always override context-pack
policy, and `aegis doctor` reports the selected provider, selected model, source
of each selection, and the provider policy entries that were applied or skipped.

## Anthropic provider

Aegis Code can use Anthropic directly through the built-in native
`anthropic` provider:

```toml
model_provider = "anthropic"
model = "claude-sonnet-4-20250514"
```

Set `ANTHROPIC_API_KEY` in the environment before starting Aegis. See
[Native Anthropic Provider](anthropic-provider.md) for supported models and
current limits.

## OpenAI-compatible providers

Aegis Code preserves OpenAI Responses-compatible provider support for the
built-in `openai` provider and custom providers configured with
`wire_api = "responses"`. See
[OpenAI-Compatible Providers](openai-compatible-provider.md) for built-in
OpenAI auth, custom provider TOML, env vars, streaming behavior, and
`aegis doctor` diagnostics.

## Local OSS providers

Aegis Code also preserves local OSS model workflows for Ollama and LM Studio.
See [Local OSS Providers](local-oss-providers.md) for `--oss`,
`--local-provider`, `oss_provider`, endpoint env vars, default models,
readiness checks, limitations, and `aegis doctor` troubleshooting.

## Commit attribution

Codex can add a [git trailer](https://git-scm.com/docs/git-interpret-trailers) to
generated commit messages so commits make Codex's involvement explicit. This
behavior is gated by the `codex_git_commit` feature flag; the top-level
`commit_attribution` setting is only used when that feature is enabled.

Add the following to `~/.aegis/config.toml`:

```toml
commit_attribution = "Codex <noreply@openai.com>"

[features]
codex_git_commit = true
```

When enabled, Codex appends a `Co-authored-by:` trailer using the configured
attribution value. If `commit_attribution` is omitted, Codex uses
`Codex <noreply@openai.com>`. Set `commit_attribution = ""` to disable the
trailer while leaving the feature flag enabled.
