# Context Packs

Context packs are TOML artifacts that carry reusable Aegis guidance into prompt
assembly. They define user guidance, project policy, or reviewed learned
behavior without letting a running session silently rewrite its own prompt.

V1 defines the schema contract only. Loading, validation, promotion, rollback,
and storage discovery are implemented by later tasks.

## Layer Model

Aegis Code assembles prompt context in this order:

1. built-in Aegis Code base guidance
2. user context pack
3. project context pack
4. promoted learned context pack
5. current task facts

Candidate learned packs are inspectable artifacts, but they do not affect active
prompt behavior until their own `promotion.status` is changed to `promoted` and
a future session or explicit resume boundary loads them.

## TOML Schema

Every context pack is a TOML document with `schema_version = 1`.

```toml
schema_version = 1
pack_id = "project:aegis-code"
kind = "project"
name = "Aegis Code project policy"
description = "Project-specific method, evidence, and tool guidance."

[compatibility]
aegis_code = ">=0.1.0"
schema = "1"

[scope]
repositories = ["mithran-hq/aegis-code"]
paths = ["."]

[[guidance]]
id = "guidance:issue-source-of-truth"
category = "method"
text = "Treat GitHub child task issues as the source of truth for implementation scope."

[[evidence.requirements]]
id = "evidence:local-ci"
description = "Run the repository local CI script before landing."
commands = ["./scripts/ci_local.sh"]

[tool_policy]
sensitive_commands = ["gh", "gcloud", "aws", "kubectl", "terraform"]
secret_broker = "aegis-secret"

[reviewer_checks]
required = ["adversarial-review", "issue-reconciliation"]

[provider_defaults]
preferred = "openai"
fallbacks = ["local"]

[provenance]
author = "project-maintainer"
source = "repository"
created_at = "2026-05-07T00:00:00Z"

[promotion]
status = "promoted"
promoted_at = "2026-05-07T00:00:00Z"
promoted_by = "project-maintainer"

[rollback]
previous_pack_id = ""
reason = ""
```

## Fields

`schema_version` is required and must be `1` for this schema. Future loaders
must reject missing or unsupported versions.

`pack_id` is required and must be stable across edits to the same logical pack.
Use prefixes such as `user:`, `project:`, or `learned:` to make ownership clear.

`kind` is required and must be one of `user`, `project`, or `learned`.

`compatibility` declares the Aegis Code and schema versions the pack targets.
For v1, `schema = "1"` means the pack expects the version 1 contract in this
document.

`scope` describes where the pack applies. User packs can use broad scope, project
packs should name repositories or paths, and learned packs should preserve the
scope that the evidence supports.

`guidance` entries carry model-visible instruction material. Each guidance entry
needs a stable `id`, a `category`, and `text`. Learned guidance must also include
`falsifiers`, so later reviewers can tell what evidence would invalidate it.

`evidence.requirements` entries describe proof expected during work, such as
tests, build commands, review steps, or closure evidence.

`tool_policy` names command policy and secret-broker requirements. It must not
contain raw API keys, tokens, passwords, or other secret values. Use policy names
or broker references instead.

`reviewer_checks` names review gates that should be run before closure.

`provider_defaults` captures preferred and fallback provider families. It is
policy input, not a runtime credential source.

`provenance` is required for learned packs and recommended for every pack. It
records who or what produced the pack, when, and from which source.

`promotion.status` is required and must be one of `candidate`, `promoted`, or
`retired`. Promotion state lives inside the artifact so it is not lost when packs
are copied, reviewed, or rolled back.

`rollback` records enough metadata to explain how to return from a promoted pack
to a prior state. It is required for promoted learned packs.

## Examples

### User Pack

```toml
schema_version = 1
pack_id = "user:bruno-method"
kind = "user"
name = "Bruno method preferences"
description = "Personal workflow preferences for Aegis Code sessions."

[compatibility]
aegis_code = ">=0.1.0"
schema = "1"

[scope]
users = ["bruno"]

[[guidance]]
id = "guidance:root-cause-first"
category = "method"
text = "Investigate root cause before committing to an implementation plan."

[[guidance]]
id = "guidance:task-sized-commits"
category = "delivery"
text = "Use one issue-sized commit for each completed child task."

[[evidence.requirements]]
id = "evidence:adversarial-review"
description = "Perform an adversarial review before committing task work."

[tool_policy]
sensitive_commands = ["gh"]
secret_broker = "aegis-secret"

[reviewer_checks]
required = ["adversarial-review"]

[provider_defaults]
preferred = "openai"
fallbacks = []

[provenance]
author = "bruno"
source = "user-authored"
created_at = "2026-05-07T00:00:00Z"

[promotion]
status = "promoted"
promoted_at = "2026-05-07T00:00:00Z"
promoted_by = "bruno"

[rollback]
previous_pack_id = ""
reason = ""
```

### Project Pack

```toml
schema_version = 1
pack_id = "project:aegis-code"
kind = "project"
name = "Aegis Code repository policy"
description = "Repository policy for implementing Aegis Code task issues."

[compatibility]
aegis_code = ">=0.1.0"
schema = "1"

[scope]
repositories = ["mithran-hq/aegis-code"]
paths = ["."]

[[guidance]]
id = "guidance:child-issues"
category = "method"
text = "Implement from child task issues, not from the parent plan issue."

[[guidance]]
id = "guidance:no-coauthor"
category = "git"
text = "Do not add Co-Authored-By trailers to commits."

[[evidence.requirements]]
id = "evidence:ci-local"
description = "Run local CI before pushing."
commands = ["./scripts/ci_local.sh"]

[[evidence.requirements]]
id = "evidence:issue-reconciled"
description = "Close completed child issues and update the parent plan after landing."

[tool_policy]
sensitive_commands = ["gh", "aws", "gcloud", "kubectl", "terraform"]
secret_broker = "aegis-secret"

[reviewer_checks]
required = ["adversarial-review", "issue-reconciliation"]

[provider_defaults]
preferred = "openai"
fallbacks = ["local"]

[provenance]
author = "project-maintainer"
source = "repository"
created_at = "2026-05-07T00:00:00Z"

[promotion]
status = "promoted"
promoted_at = "2026-05-07T00:00:00Z"
promoted_by = "project-maintainer"

[rollback]
previous_pack_id = ""
reason = ""
```

### Learned Candidate Pack

```toml
schema_version = 1
pack_id = "learned:aegis-code:issue-sized-delivery"
kind = "learned"
name = "Issue-sized delivery learning"
description = "Candidate learning derived from repeated successful task delivery."

[compatibility]
aegis_code = ">=0.1.0"
schema = "1"

[scope]
repositories = ["mithran-hq/aegis-code"]
paths = ["."]

[[guidance]]
id = "guidance:commit-after-each-task"
category = "delivery"
text = "Commit each completed child task immediately after verification and review."
falsifiers = [
  "Repository policy requires pull-request-only landing before any task commit can reach main.",
  "The task is explicitly split into multiple implementation issues before work starts.",
]

[[guidance]]
id = "guidance:update-parent-evidence"
category = "issue-reconciliation"
text = "After landing, update the parent plan issue with evidence that reflects the landed repository state."
falsifiers = [
  "The child task is closed by a repository automation that also updates parent evidence.",
  "The parent issue is retired or replaced before the task lands.",
]

[[evidence.requirements]]
id = "evidence:landed-commit"
description = "Record the landed commit hash in the child issue and parent plan."

[tool_policy]
sensitive_commands = ["gh"]
secret_broker = "aegis-secret"

[reviewer_checks]
required = ["adversarial-review", "redaction-check"]

[provider_defaults]
preferred = "openai"
fallbacks = []

[provenance]
author = "aegis-engine"
source = "rollout-evidence"
source_refs = ["thread:00000000-0000-4000-8000-000000000001"]
created_at = "2026-05-07T00:00:00Z"

[promotion]
status = "candidate"
review_required = true
promoted_at = ""
promoted_by = ""

[rollback]
previous_pack_id = "project:aegis-code"
reason = "Revert if candidate guidance conflicts with repository landing policy."
```

## Compatibility Rules

V1 packs must use `schema_version = 1` and `[compatibility].schema = "1"`.
Loaders introduced later must reject packs with missing versions, unsupported
major versions, or a `kind` value outside `user`, `project`, and `learned`.

Minor Aegis Code releases can add optional fields, but they must not change the
meaning of existing v1 fields. A future schema version is required for breaking
changes such as renaming fields, changing promotion status values, or changing
the required provenance rules for learned packs.

## Secret Handling

Context packs are guidance artifacts, not secret stores. They must not contain
raw credentials, bearer tokens, private keys, passwords, session cookies, or
provider API keys. Use references such as `secret_broker = "aegis-secret"` or
policy names that a later loader can resolve without exposing secret values.
