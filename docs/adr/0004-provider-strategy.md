# ADR 0004: Provider Strategy

Status: Accepted

## Context

Aegis Code inherits Codex provider behavior at import time, but production use
needs provider flexibility. OpenAI-compatible APIs provide the first continuity
path. Native Anthropic support is important because provider-native prompt
caching, tool semantics, and message formats affect cost and correctness. Local
OSS providers remain useful where practical.

Provider work must not obscure the initial upstream import or force an early
routing policy before the provider layer has been mapped.

## Decision

The first provider contract should preserve upstream OpenAI-compatible behavior.
Native Anthropic support is a first-class provider track. Local OSS provider
support should be preserved where practical.

Provider routing will be handled as a later policy layer after the provider
abstraction has been reviewed.

## Non-goals

- This ADR does not implement any provider.
- This ADR does not define provider routing rules, fallback behavior, model
  selection policy, or configuration migration.
- This ADR does not require provider changes during the upstream source import.

## Consequences

- The source import can prioritize fidelity to upstream provider behavior.
- Provider expansion can proceed in ordered slices: abstraction review, native
  Anthropic support, OpenAI-compatible preservation, local OSS preservation, and
  routing policy.
- Provider decisions remain part of Aegis Code because provider semantics affect
  prompt assembly, tool use, evidence, and session validity.
