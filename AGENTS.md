# Aegis Code

Aegis Code is an Aegis-controlled coding agent harness derived from Codex.

## Development Commands

Before source import, use:

```bash
./scripts/ci_local.sh
```

After the scaffold lands, extend that script as the canonical local pre-push
check. Do not bypass it when the repo provides it.

## Method Rules

- Treat GitHub issues as the source of truth for implementation scope.
- Work from child task issues, not from the parent plan issue.
- Keep every implementation slice falsifiable: objective, scope, acceptance
  criteria, evidence, review, and closure.
- Preserve the boundary between Aegis Code, Aegis Secret, Aegis Engine, and
  Aegis Agent Runtime.
- Do not silently mutate active prompts from learned behavior. Learned behavior
  must become a promoted context pack before it affects a future session.
- Use Aegis Secret for wrapped tools such as `gh`, `gcloud`, `aws`, `kubectl`,
  and `terraform` when available.

## Git Policy

- Configure local commits in this repo as `Bruno Fernandez-Ruiz <b@mithran.ai>`.
- Never add `Co-Authored-By` trailers.
- Prefer issue-sized commits and pull requests.
- Run local CI before push.
- Preserve upstream Codex attribution and license obligations when source is
  imported.
