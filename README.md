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
aegis
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

## Documentation

Start with the [documentation home](docs/README.md) and the
[first-run guide](docs/getting-started.md). The docs distinguish behavior that
is implemented today from release and distribution work that is still tracked
as roadmap.

## Boundary

Aegis Code is not Aegis Secret, Aegis Engine, or Aegis Agent Runtime.

Those projects stay separate:

| Project             | Role                                                      |
| ------------------- | --------------------------------------------------------- |
| Aegis Code          | Coding agent harness and validity control                 |
| Aegis Secret        | Authority broker for sensitive commands and secrets       |
| Aegis Engine        | Asynchronous event intelligence and context-pack learning |
| Aegis Agent Runtime | Shared execution, sandbox, session, and tool substrate    |

## Current Status

This repository contains the Codex-derived Aegis Code CLI and a GitHub issue
train for issue-sized delivery.

See [docs/IMPLEMENTATION_PLAN.md](docs/IMPLEMENTATION_PLAN.md).

## Development

The local CI script is the required pre-push check for issue-sized work:

```bash
./scripts/ci_local.sh
```

Use the broader local suite for CI, dependency, workflow, or cross-cutting Rust
changes:

```bash
./scripts/ci_local.sh --full
```

The default suite requires Rust 1.93.0, Node 22 or newer, pnpm 10.33.0, and
Python 3. It runs workspace unit/bin/example tests plus integration smoke
coverage for schema generation, apply-patch, core, and app-server paths. Rust
tests run with `RUST_TEST_THREADS=4` by default for parallel-safe crates, while
`codex-core` unit tests run serially for deterministic goal/session coverage;
override that environment variable when you need different non-core test
concurrency. The full suite additionally requires `cargo-deny`,
`cargo-shear`, and Bazel or Bazelisk.

After implementation begins, each child GitHub issue is the unit of delivery.
Do not implement from the parent plan issue when child task issues exist.
