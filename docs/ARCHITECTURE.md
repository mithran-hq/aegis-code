# Architecture

> **Status:** Superseded by `mithran-hq/aegis#1`.
>
> This document records the historical Aegis Code harness architecture. Current
> Aegis OSS supervision, mutation authority, evidence, daemon behavior, and
> packaging belong in `mithran-hq/aegis`. This repo now supplies only Codex
> adapter fixtures before archive.

Aegis Code was a coding agent harness derived from Codex. The historical design
explored control surfaces that prompt-only methods could not enforce: method
state, evidence receipts, sensitive tool mediation, sandbox posture, provider
routing, session resume validity, and asynchronous learning from runtime events.

## Historical Product Boundary

| Layer               | Responsibility                                                         |
| ------------------- | ---------------------------------------------------------------------- |
| Aegis               | Native app, daemon, adapters, evidence, mutation, package              |
| Aegis Code          | Temporary Codex adapter fixture source                                 |
| Aegis Secret        | Authority decisions for sensitive local commands and secrets           |
| Aegis Engine        | Asynchronous event triage, drift intelligence, candidate context packs |
| Aegis Agent Runtime | Optional execution, sandbox, session, and tool substrate               |

The Codex-derived crate and module attachment points for v1 implementation are
preserved as historical evidence in
[CODEX_ARCHITECTURE_MAP.md](CODEX_ARCHITECTURE_MAP.md).

## Architecture Decisions

Historical v1 architecture decisions are recorded in ADRs:

- [ADR 0001: Aegis Code Product Boundary](adr/0001-aegis-code-product-boundary.md)
  names Aegis Code, chooses the harness posture, and separates sibling Aegis
  project responsibilities.
- [ADR 0002: Context Pack Promotion](adr/0002-context-pack-promotion.md)
  requires learned prompt changes to become promoted context packs before they
  affect future sessions.
- [ADR 0003: Upstream Codex Fork Posture](adr/0003-upstream-codex-fork-posture.md)
  chooses upstream Codex as the implementation base and preserves attribution.
  The operational sync workflow is documented in [UPSTREAM.md](UPSTREAM.md).
- [ADR 0004: Provider Strategy](adr/0004-provider-strategy.md) preserves
  OpenAI-compatible behavior first, makes native Anthropic a first-class track,
  and keeps provider routing as a later policy layer.

## Historical Method State

The method record is:

```text
Intent -> Claims -> Assumptions -> Falsifiers -> Evidence -> Gates -> Review -> Closure
```

The record must survive session resume. A completed statement only counts when
its falsifiers and evidence remain inspectable after the agent's short-term
memory has gone.

## Historical Prompt And Context Layers

Prompt assembly should be deterministic:

1. built-in Aegis Code base guidance
2. user context pack
3. project context pack
4. promoted learned context pack
5. current task facts

Unpromoted candidate packs are visible to humans and tools, but they do not
change active behavior.

The v1 context-pack TOML contract is defined in
[Context Packs](context-packs.md).

## Historical Aegis Engine Loop

The v1 learning loop is asynchronous:

```text
aegis -> structured events -> aegis-daemon -> alerts/intelligence
  -> candidate context pack -> promotion gate -> future session
```

Runtime events can warn the current session, but persistent instruction changes
apply only after promotion and only at a new session or resume boundary.

The v1 event contract is defined in
[Aegis Runtime Events](aegis-runtime-events.md).

## Historical Sensitive Tools

The historical harness design mediated configured sensitive commands through
Aegis Secret when available. The request included argv, cwd, repo, task scope,
current method state, and the reason the command was sensitive.

The v1 broker request and response contract is defined in
[Aegis Secret Policy Contract](aegis-secret-policy.md).

## Aegis Agent Runtime

The optional runtime subprocess adapter is documented in
[Aegis Agent Runtime Adapter](aegis-agent-runtime.md). It is feature/config
gated and preserves native execution as the default.

## Provider Strategy

The first provider contract should preserve upstream OpenAI-compatible behavior.
Native Anthropic support is a first-class track because provider-native prompt
caching and tool semantics matter for production cost and correctness.
The current provider seams and native Anthropic gaps are documented in
[Provider Abstraction Review](provider-abstraction-review.md).
