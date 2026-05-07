# ADR 0001: Aegis Code Product Boundary

Status: Accepted

## Context

The original planning name was Bruno Gate. That created confusion with Bruno as
a person and also described a narrower validator CLI. The product direction is
now a Codex-derived coding harness that wires the method into the agent loop.

The Aegis suite already has separate projects for secrets, event intelligence,
and runtime execution. Aegis Code must compose with them without becoming all of
them.

## Decision

Name the project Aegis Code and expose the binary as `aegis`.

Aegis Code owns the coding harness: method state, prompt/context assembly,
evidence receipts, tool-call preflights, sandbox posture, provider routing,
session resume validity, and user-facing coding workflows.

Aegis Secret remains the authority broker for sensitive local commands and
secrets. Aegis Engine remains asynchronous intelligence for events and context
pack learning. Aegis Agent Runtime remains the optional execution substrate.

## Non-goals

- Aegis Code will not be a standalone checker or validator CLI in v1.
- Aegis Code will not own secret storage, command authority policy decisions,
  event intelligence, context-pack learning, or the shared execution runtime.
- Aegis Code will not replace the sibling Aegis projects; it will integrate with
  them through explicit contracts.

## Consequences

- The first implementation task imports upstream Codex source rather than
  building a new agent harness from scratch.
- Prompt-only control is treated as advisory. Durable control lives in runtime
  state, receipts, policy, and CI.
- Learned prompt changes are compiled into context packs and promoted before
  future sessions load them.
- The old `bruno-gate` repository is superseded after `aegis` has its issue
  train and bootstrap commit.
