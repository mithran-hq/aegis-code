# Codex Architecture Map

This map names the Codex-derived crates and modules where Aegis Code should
attach control surfaces. It is intentionally a map, not a redesign: prefer
narrow adapters at these seams before changing inherited Codex flow.

## Runtime Path

The normal interactive path starts in `codex-rs/cli/src/main.rs`. The `aegis`
binary parses the top-level command, forwards interactive runs to
`codex-rs/tui`, forwards non-interactive runs to `codex-rs/exec`, and exposes
maintenance surfaces such as MCP, sandbox, review, and config debugging.

Session ownership lives in `codex-rs/core/src/thread_manager.rs`,
`codex-rs/core/src/codex_thread.rs`, and `codex-rs/core/src/session/`. A
`ThreadManager` creates or resumes a `CodexThread`; the thread wraps a
`Session`; the session builds per-turn `TurnContext` values and runs task types
from `codex-rs/core/src/tasks/`.

The main agent loop is `run_turn` in `codex-rs/core/src/session/turn.rs`.
That function assembles prompt input, resolves skills/plugins/MCP tools,
creates the `ToolRouter`, streams model responses through `ModelClient`, and
dispatches tool calls through the tool runtime.

Model access is centered on `codex-rs/core/src/client.rs`,
`codex-rs/model-provider/src/`, and `codex-rs/model-provider-info/src/`.
Provider selection is configured before the session starts, while per-turn
model, reasoning, tools, telemetry, and sandbox values flow through
`TurnContext`.

Tool execution is centered on `codex-rs/core/src/tools/`. Tool specs are built
from `spec.rs` and `spec_plan.rs`, resolved by `router.rs`, executed by
`parallel.rs`, and mediated by `orchestrator.rs` plus runtime implementations
under `runtimes/`.

Persistence is split between transcript rollouts and thread metadata.
`codex-rs/rollout/src/recorder.rs` records session events and transcript items;
`codex-rs/thread-store/src/` provides local, remote, and in-memory thread store
implementations; `codex-rs/core/src/session/rollout_reconstruction.rs`
hydrates resumed and forked sessions.

## Extension Points

| Area                | Primary entry points                                                                                                                                                                                                     | Aegis attachment                                                                                                                        |
| ------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------- |
| Provider layer      | `codex-rs/core/src/client.rs`, `codex-rs/model-provider/src/provider.rs`, `codex-rs/model-provider-info/src/lib.rs`                                                                                                      | Add provider contracts and routing around provider creation and per-turn request setup, preserving OpenAI-compatible defaults.          |
| Prompt assembly     | `codex-rs/core/src/session/turn.rs`, `codex-rs/core/src/context/`, `codex-rs/core/src/client_common.rs`, `codex-rs/core/src/prompt_debug.rs`                                                                             | Insert deterministic Aegis context layers before `Prompt` is sent, and keep debug rendering aligned with real prompt assembly.          |
| Tool execution      | `codex-rs/core/src/tools/router.rs`, `codex-rs/core/src/tools/registry.rs`, `codex-rs/core/src/tools/parallel.rs`, `codex-rs/core/src/tools/handlers/`                                                                   | Add Aegis tool gates at dispatch boundaries without rewriting individual tool handlers unless a handler has tool-specific policy.       |
| Sandbox policy      | `codex-rs/core/src/session/turn_context.rs`, `codex-rs/core/src/tools/orchestrator.rs`, `codex-rs/core/src/tools/sandboxing.rs`, `codex-rs/sandboxing/src/`                                                              | Attach policy selection to `PermissionProfile`, `TurnContext`, and orchestrator decisions before platform sandbox execution.            |
| Approvals           | `codex-rs/core/src/tools/orchestrator.rs`, `codex-rs/core/src/guardian/`, `codex-rs/core/src/session/mcp.rs`, `codex-rs/core/src/tools/handlers/request_permissions.rs`                                                  | Route sensitive approvals through Aegis policy before user or guardian approval, keeping fail-closed behavior.                          |
| MCP client tools    | `codex-rs/core/src/mcp.rs`, `codex-rs/core/src/mcp_tool_call.rs`, `codex-rs/core/src/mcp_tool_exposure.rs`, `codex-rs/core/src/session/mcp.rs`                                                                           | Mediate exposed MCP tools, approval metadata, and elicitation handling at the session boundary.                                         |
| MCP server surface  | `codex-rs/mcp-server/src/message_processor.rs`, `codex-rs/mcp-server/src/codex_tool_runner.rs`, `codex-rs/mcp-server/src/exec_approval.rs`, `codex-rs/mcp-server/src/patch_approval.rs`                                  | Expose Aegis-owned MCP operations through the existing server request processor and approval adapters.                                  |
| Session persistence | `codex-rs/core/src/session/mod.rs`, `codex-rs/core/src/codex_thread.rs`, `codex-rs/rollout/src/recorder.rs`, `codex-rs/thread-store/src/`                                                                                | Persist method state and evidence beside existing thread lifecycle, rollout, and metadata writes.                                       |
| Exec mode           | `codex-rs/exec/src/cli.rs`, `codex-rs/exec/src/lib.rs`, `codex-rs/exec/src/event_processor*.rs`, `codex-rs/core/src/tasks/regular.rs`                                                                                    | Keep non-interactive behavior as a first-class frontend over the same core session and event stream.                                    |
| TUI                 | `codex-rs/tui/src/app.rs`, `codex-rs/tui/src/app/thread_events.rs`, `codex-rs/tui/src/chatwidget/`, `codex-rs/tui/src/bottom_pane/`                                                                                      | Render Aegis method state, evidence, and approval status from protocol events rather than duplicating runtime state.                    |
| Review command      | `codex-rs/cli/src/main.rs`, `codex-rs/core/src/session/review.rs`, `codex-rs/core/src/tasks/review.rs`, `codex-rs/core/src/review_prompts.rs`                                                                            | Implement adversarial review as a task-mode specialization over the existing review thread path.                                        |
| Config loading      | `codex-rs/config/src/`, `codex-rs/core/src/config/mod.rs`, `codex-rs/core/src/config/schema.rs`, `codex-rs/config/src/loader/`                                                                                           | Add Aegis configuration through typed TOML, layered loading, and generated schema paths.                                                |
| Test harnesses      | `codex-rs/core/src/test_support.rs`, `codex-rs/core/src/session/tests.rs`, `codex-rs/core/src/tools/*_tests.rs`, `codex-rs/exec/src/*_tests.rs`, `codex-rs/tui/src/snapshots/`, `codex-rs/thread-store/src/in_memory.rs` | Cover new behavior at the nearest seam, using in-memory stores and existing session/tool/TUI fixtures before broader integration tests. |

## Aegis Feature Map

| Planned work                                         | Extension points                                                                                                                                                                                              |
| ---------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Method state model and persistence (#8, #9)          | Define the data model near `codex-rs/protocol/src/` or `codex-rs/core/src/state/`; persist through `Session`, `RolloutRecorder`, and `ThreadStore`; resume through `rollout_reconstruction.rs`.               |
| Aegis context packs (#10-#13)                        | Load config through `codex-rs/config`; inject active packs through `codex-rs/core/src/context/` and `build_prompt` in `session/turn.rs`; debug with `prompt_debug.rs`.                                        |
| Aegis Secret and tool preflight gates (#14-#16)      | Mediate commands in `tools/orchestrator.rs`, `tools/runtimes/shell.rs`, `tools/runtimes/unified_exec.rs`, `execpolicy`, and MCP approval paths in `session/mcp.rs`.                                           |
| Evidence receipts and validators (#17-#20)           | Record evidence through `rollout`, `thread-store`, `event_mapping.rs`, and protocol events; implement CLI validators from `codex-rs/cli/src/main.rs` with core helpers.                                       |
| Adversarial review command (#21)                     | Extend the existing review flow in `session/review.rs`, `tasks/review.rs`, `review_prompts.rs`, and `review_format.rs`.                                                                                       |
| Optional Aegis Agent Runtime (#22)                   | Attach runtime selection at `tools/runtimes/`, `exec-server`, `sandboxing`, and `TurnContext` permission/profile resolution.                                                                                  |
| Aegis Engine events and learning (#23-#26)           | Emit structured events from `Session::send_event` call sites, protocol event types, analytics/otel hooks, and rollout persistence; compile learned packs as inactive context-pack candidates until promotion. |
| Provider review and routing (#27-#31)                | Use `model-provider`, `model-provider-info`, `ModelClient`, and `TurnContext::with_model`; keep routing policy outside provider implementations until the abstraction review lands.                           |
| Config migration and managed installers (#32, #33)   | Use typed config in `codex-rs/config`, edit helpers in `codex-rs/core/src/config/edit.rs`, CLI subcommands in `codex-rs/cli/src/main.rs`, and TUI config persistence where interactive flows need it.         |
| MCP server and non-interactive exec (#34, #35)       | Build on `codex-rs/mcp-server` request handling and `codex-rs/exec` event processors, keeping both as frontends over core session behavior.                                                                   |
| TUI method status and sandbox integration (#36, #37) | Render state in `codex-rs/tui/src/chatwidget/` and `bottom_pane/`; enforce sandbox changes in `TurnContext`, `ToolOrchestrator`, and `codex-rs/sandboxing`.                                                   |
| CI, integration, and security tests (#38-#40)        | Extend `scripts/ci_local.sh`, core/tool/exec/TUI tests, rollout/thread-store fixtures, and targeted redaction/security cases.                                                                                 |
| Docs and distribution (#41-#45)                      | Keep user docs under `docs/`; package through `codex-cli`, `codex-rs/cli`, release workflows, Homebrew/npm wrapper surfaces, and diagnostics commands.                                                        |
| Supersede old planning repo (#46)                    | Use repository docs, issue train state, and release diagnostics as closure evidence; no runtime attachment point is required.                                                                                 |

## Unknowns And Follow-Ups

No new follow-up issue is required for this map. The local attachment points are
known for every planned Aegis feature above.

External wire contracts for Aegis Secret, Aegis Engine, and Aegis Agent Runtime
are intentionally left to their existing child issues. Those issues should
define contracts before implementation crosses repository boundaries.

When a future task discovers that a listed seam is too broad, split that task
and update this map in the same commit that introduces the narrower seam.
