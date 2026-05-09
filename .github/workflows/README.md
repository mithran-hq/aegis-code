# Workflow Strategy

The workflows in this directory are split so that pull requests get fast, review-friendly signal while `main` still gets the full cross-platform verification pass.

## Pull Requests

- `local-ci.yml` runs `./scripts/ci_local.sh` on Linux. This is the same
  default local pre-push contract developers and agents run before landing
  issue-sized work. The script uses `RUST_TEST_THREADS=4` by default for
  parallel-safe crates and runs `codex-core` unit tests serially for
  deterministic goal/session coverage. It runs workspace unit/bin/example tests
  and a focused integration smoke set rather than the multi-hour exhaustive
  Cargo integration matrix.
- `bazel.yml` is the main pre-merge verification path for Rust code.
  It runs Bazel `test` and Bazel `clippy` on the supported Bazel targets,
  including the generated Rust test binaries needed to lint inline `#[cfg(test)]`
  code.
- `rust-ci.yml` keeps the Cargo-native PR checks intentionally small:
  - `cargo fmt --check`
  - `cargo shear`
  - `argument-comment-lint` on Linux, macOS, and Windows
  - `tools/argument-comment-lint` package tests when the lint or its workflow wiring changes

## Post-Merge On `main`

- `bazel.yml` also runs on pushes to `main`.
  This re-verifies the merged Bazel path and helps keep the BuildBuddy caches warm.
- `rust-ci-full.yml` is the full Cargo-native verification workflow.
  It keeps the heavier checks off the PR path while still validating them after merge:
  - the full Cargo `clippy` matrix
  - the full Cargo `nextest` matrix
  - release-profile Cargo builds
  - cross-platform `argument-comment-lint`
  - Linux remote-env tests
- `ci.yml` is limited to cheap static and package-format checks. It does not
  stage npm release artifacts; native npm staging belongs to release and
  distribution workflow once release artifacts are available.
- `sdk.yml` is manual-only until the inherited TypeScript SDK workflow is ported
  to Aegis package names and hosted/actionable runner infrastructure.

## Rule Of Thumb

- Keep `./scripts/ci_local.sh` aligned with required pre-push checks. Add broad,
  slow, or optional-tool checks to `./scripts/ci_local.sh --full` unless they
  are cheap enough to run on every issue-sized task.
- If a build/test/clippy check can be expressed in Bazel, prefer putting the PR-time version in `bazel.yml`.
- Keep `rust-ci.yml` fast enough that it usually does not dominate PR latency.
- Reserve `rust-ci-full.yml` for heavyweight Cargo-native coverage that Bazel does not replace yet.
