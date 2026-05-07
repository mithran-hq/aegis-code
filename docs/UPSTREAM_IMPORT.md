# Upstream Codex Import

Issue #4 imports upstream OpenAI Codex as the implementation base for Aegis
Code.

## Imported Snapshot

| Field | Value |
| --- | --- |
| Upstream repository | https://github.com/openai/codex |
| Upstream default branch | `main` |
| Imported commit | `f7e8ff8e5026f92fc4b0be1478bf98f7ffcdd781` |
| Commit URL | https://github.com/openai/codex/commit/f7e8ff8e5026f92fc4b0be1478bf98f7ffcdd781 |
| Commit date | 2026-05-07T11:33:47+02:00 |
| Commit subject | Make turn diff tracking operation backed (#21180) |

The local repository has an `upstream` remote pointing at
`https://github.com/openai/codex.git`.
See `docs/UPSTREAM.md` for the operational sync workflow.

## Import Method

The import was performed as a single snapshot of the pinned upstream tree. The
snapshot keeps upstream source and build surfaces directly in this repository so
future Aegis Code work can modify them as normal fork commits.

This task intentionally does not rename Codex product surfaces. The follow-up
rename task should expose the local CLI as `aegis`.

## Collision Policy

Aegis-owned governance and planning files remained authoritative at the repo
root:

- `README.md`
- `AGENTS.md`
- `LICENSE`
- `NOTICE`
- `SECURITY.md`
- `.github/ISSUE_TEMPLATE/*`
- `.github/pull_request_template.md`

Upstream implementation, build, docs, scripts, SDK, tooling, and workflow
surfaces were imported where they did not replace those Aegis-owned files.
`.gitignore` was merged so Aegis local ignores and upstream build/tool ignores
are both preserved.

## License And Notice

OpenAI Codex is licensed under the Apache License, Version 2.0. Aegis Code keeps
the Apache-2.0 license and records OpenAI Codex attribution and upstream notice
obligations in `NOTICE`.

## Verification

Verification for this import:

- `./scripts/ci_local.sh` passed.
- `pnpm install --frozen-lockfile` passed.
- From `codex-rs/`,
  `cargo fmt -- --config imports_granularity=Item --check` passed. Stable
  rustfmt warned that `imports_granularity=Item` is nightly-only.
- `cargo check --manifest-path codex-rs/Cargo.toml --workspace --locked`
  passed.

If a verification command fails due to local tooling or an upstream build gap,
record the exact failure here and create a follow-up GitHub issue before closing
#4.
