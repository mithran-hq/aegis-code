# Getting Started With Aegis Code

This guide gives a working first-run path from a local checkout to an
interactive Aegis Code session. It assumes you are building from source because
release installers are still tracked separately from this documentation task.

## Requirements

- Rust 1.93.0 with `rustfmt`.
- Node 22 or newer and pnpm 10.33.0.
- Python 3.
- A provider credential or a local OSS provider.

For full setup details, see [Install and build](install.md).

## Build The CLI

Clone the repository and build the Rust workspace binary:

```bash
git clone https://github.com/mithran-hq/aegis-code.git
cd aegis-code
cargo build --manifest-path codex-rs/Cargo.toml --bin aegis
```

You can run the built binary directly:

```bash
./codex-rs/target/debug/aegis --version
```

Or install it into your Cargo bin directory:

```bash
cargo install --path codex-rs/cli --locked
```

After installation, `command -v aegis` should resolve the local CLI.

## Configure A Home Directory

Aegis Code uses `$AEGIS_HOME`, defaulting to `~/.aegis`. The main config file is
`~/.aegis/config.toml`.

Create the directory before first use:

```bash
mkdir -p ~/.aegis
```

Start with the minimal OpenAI-compatible provider path:

```toml
# ~/.aegis/config.toml
model_provider = "openai"
model = "gpt-5.4"

[aegis_engine]
enabled = true
failure_mode = "best-effort"
```

Then provide credentials with one of the supported authentication paths:

```bash
export OPENAI_API_KEY="..."
# or store the key through the CLI
printenv OPENAI_API_KEY | aegis login --with-api-key
```

For Anthropic or local providers, use the provider examples in
[Configuration](config.md).

## Check The Installation

Run doctor before starting real work:

```bash
aegis doctor
```

The report should identify the selected provider, model, sandbox posture, Aegis
Engine alert paths, and configured context packs. Environment keys are reported
as present or missing, but secret values are not printed.

If the provider is wrong, check `model_provider` in `~/.aegis/config.toml`. If
the provider key is missing, set the documented environment variable for that
provider and run `aegis doctor` again.

## Start An Interactive Session

From a project directory, start the familiar coding loop:

```bash
aegis
```

You can also pass the first prompt directly:

```bash
aegis "explain this repository and suggest a safe first task"
```

Aegis Code preserves the Codex-style loop while adding Aegis controls around
method state, evidence, review, sensitive tools, sandbox posture, provider
routing, runtime events, and context-pack promotion.

## Run Non-Interactively

Use `aegis exec` for scripts or CI:

```bash
aegis exec "summarize the pending diff"
```

When a workflow needs durable method-state evidence, pass method-state input and
output paths:

```bash
aegis exec \
  --method-state method-state.json \
  --method-state-output artifacts/method-state.json \
  --json \
  "run the required verification and summarize the evidence"
```

See [Non-interactive mode](exec.md) for exit codes and JSON output.

## Work From GitHub Issues

For Aegis Code repository work, child GitHub task issues are the source of
truth. Validate the issue train before landing task work:

```bash
aegis issue-train validate --repo mithran-hq/aegis-code --parent 1
```

Before a PR or task closure, validate readiness against the method-state
artifact:

```bash
aegis pr-readiness validate \
  --repo mithran-hq/aegis-code \
  --method-state artifacts/method-state.json
```

The day-to-day workflow is described in [Method workflow](method-workflow.md).

## Logs And Artifacts

- Config: `~/.aegis/config.toml`, or `$AEGIS_HOME/config.toml`.
- TUI log: `~/.aegis/log/aegis-tui.log`.
- Aegis Engine events: `~/.aegis/aegis-engine/events.jsonl`.
- Aegis Engine alerts: `~/.aegis/aegis-engine/alerts.jsonl`.
- Candidate context-pack inputs:
  `~/.aegis/aegis-engine/candidate-pack-inputs.jsonl`.

Runtime events and alerts never promote prompt changes by themselves. Learned
behavior affects future sessions only after it becomes a promoted context pack.
