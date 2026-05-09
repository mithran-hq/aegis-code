# Contributing

Aegis Code uses issue-sized delivery.

## Workflow

1. Work from a child GitHub issue with objective, scope, acceptance criteria,
   dependencies, and falsifiers.
2. Keep commits focused on one delivery unit.
3. Run `./scripts/ci_local.sh` before push.
4. Run `./scripts/ci_local.sh --full` for CI, dependency, workflow, or
   cross-cutting Rust changes.
5. Open a pull request with a clear summary, test plan, and `Fixes #<issue>`.
6. Perform adversarial review before merge.

## Local CI Toolchain

The default local CI suite requires Rust 1.93.0, Node 22 or newer, pnpm
10.33.0, and Python 3. It runs repository policy checks, README checks, pnpm
format checks, Rust formatting, Rust workspace build checks, and Rust workspace
unit/bin/example tests, plus integration smoke coverage for schema generation,
apply-patch, core, and app-server paths. Rust tests use `RUST_TEST_THREADS=4`
by default for parallel-safe crates, while `codex-core` unit tests run serially
for deterministic goal/session coverage; set that environment variable to tune
non-core concurrency.

The full local CI suite adds `cargo-deny`, `cargo-shear`, and Bazel/Bazelisk
checks. If a full-mode tool is missing, the script fails with install guidance
instead of silently skipping that check.

## Scope Discipline

If a task is too large, split it before implementation. If a side problem is
required to complete the current issue, include it. If it is not required,
capture it as a follow-up issue rather than expanding scope silently.

## Attribution

Do not add `Co-Authored-By` trailers. Preserve upstream license notices when
working with imported Codex source.
