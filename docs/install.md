# Installing And Building

Aegis Code currently supports source builds from this repository. GitHub
Release artifacts, DotSlash files, Homebrew, and npm wrappers are v1
distribution goals documented in [Distribution](DISTRIBUTION.md); do not assume
those installers exist until a release publishes them.

## System Requirements

| Requirement                 | Details                                                         |
| --------------------------- | --------------------------------------------------------------- |
| Operating systems           | macOS 12+, Ubuntu 20.04+/Debian 10+, or Windows 11 **via WSL2** |
| Git (optional, recommended) | 2.23+ for built-in PR helpers                                   |
| RAM                         | 4-GB minimum (8-GB recommended)                                 |
| Rust                        | 1.93.0 with `rustfmt`                                           |
| Node                        | 22 or newer                                                     |
| pnpm                        | 10.33.0                                                         |
| Python                      | Python 3                                                        |

## Source Build

Build the local CLI from a checkout:

```bash
git clone https://github.com/mithran-hq/aegis-code.git
cd aegis-code

curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"
rustup component add rustfmt
rustup component add clippy

cargo build --manifest-path codex-rs/Cargo.toml --bin aegis
./codex-rs/target/debug/aegis --version
```

Install the local binary on `PATH`:

```bash
cargo install --path codex-rs/cli --locked
```

From inside `codex-rs`, use:

```bash
cargo install --path cli --locked
```

Confirm the command name and diagnostics:

```bash
aegis --version
aegis doctor
```

## Optional Developer Tools

The Rust workspace justfile is useful for focused crate work:

```bash
cd codex-rs
cargo install just
cargo install --locked cargo-nextest

just fmt
just fix -p <crate-you-touched>
cargo test -p codex-tui
just test
```

Avoid `--all-features` for routine local runs because it increases build time
and `target/` disk usage by compiling additional feature combinations. Use it
only when full feature coverage is the point of the check:

```bash
cargo test --all-features
```

## Repository Checks

The repository-level local CI script is the required pre-push check:

```bash
./scripts/ci_local.sh
```

Use the broader suite for CI, dependency, workflow, or cross-cutting Rust
changes:

```bash
./scripts/ci_local.sh --full
```

The default suite checks repository scaffold files, README table of contents,
Markdown/JSON/workflow/JS formatting, Rust formatting, Rust workspace build,
workspace unit tests, and selected integration smoke tests.

## DotSlash And Release Installers

DotSlash, Homebrew, npm, and signed release artifacts are distribution targets,
not the baseline first-run path for this repository state. When a GitHub Release
contains a DotSlash file named `aegis`, teams can commit that file to source
control so every contributor runs the same platform binary.

## Tracing And Verbose Logging

Aegis Code is written in Rust, so it honors the `RUST_LOG` environment variable.

The TUI defaults to:

```text
RUST_LOG=codex_core=info,codex_tui=info,codex_rmcp_client=info
```

TUI logs are written to `~/.aegis/log/aegis-tui.log` by default. For a single
run, override the log directory with `-c log_dir=...`:

```bash
aegis -c log_dir=./.aegis-log
tail -F ~/.aegis/log/aegis-tui.log
```

The non-interactive mode, `aegis exec`, defaults to `RUST_LOG=error` and prints
messages inline, so it does not require monitoring a separate log file.
