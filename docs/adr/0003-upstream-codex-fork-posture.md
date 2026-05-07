# ADR 0003: Upstream Codex Fork Posture

Status: Accepted

## Context

Aegis Code is intended to preserve the familiar Codex coding loop while adding
Aegis-native validity controls. Building a new coding harness from scratch would
delay the product proof and increase the risk of diverging from proven upstream
behavior.

The repository currently contains only bootstrap documentation. The first source
implementation task is the upstream Codex import.

## Decision

Aegis Code will import upstream `openai/codex` as the implementation base. The
import must record the exact upstream commit, preserve Apache-2.0 license and
notice obligations, and keep upstream attribution visible.

Aegis-owned changes should remain auditable as intentional layers on top of the
imported source. The import commit must not mix in Aegis feature work.

Future sync work should maintain a clear distinction between upstream code and
Aegis-owned integration surfaces.

## Non-goals

- This ADR does not perform the upstream source import.
- This ADR does not define the complete upstream sync procedure, merge cadence,
  or conflict policy.
- This ADR does not rename binaries, packages, config paths, or product
  surfaces.

## Consequences

- Issue #4 can import upstream source without re-deciding the fork strategy.
- Issue #6 can document the operational sync workflow against a known posture.
- Attribution and license preservation become part of the implementation
  contract, not a cleanup task.
