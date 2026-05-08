# Configuration

For basic configuration instructions, see [this documentation](https://developers.openai.com/codex/config-basic).

For advanced configuration instructions, see [this documentation](https://developers.openai.com/codex/config-advanced).

For a full configuration reference, see [this documentation](https://developers.openai.com/codex/config-reference).

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
