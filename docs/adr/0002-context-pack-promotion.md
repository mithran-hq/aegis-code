# ADR 0002: Context Pack Promotion

Status: Accepted

## Context

Aegis Code needs prompt and context behavior that remains auditable across
session resume. Runtime observations can identify better guidance, but allowing
the active prompt to rewrite itself during a session would make behavior hard to
inspect and hard to falsify.

The architecture already separates built-in guidance, user context, project
context, learned context, and current task facts. The missing decision is how
learned behavior becomes active.

## Decision

Prompt assembly will be deterministic. V1 loads context in this order:

1. built-in Aegis Code base guidance
2. user context pack
3. project context pack
4. promoted learned context pack
5. current task facts

Learned prompt changes must become promoted context packs before they affect a
future session. Candidate learned packs can be generated, reviewed, and
inspected, but they do not change active behavior until promotion.

Promotion affects future sessions or explicit resume boundaries. Runtime events
may warn the current session, but they do not mutate the active prompt.

## Non-goals

- V1 will not support live self-modifying prompts.
- This ADR does not define the context-pack schema, loader, validator,
  promotion command, or rollback command.
- This ADR does not decide the storage location or distribution mechanism for
  context packs.

## Consequences

- Learned behavior stays reviewable before it changes future agent behavior.
- Session transcripts and evidence receipts can be interpreted against a stable
  prompt/context stack.
- Context-pack schema and promotion work can be implemented as later child
  issues without reopening the product decision.
