#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

mode="default"

usage() {
  cat <<'EOF'
Usage: ./scripts/ci_local.sh [--full]

Runs the required local pre-push checks for Aegis Code.

Options:
  --full    Run the default checks plus heavier optional-tool checks.
  -h, --help
            Show this help.

Default toolchain:
  Rust 1.93.0, Node >=22, pnpm 10.33.0, Python 3.

Environment:
  RUST_TEST_THREADS
            Rust libtest concurrency for parallel-safe crates. Defaults to 4.
            codex-core unit tests run serially for deterministic goal/session tests.

Full-mode tools:
  cargo-deny, cargo-shear, bazel.
EOF
}

while (($#)); do
  case "$1" in
    --full)
      mode="full"
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

step_index=0

run_step() {
  local name="$1"
  shift
  step_index=$((step_index + 1))
  printf '\n[%02d] %s\n' "$step_index" "$name"
  set +e
  "$@"
  local status=$?
  set -e
  if [[ "$status" -ne 0 ]]; then
    printf '\nci_local failed in step %02d: %s\n' "$step_index" "$name" >&2
    printf 'command:' >&2
    printf ' %q' "$@" >&2
    printf '\n' >&2
    exit "$status"
  fi
}

require_command() {
  local cmd="$1"
  local guidance="$2"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "missing required command: $cmd" >&2
    echo "$guidance" >&2
    exit 127
  fi
}

check_scaffold() {
  local required_files=(
    README.md
    LICENSE
    NOTICE
    AGENTS.md
    CONTRIBUTING.md
    SECURITY.md
    CODE_OF_CONDUCT.md
    docs/ARCHITECTURE.md
    docs/DISTRIBUTION.md
    docs/IMPLEMENTATION_PLAN.md
    docs/adr/0001-aegis-code-product-boundary.md
    .github/ISSUE_TEMPLATE/plan.yml
    .github/ISSUE_TEMPLATE/task.yml
  )

  for file in "${required_files[@]}"; do
    if [[ ! -s "$file" ]]; then
      echo "missing required file: $file" >&2
      return 1
    fi
  done

  if grep -R "bruno-gate\\|Bruno Gate" README.md AGENTS.md .github 2>/dev/null; then
    echo "found stale bruno-gate naming in product surfaces" >&2
    return 1
  fi
}

check_default_tools() {
  require_command python3 "Install Python 3 and retry."
  require_command cargo "Install Rust 1.93.0 with rustup and retry."
  require_command pnpm "Install pnpm 10.33.0, or run corepack enable, and retry."
}

check_full_tools() {
  require_command cargo-deny "Install with: cargo install --locked cargo-deny"
  require_command cargo-shear "Install with: cargo install --locked cargo-shear"
  require_command bazel "Install Bazel/Bazelisk and retry."
}

rust_test_threads() {
  printf '%s' "${RUST_TEST_THREADS:-4}"
}

run_rust_unit_tests() {
  local test_threads
  test_threads="$(rust_test_threads)"
  RUST_MIN_STACK="${RUST_MIN_STACK:-8388608}" cargo test \
    --manifest-path codex-rs/Cargo.toml \
    --workspace \
    --exclude codex-core \
    --lib \
    --bins \
    --examples \
    --locked \
    -- \
    --test-threads="$test_threads"

  RUST_MIN_STACK="${RUST_MIN_STACK:-8388608}" cargo test \
    --manifest-path codex-rs/Cargo.toml \
    -p codex-core \
    --lib \
    --locked \
    -- \
    --test-threads=1
}

run_rust_integration_smoke_tests() {
  local test_threads
  test_threads="$(rust_test_threads)"

  RUST_MIN_STACK="${RUST_MIN_STACK:-8388608}" cargo test \
    --manifest-path codex-rs/Cargo.toml \
    -p codex-app-server-protocol \
    --test schema_fixtures \
    --locked \
    -- \
    --test-threads="$test_threads"

  RUST_MIN_STACK="${RUST_MIN_STACK:-8388608}" cargo test \
    --manifest-path codex-rs/Cargo.toml \
    -p codex-apply-patch \
    --test all \
    --locked \
    -- \
    --test-threads="$test_threads"

  RUST_MIN_STACK="${RUST_MIN_STACK:-8388608}" cargo test \
    --manifest-path codex-rs/Cargo.toml \
    -p codex-core \
    --test all \
    --locked \
    suite::exec::exit_code_0_succeeds \
    -- \
    --test-threads=1

  RUST_MIN_STACK="${RUST_MIN_STACK:-8388608}" cargo test \
    --manifest-path codex-rs/Cargo.toml \
    -p codex-app-server \
    --test all \
    --locked \
    suite::v2::thread_start::thread_start_creates_thread_and_emits_started \
    -- \
    --test-threads=1
}

run_step "Check local tool availability" check_default_tools
run_step "Validate repository scaffold" check_scaffold
run_step "Verify Cargo workspace manifest policy" python3 .github/scripts/verify_cargo_workspace_manifests.py
run_step "Verify TUI/core dependency boundary" python3 .github/scripts/verify_tui_core_boundary.py
run_step "Verify Bazel clippy lint wiring" python3 .github/scripts/verify_bazel_clippy_lints.py
run_step "Check README ASCII policy" ./scripts/asciicheck.py README.md
run_step "Check README table of contents" python3 scripts/readme_toc.py README.md
run_step "Install Node dependencies" pnpm install --frozen-lockfile
run_step "Check Markdown, JSON, workflow, and JS formatting" pnpm run format
run_step "Check Rust formatting" bash -c 'cd codex-rs && cargo fmt --all -- --config imports_granularity=Item --check'
run_step "Check Rust workspace build" cargo check --manifest-path codex-rs/Cargo.toml --workspace --locked
run_step "Run Rust workspace unit tests" run_rust_unit_tests
run_step "Run Rust integration smoke tests" run_rust_integration_smoke_tests

if [[ "$mode" == "full" ]]; then
  run_step "Check full-mode tool availability" check_full_tools
  run_step "Run Rust clippy workspace lint" cargo clippy --manifest-path codex-rs/Cargo.toml --workspace --tests --locked -- -D warnings
  run_step "Run cargo deny" bash -c 'cd codex-rs && cargo deny check'
  run_step "Run cargo shear" bash -c 'cd codex-rs && cargo shear'
  run_step "Check Bazel module lock" ./scripts/check-module-bazel-lock.sh
fi

printf '\nci_local %s checks passed\n' "$mode"
