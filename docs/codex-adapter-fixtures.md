# Codex Adapter Fixtures

This repository is retained only long enough to extract Codex adapter evidence
for `mithran-hq/aegis`. The native Aegis daemon owns cross-agent observation,
mutation authority, evidence, and product packaging. Aegis Code does not remain
a runtime dependency for the daemon.

The fixtures under `fixtures/codex-adapter/` are synthetic examples for
`mithran-hq/aegis#5` and `mithran-hq/aegis#6`. They are not a daemon protocol;
the final protocol belongs in `mithran-hq/aegis#5`.

## Source Evidence

The extracted expectations come from these repo sources:

| Evidence                                                              | Finding                                                                                                                                                                                                          |
| --------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `docs/migration-from-codex.md`                                        | Upstream Codex uses `~/.codex`; Aegis Code used `$AEGIS_HOME`, defaulting to `~/.aegis`; safe imports read from `~/.codex/config.toml` and write to `$AEGIS_HOME/config.toml`.                                   |
| `codex-rs/utils/home-dir/src/lib.rs`                                  | `$AEGIS_HOME` overrides the default home and must point at an existing directory when set.                                                                                                                       |
| `codex-rs/core/src/agents_md.rs`                                      | Global instructions load from `$AEGIS_HOME/AGENTS.override.md` before `$AEGIS_HOME/AGENTS.md`; project instructions walk from project root to cwd and use per-directory `AGENTS.override.md` before `AGENTS.md`. |
| `codex-rs/core/src/managed_guidance.rs`                               | Managed guidance is delimited by `<!-- BEGIN AEGIS CODE MANAGED GUIDANCE -->` and `<!-- END AEGIS CODE MANAGED GUIDANCE -->`; malformed or duplicate managed blocks are conflicts.                               |
| `codex-rs/rollout/src/recorder.rs` and `codex-rs/rollout/src/list.rs` | Session rollouts live under `$AEGIS_HOME/sessions/YYYY/MM/DD/rollout-YYYY-MM-DDThh-mm-ss-<uuid>.jsonl`; archived sessions use `$AEGIS_HOME/archived_sessions`.                                                   |
| `docs/aegis-engine.md` and `docs/aegis-runtime-events.md`             | Historical Aegis Code event paths were `$AEGIS_HOME/aegis-engine/events.jsonl`, `alerts.jsonl`, and `candidate-pack-inputs.jsonl`; daemon ownership now moves to `mithran-hq/aegis`.                             |

## Watch Targets

The daemon Codex adapter should be able to observe these paths. Mutation is
allowed only for the managed instruction targets listed in
[Mutation Targets](#mutation-targets):

- `~/.codex/config.toml`: upstream Codex user configuration and safe migration
  source.
- `<project>/.codex/config.toml`: project-scoped Codex configuration when
  present.
- `$AEGIS_HOME/config.toml`: Aegis Code historical home configuration, defaulting
  to `~/.aegis/config.toml` when `AEGIS_HOME` is unset.
- `$AEGIS_HOME/AGENTS.override.md` and `$AEGIS_HOME/AGENTS.md`: global user
  instruction files.
- Project `AGENTS.override.md` and `AGENTS.md` files from project root to cwd.
- `$AEGIS_HOME/sessions/YYYY/MM/DD/rollout-*.jsonl` and
  `$AEGIS_HOME/archived_sessions/**/rollout-*.jsonl`: session observation
  sources. Treat transcript content as sensitive evidence; do not copy raw
  prompts into daemon logs.

## Mutation Targets

Phase 0 mutation should be limited to managed guidance blocks in instruction
files:

- `$AEGIS_HOME/AGENTS.md`
- Project-root `AGENTS.md`
- Any later daemon-owned context-pack or prompt target defined by
  `mithran-hq/aegis`

The adapter must preserve all user-authored content outside the managed block.
It must report a conflict and avoid writing when an instruction file contains
duplicate markers, missing markers, or an end marker before a begin marker.

The adapter must not mutate `~/.codex/config.toml`, authentication files,
session rollouts, raw history, or secret-bearing config values. Safe config
migration can be offered as a reviewed operation, but that operation belongs to
the native Aegis product, not to this fork.

## Instruction Loading Semantics

Global instruction precedence:

1. `$AEGIS_HOME/AGENTS.override.md`
2. `$AEGIS_HOME/AGENTS.md`

Project instruction discovery:

1. Determine the project root by walking upward from the cwd until a configured
   project-root marker is found. The default marker is `.git`.
2. Walk from the project root down to the cwd.
3. In each directory, use `AGENTS.override.md` when present; otherwise use
   `AGENTS.md`.
4. Concatenate discovered project instruction files in root-to-cwd order.

Reload prompts are required after managed instruction or config changes because
running sessions may already have assembled their prompt context. The daemon can
surface this as a restart or new-session prompt.

## Phase 0 Extension Finding

No harness-level extension is required for `mithran-hq/aegis#6`. The watcher
proof can be implemented from filesystem observation, the precedence rules
above, and the synthetic fixtures in `fixtures/codex-adapter/`.

Useful historical helper behavior exists in Aegis Code, but it should be
treated as evidence rather than a dependency:

- `aegis config import-codex` previewed safe copies from `~/.codex/config.toml`
  into `$AEGIS_HOME/config.toml`.
- `aegis guidance install` managed bounded instruction blocks in user and repo
  `AGENTS.md` files.

The native daemon should reimplement or adapt these capabilities inside
`mithran-hq/aegis` if they remain desirable.

## Fixture Map

`fixtures/codex-adapter/` contains:

- `codex-home/config.toml`: upstream Codex user config observation source.
- `aegis-home/config.toml`: historical `$AEGIS_HOME` config after safe import.
- `aegis-home/AGENTS.md`: global managed guidance target.
- `aegis-home/AGENTS.override.md`: global override that wins over
  `AGENTS.md`.
- `project-root/.codex/config.toml`: project Codex config observation source.
- `project-root/AGENTS.md`: project managed guidance target.
- `project-root/service/AGENTS.override.md`: nested project override source.
- `aegis-home/sessions/2026/05/11/rollout-2026-05-11T12-00-00-00000000-0000-0000-0000-000000000062.jsonl`:
  synthetic rollout evidence.
- `observations/*.json`: non-protocol examples of normalized adapter findings
  for protocol design and watcher tests.

All fixture paths are placeholders. They must not be interpreted as real user
home paths, and they contain no secrets.
