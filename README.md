# Aegis Code

[![License](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

> **Status:** Archived. `mithran-hq/aegis` is the Aegis OSS native product
> control plane. This repo is retained as historical Codex-derived harness
> evidence and Codex adapter fixtures only.

Aegis Code is a Codex-derived harness. It is no longer the center of the Aegis
OSS architecture. Per D42, cross-agent local supervision, mutation authority,
evidence-journal ownership, daemon authority, and product packaging belong in
[`mithran-hq/aegis`](https://github.com/mithran-hq/aegis).

## Archived Purpose

The final purpose of this repo is:

- Preserve Codex adapter fixtures for `mithran-hq/aegis#5` and
  `mithran-hq/aegis#6`.
- Document `~/.codex`, `$AEGIS_HOME`, config/session, and `AGENTS.md`
  expectations useful to the Aegis daemon.
- Preserve historical issue-train evidence from the superseded Codex-derived
  harness.

The extraction artifact is
[docs/codex-adapter-fixtures.md](docs/codex-adapter-fixtures.md).

## Documentation

Start with the [documentation home](docs/README.md) and the
[first-run guide](docs/getting-started.md). The docs distinguish behavior that
was implemented during the Codex-derived harness train from work that has now
been superseded by `mithran-hq/aegis`.

## Boundary

Aegis Code is not Aegis Secret, Aegis Engine, Aegis Agent Runtime, or the Aegis
native product control plane.

Those projects stay separate:

| Project             | Role                                                      |
| ------------------- | --------------------------------------------------------- |
| Aegis               | Native app, daemon, adapters, evidence, mutation, package |
| Aegis Code          | Temporary Codex adapter fixture source                    |
| Aegis Secret        | Authority broker for sensitive commands and secrets       |
| Aegis Engine        | Asynchronous event intelligence and context-pack learning |
| Aegis Agent Runtime | Shared execution, sandbox, session, and tool substrate    |

## Current Status

This archived repository contains the Codex-derived Aegis Code CLI and
historical issue train evidence. The old product train is superseded by
[`mithran-hq/aegis#1`](https://github.com/mithran-hq/aegis/issues/1).

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
