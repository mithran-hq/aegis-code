# ADR 0001: Aegis Code Product Boundary

Status: Superseded by `mithran-hq/aegis#1`

## Context

The original planning name was Bruno Gate. That created confusion with Bruno as
a person and also described a narrower validator CLI. The product direction was
a Codex-derived coding harness that wired the method into the agent loop.

The Aegis suite already had separate projects for secrets, event intelligence,
and runtime execution. Aegis Code needed to compose with them without becoming
all of them.

This decision was superseded by D42: the native Aegis product in
`mithran-hq/aegis` owns local supervision, mutation authority, evidence, daemon
behavior, and packaging. Aegis Code is retained only as a Codex adapter fixture
source until archive.

## Decision

Name the project Aegis Code and expose the binary as `aegis`.

Aegis Code owned the historical coding harness plan: method state,
prompt/context assembly, evidence receipts, tool-call preflights, sandbox
posture, provider routing, session resume validity, and user-facing coding
workflows.

Aegis Secret remained the authority broker for sensitive local commands and
secrets. Aegis Engine remained asynchronous intelligence for events and context
pack learning. Aegis Agent Runtime remained the optional execution substrate.

## Non-goals

- Aegis Code would not be a standalone checker or validator CLI in v1.
- Aegis Code would not own secret storage, command authority policy decisions,
  event intelligence, context-pack learning, or the shared execution runtime.
- Aegis Code would not replace the sibling Aegis projects; it would integrate
  with them through explicit contracts.

## Consequences

- The first implementation task imported upstream Codex source rather than
  building a new agent harness from scratch.
- Prompt-only control was treated as advisory. Durable control lived in runtime
  state, receipts, policy, and CI.
- Learned prompt changes were compiled into context packs and promoted before
  future sessions load them.
- The old `bruno-gate` repository was superseded after `aegis` had its issue
  train and bootstrap commit.
