# Aegis Code CLI (Rust Implementation)

We provide Aegis Code CLI as a standalone executable to ensure a zero-dependency install.

## Installing Aegis Code

Today, the easiest way to install Aegis Code is via `npm`:

```shell
npm i -g @mithran/aegis
aegis
```

You can also install via Homebrew (`brew install --cask aegis`) or download a platform-specific release directly from our [GitHub Releases](https://github.com/mithran-hq/aegis-code/releases).

## Documentation quickstart

- First run with Aegis Code? Start with [`docs/getting-started.md`](../docs/getting-started.md) (links to the walkthrough for prompts, keyboard shortcuts, and session management).
- Want deeper control? See [`docs/config.md`](../docs/config.md) and [`docs/install.md`](../docs/install.md).

## What's new in the Rust CLI

The Rust implementation is now the maintained Aegis Code CLI and serves as the default experience. It includes a number of features that the legacy TypeScript CLI never supported.

### Config

Aegis Code supports a rich set of configuration options. Note that the Rust CLI uses `config.toml` instead of `config.json`. See [`docs/config.md`](../docs/config.md) for details.

### Model Context Protocol Support

#### MCP client

Aegis Code CLI functions as an MCP client that allows the Aegis Code CLI and IDE extension to connect to MCP servers on startup. See the [`configuration documentation`](../docs/config.md#connecting-to-mcp-servers) for details.

#### MCP server (experimental)

Aegis Code can be launched as an MCP _server_ by running `aegis mcp-server`. This allows _other_ MCP clients to use Aegis Code as a tool for another agent.

Use the [`@modelcontextprotocol/inspector`](https://github.com/modelcontextprotocol/inspector) to try it out:

```shell
npx @modelcontextprotocol/inspector aegis mcp-server
```

Use `aegis mcp` to add/list/get/remove MCP server launchers defined in `config.toml`, and `aegis mcp-server` to run the MCP server directly.

### Notifications

You can enable notifications by configuring a script that is run whenever the agent finishes a turn. The [notify documentation](../docs/config.md#notify) includes a detailed example that explains how to get desktop notifications via [terminal-notifier](https://github.com/julienXX/terminal-notifier) on macOS. When Aegis Code detects that it is running under WSL 2 inside Windows Terminal (`WT_SESSION` is set), the TUI automatically falls back to native Windows toast notifications so approval prompts and completed turns surface even though Windows Terminal does not implement OSC 9.

### `aegis exec` to run Aegis Code programmatically/non-interactively

To run Aegis Code non-interactively, run `aegis exec PROMPT` (you can also pass the prompt via `stdin`) and Aegis Code will work on your task until it decides that it is done and exits. If you provide both a prompt argument and piped stdin, Aegis Code appends stdin as a `<stdin>` block after the prompt so patterns like `echo "my output" | aegis exec "Summarize this concisely"` work naturally. Output is printed to the terminal directly. You can set the `RUST_LOG` environment variable to see more about what's going on.
Use `aegis exec --ephemeral ...` to run without persisting session rollout files to disk.

### Experimenting with the Aegis Sandbox

To test to see what happens when a command is run under the sandbox provided by Aegis Code, we provide the following subcommands in Aegis Code CLI:

```
# macOS
aegis sandbox macos [--log-denials] [COMMAND]...

# Linux
aegis sandbox linux [COMMAND]...

# Windows
aegis sandbox windows [COMMAND]...

# Legacy aliases
aegis debug seatbelt [--log-denials] [COMMAND]...
aegis debug landlock [COMMAND]...
```

To try a writable legacy sandbox mode with these commands, pass an explicit config override such
as `-c 'sandbox_mode="workspace-write"'`.

### Selecting a sandbox policy via `--sandbox`

The Rust CLI exposes a dedicated `--sandbox` (`-s`) flag that lets you pick the sandbox policy **without** having to reach for the generic `-c/--config` option:

```shell
# Run Aegis Code with the default, read-only sandbox
aegis --sandbox read-only

# Allow the agent to write within the current workspace while still blocking network access
aegis --sandbox workspace-write

# Danger! Disable sandboxing entirely (only do this if you are already running in a container or other isolated env)
aegis --sandbox danger-full-access
```

The same setting can be persisted in `~/.aegis/config.toml` via the top-level `sandbox_mode = "MODE"` key, e.g. `sandbox_mode = "workspace-write"`.
In `workspace-write`, Aegis Code also includes `~/.aegis/memories` in its writable roots so memory maintenance does not require an extra approval.

## Code Organization

This folder is the root of a Cargo workspace. It contains quite a bit of experimental code, but here are the key crates:

- [`core/`](./core) contains the business logic for Aegis Code. Ultimately, we hope this becomes a library crate that is generally useful for building other Rust/native applications that use Aegis Code.
- [`exec/`](./exec) "headless" CLI for use in automation.
- [`tui/`](./tui) CLI that launches a fullscreen TUI built with [Ratatui](https://ratatui.rs/).
- [`cli/`](./cli) CLI multitool that provides the aforementioned CLIs via subcommands.

If you want to contribute or inspect behavior in detail, start by reading the module-level `README.md` files under each crate and run the project workspace from the top-level `codex-rs` directory so shared config, features, and build scripts stay aligned.
