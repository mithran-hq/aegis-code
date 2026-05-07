# Contributing

Aegis Code uses issue-sized delivery.

## Workflow

1. Work from a child GitHub issue with objective, scope, acceptance criteria,
   dependencies, and falsifiers.
2. Keep commits focused on one delivery unit.
3. Run `./scripts/ci_local.sh` before push.
4. Open a pull request with a clear summary, test plan, and `Fixes #<issue>`.
5. Perform adversarial review before merge.

## Scope Discipline

If a task is too large, split it before implementation. If a side problem is
required to complete the current issue, include it. If it is not required,
capture it as a follow-up issue rather than expanding scope silently.

## Attribution

Do not add `Co-Authored-By` trailers. Preserve upstream license notices when
working with imported Codex source.
