# AGENTS.md

For information about AGENTS.md, see [this documentation](https://developers.openai.com/codex/guides/agents-md).

## Hierarchical agents message

When the `child_agents_md` feature flag is enabled (via `[features]` in `config.toml`), Codex appends additional guidance about AGENTS.md scope and precedence to the user instructions message and emits that message even when no AGENTS.md is present.

## Managed Aegis guidance

`aegis guidance install` writes Aegis Code method guidance into managed blocks
inside instruction files. Managed blocks are delimited with
`<!-- BEGIN AEGIS CODE MANAGED GUIDANCE -->` and
`<!-- END AEGIS CODE MANAGED GUIDANCE -->`, so user-authored content outside the
markers is preserved.

Use `--target user` to manage `$AEGIS_HOME/AGENTS.md`, `--target repo` to manage
the project-root `AGENTS.md`, or `--target all` to update both. `$AEGIS_HOME/AGENTS.override.md`
remains a manual override and is not modified by the installer.

Preview exact changes without writing files:

```bash
aegis guidance install --target all --dry-run
```

Apply or remove the managed guidance:

```bash
aegis guidance install --target all
aegis guidance remove --target all
```

If an instruction file contains multiple managed blocks or malformed markers,
the command reports a conflict and leaves the file unchanged.
