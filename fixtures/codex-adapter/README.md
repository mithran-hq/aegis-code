# Codex Adapter Fixtures

These fixtures are synthetic inputs for `mithran-hq/aegis#5` and
`mithran-hq/aegis#6`. They capture Codex and historical Aegis Code filesystem
behavior without requiring the native Aegis daemon to depend on this fork at
runtime.

The JSON files in `observations/` use a deliberately small fixture envelope.
They are evidence for protocol design, not the final daemon protocol.

Important paths:

- `codex-home/` represents `~/.codex`.
- `aegis-home/` represents `$AEGIS_HOME`, defaulting historically to
  `~/.aegis`.
- `project-root/` represents a repository with project and nested
  instructions.

The daemon watcher proof should read these files from the fixture directory,
not from a real user home.
