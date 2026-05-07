# Distribution

Aegis Code should be easy to install locally and strict enough for team CI.

## Recommended v1

1. GitHub Releases with checksums and signed artifacts.
2. Homebrew formula under the Mithran tap.
3. npm wrapper that installs or launches the platform binary.
4. Source builds for contributors.

## Binary

The binary name is:

```bash
aegis-code
```

Release artifacts should cover at least:

- macOS arm64
- macOS x64
- Linux x64
- Linux arm64

## npm Wrapper

The npm package should be a thin installer or launcher. It must not become a
second implementation of the harness.

## Diagnostics

Every distribution path should support:

```bash
aegis-code doctor
```

The diagnostic command should report binary version, upstream Codex base, config
paths, provider configuration, Aegis Secret status, Aegis Engine sink status,
active context packs, and sandbox posture.
