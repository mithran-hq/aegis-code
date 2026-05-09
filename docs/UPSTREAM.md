# Upstream Sync Strategy

Aegis Code is a fork of OpenAI Codex. This document describes how to inspect,
plan, and perform future upstream sync work without losing Aegis-owned changes.

## Current Upstream Base

| Field                    | Value                                      |
| ------------------------ | ------------------------------------------ |
| Upstream repository      | `https://github.com/openai/codex`          |
| Upstream default branch  | `main`                                     |
| Local repository         | `https://github.com/mithran-hq/aegis-code` |
| Local default branch     | `main`                                     |
| Imported upstream commit | `f7e8ff8e5026f92fc4b0be1478bf98f7ffcdd781` |
| Import record            | `docs/UPSTREAM_IMPORT.md`                  |

The imported snapshot is not graph-connected to upstream history. Use the pinned
upstream commit above as the comparison base unless a future issue explicitly
rewrites or grafts history.

Expected remotes:

```bash
git remote get-url origin
git remote get-url upstream
git remote get-url --push upstream
```

`origin` should point at `https://github.com/mithran-hq/aegis-code.git`.
`upstream` should point at `https://github.com/openai/codex.git`.
Do not push to `upstream`; if needed, disable the upstream push URL locally with
`git remote set-url --push upstream DISABLED`.

## Read-Only Upstream Check

Use this check to see what has changed upstream. These commands fetch and
compare only; they do not merge, rebase, or edit files.

```bash
git fetch upstream main
git merge-base f7e8ff8e5026f92fc4b0be1478bf98f7ffcdd781 upstream/main
git log --oneline --decorate f7e8ff8e5026f92fc4b0be1478bf98f7ffcdd781..upstream/main
git diff --stat f7e8ff8e5026f92fc4b0be1478bf98f7ffcdd781..upstream/main
git diff --name-status f7e8ff8e5026f92fc4b0be1478bf98f7ffcdd781..upstream/main
git diff --stat b622d11..HEAD
```

Run the read-only check monthly, before release work, and before large Aegis
harness changes. If the diff is non-trivial, create or update a child GitHub
task issue before doing any sync work.

## Identifying Aegis-Owned Patches

Aegis-owned patches are local commits after the upstream import commit and any
future conflict-resolution commits made during sync work. Inspect them with:

```bash
git log --oneline b622d11..HEAD
git diff --name-status b622d11..HEAD
```

Use the Aegis-owned conflict areas below to decide whether a conflict should
keep local behavior, take upstream behavior, or become a documented follow-up
issue.

## Sync Branch Policy

Real sync work must happen in a short-lived child-task branch from current
`origin/main`:

```bash
git fetch origin main
git switch main
git pull --ff-only origin main
git switch -c upstream-sync/YYYY-MM-DD
git fetch upstream main
```

Use one child issue for each sync slice. Do not combine upstream sync with
unrelated Aegis feature work. If the upstream range is large enough that conflict
resolution becomes hard to review, split the work into multiple sync issues.

## Sync Application Policy

Because the import is a snapshot, do not run `git merge upstream/main` from
`main`; the histories do not share a local merge base. Apply the upstream
delta from the imported upstream commit to the chosen upstream target:

```bash
git diff --binary f7e8ff8e5026f92fc4b0be1478bf98f7ffcdd781..upstream/main > /tmp/aegis-upstream.patch
git apply --3way --index /tmp/aegis-upstream.patch
```

Resolve conflicts by preserving upstream behavior unless the file is in an
Aegis-owned area listed below. When conflicts touch Aegis-owned areas, keep the
Aegis product boundary intact and record the reason in the commit or issue
evidence. The sync commit message or issue evidence must include the upstream
commit range because upstream commits are applied as a delta rather than merged
as first-parent history. Do not erase OpenAI Codex attribution, Apache-2.0
notices, or upstream license history.

After resolving conflicts, run the repo's required local verification, including
`./scripts/ci_local.sh`, then land according to the repository issue workflow.

## Aegis-Owned Conflict Areas

Treat these surfaces as Aegis-owned during conflict resolution:

- public product identity: `Aegis Code`, `aegis`, `AEGIS_HOME`, `~/.aegis`,
  `@mithran/aegis`, and Aegis release artifact names
- root governance and planning files: `README.md`, `AGENTS.md`, `LICENSE`,
  `NOTICE`, `SECURITY.md`, issue templates, and pull request templates
- Aegis decision records and docs under `docs/adr/`, `docs/ARCHITECTURE.md`,
  `docs/IMPLEMENTATION_PLAN.md`, `docs/UPSTREAM*.md`, and distribution docs
- install, packaging, release, DotSlash, code-signing, and Homebrew/npm wrapper
  surfaces that expose Aegis names or repositories
- future Aegis method-state, evidence, context-pack, Aegis Secret, Aegis Engine,
  provider-routing, sandbox-policy, and runtime integration surfaces

Internal `codex-*` crate/module/protocol names may remain upstream-aligned unless
a child issue explicitly scopes a rename. Preserving those names is often better
for future upstream syncs.

## Release Branch Policy

Normal development and upstream sync work lands on `main`. Cut releases from
`main` after local and CI verification. Use `release/vX.Y.Z` only for
stabilization or hotfix work that must be isolated from ongoing development, and
merge any release fixes back to `main`.

## Sync Evidence

Every upstream sync issue should record:

- upstream commit range inspected
- merge base used for comparison
- command output summaries for the read-only check
- conflict files and whether each was upstream-owned or Aegis-owned
- verification commands and outcomes
- any follow-up issues for unresolved upstream drift
