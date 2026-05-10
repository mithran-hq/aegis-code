# Distribution

Aegis Code should be easy to install locally and strict enough for team CI.

## Recommended v1

1. GitHub Releases with checksums and signed artifacts.
2. Homebrew cask under the Mithran tap for macOS.
3. npm wrapper that installs or launches the platform binary.
4. Source builds for contributors.

## Binary

The binary name is:

```bash
aegis
```

Release artifacts should cover at least:

- macOS arm64
- macOS x64
- Linux x64
- Linux arm64

The GitHub release workflow is the canonical v1 binary pipeline. A release tag
must use the `rust-vX.Y.Z` form and must match the Rust workspace version in
`codex-rs/Cargo.toml`. Stable releases use plain semantic versions; prereleases
may use `-alpha.N` or `-beta.N` suffixes.

The workflow also supports a dry-run path through manual `workflow_dispatch`.
Set `version` to the Cargo workspace version and leave `create_release` disabled.
The workflow builds the same artifacts and uploads them as a
`github-release-dry-run-<version>` workflow artifact instead of creating a
GitHub Release.

Each GitHub Release must include:

- `aegis-<target>.zst` and `aegis-<target>.tar.gz` for each supported target.
- `aegis-npm-<platform>-<version>.tgz` standalone installer packages consumed
  by `scripts/install/install.sh`.
- `SHA256SUMS` covering all published release assets.
- Linux `.sigstore` bundles when GitHub OIDC signing succeeds.
- macOS DMGs and, when Apple signing secrets are configured, signed and
  notarized macOS binaries and DMGs.
- An `*.unsigned.txt` explanation for any macOS target published without Apple
  signing credentials.

## npm Wrapper

The npm package should be a thin installer or launcher. It must not become a
second implementation of the harness.

## Homebrew Cask

The Homebrew path is a macOS cask named `aegis` in the Mithran tap:

```bash
brew tap mithran-hq/tap
brew install --cask aegis
```

The tap repository is `mithran-hq/homebrew-tap`. Its cask update workflow takes
an Aegis Code release version, downloads the matching `SHA256SUMS` from the
`rust-vX.Y.Z` GitHub Release, renders `Casks/aegis.rb`, validates the cask, and
commits the update to the tap. Checksums must come from the published release
assets rather than manual edits.

The cask installs the `aegis` executable from the macOS release tarballs. Linux
Homebrew support is not part of the v1 cask path; Linux users should use the
standalone installer or npm wrapper until a formula is added.

Until the first public `rust-vX.Y.Z` release is published and the tap workflow
is dispatched, the tap exists but `brew install --cask aegis` will not yet have
a committed cask to install.

## Diagnostics

Every distribution path should support:

```bash
aegis doctor
```

The diagnostic command should report binary version, upstream Codex base, config
paths, provider configuration, Aegis Secret status, Aegis Engine sink status,
active context packs, and sandbox posture.
