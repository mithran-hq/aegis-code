# Aegis Code

[![License](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

Aegis Code is an Aegis-controlled coding agent harness derived from Codex.

The project exists to make agent coding work falsifiable, resumable,
policy-aware, and auditable. Prompts remain useful, but the product boundary is
the harness: method state, evidence receipts, tool policy, sandbox posture,
provider routing, and structured events.

## Product Shape

The planned v1 binary is:

```bash
aegis-code
```

The first release should keep the familiar Codex-style coding loop while adding
Aegis-native control:

- method gates for intent, claims, assumptions, falsifiers, evidence, review,
  and closure
- issue-train validation for parent plans and child execution tasks
- evidence receipts that survive memory loss and session resume
- Aegis Secret mediation for sensitive local commands
- optional Aegis Agent Runtime execution substrate
- asynchronous Aegis Engine events and promoted context packs
- provider support for OpenAI-compatible APIs, native Anthropic, and local OSS
  providers where practical

## Boundary

Aegis Code is not Aegis Secret, Aegis Engine, or Aegis Agent Runtime.

Those projects stay separate:

| Project | Role |
| --- | --- |
| Aegis Code | Coding agent harness and validity control |
| Aegis Secret | Authority broker for sensitive commands and secrets |
| Aegis Engine | Asynchronous event intelligence and context-pack learning |
| Aegis Agent Runtime | Shared execution, sandbox, session, and tool substrate |

## Current Status

This repository starts with a bootstrap commit and a GitHub issue train. The
first implementation task imports the upstream Codex source and establishes the
long-term fork/sync strategy.

See [docs/IMPLEMENTATION_PLAN.md](docs/IMPLEMENTATION_PLAN.md).

## Development

Until the source import lands, the local CI script validates the repository
scaffold:

```bash
./scripts/ci_local.sh
```

After implementation begins, each child GitHub issue is the unit of delivery.
Do not implement from the parent plan issue when child task issues exist.
