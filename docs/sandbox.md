## Sandbox & approvals

For information about Aegis Code sandboxing and approvals, see [this documentation](https://developers.openai.com/codex/security).

## Aegis sandbox policy

Aegis Code reports the effective sandbox posture in doctor output, TUI status
diagnostics, exec command events, and method evidence receipts. The posture
includes the sandbox mode, permission profile summary, enforcement kind, and
network posture that were active for the command or method state update.

Managed policy uses the existing `allowed_sandbox_modes` requirement as an
allow-list. Supported values are `read-only`, `workspace-write`,
`danger-full-access`, and `external-sandbox`. When an allow-list is configured,
protected risky workflows are blocked if the active sandbox mode is missing or
outside the list. A command-level sandbox override that would run without the
sandbox is treated as `danger-full-access` and is blocked unless that mode is
explicitly allowed.

The policy intentionally does not define an ordering such as "minimum sandbox"
across all modes. In particular, `external-sandbox` means Aegis Code observed
external enforcement and reports it as such; it is not a guarantee about the
capabilities enforced by that external sandbox.
