# Aegis Code Documentation

> **Status:** Superseded. `mithran-hq/aegis` is now the Aegis OSS native
> product control plane. This repository is retained temporarily for Codex
> adapter fixture extraction before archive.

These pages mostly preserve historical Aegis Code behavior and design evidence.
The current extraction artifact is
[Codex adapter fixtures](codex-adapter-fixtures.md).

## First Run

- [Install and build](install.md) explains supported operating systems, source
  builds, release-artifact expectations, and local development checks.
- [Getting started](getting-started.md) gives the shortest working path from a
  checkout to `aegis doctor`, an interactive prompt, and `aegis exec`.
- [Authentication](authentication.md) covers OpenAI login, API-key auth, and
  provider-specific environment variables.
- [Sample configuration](example-config.md) gives a safe starter
  `~/.aegis/config.toml`.

## Daily Workflow

- [Method workflow](method-workflow.md) explains issue-train validation, method
  state, evidence receipts, adversarial review, PR readiness, and closure.
- [Non-interactive mode](exec.md) documents `aegis exec` and method-state
  artifacts for CI or scripts.
- [Configuration](config.md) covers config precedence, context packs, providers,
  sandbox policy, Aegis Engine, Aegis Agent Runtime, and Codex migration.
- [Troubleshooting](troubleshooting.md) maps common first-run and method issues
  to concrete diagnostics.

## Aegis Integrations

- [Aegis Secret policy](aegis-secret-policy.md) defines how sensitive local
  commands are summarized for a broker without sending secrets or full prompts.
- [Aegis Engine](aegis-engine.md) explains event logs, alert ingestion,
  candidate guidance, and the boundary between warnings and prompt changes.
- [Aegis runtime events](aegis-runtime-events.md) is the detailed JSON event
  contract.
- [Aegis Agent Runtime](aegis-agent-runtime.md) documents the optional stdio
  runtime adapter.
- [Context packs](context-packs.md) defines explicit context-pack loading,
  promotion, retirement, rollback, and lineage.

## Providers And Policy

- [OpenAI-compatible providers](openai-compatible-provider.md) covers OpenAI,
  Azure-shaped Responses endpoints, and custom Responses-compatible providers.
- [Native Anthropic provider](anthropic-provider.md) covers the built-in
  Anthropic Messages provider.
- [Local OSS providers](local-oss-providers.md) covers Ollama and LM Studio.
- [Sandbox policy](sandbox.md) covers reported sandbox posture and managed
  allow-list policy.
- [MCP server](mcp-server.md) documents the stdio MCP surface and read-only
  Aegis advisory tools.

## Migration And Project Context

- [Migration from Codex](migration-from-codex.md) explains what can be imported,
  what stays separate, and what behavior intentionally changes.
- [Architecture](ARCHITECTURE.md) summarizes the superseded harness boundary and
  current handoff to `mithran-hq/aegis`.
- [Distribution](DISTRIBUTION.md) preserves historical release and installer
  expectations.
- [Upstream sync](UPSTREAM.md) documents the Codex fork posture.
