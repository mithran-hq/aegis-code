# Sandbox Policy

Aegis Code reports sandbox posture in doctor output, TUI status diagnostics,
exec command events, Aegis Engine events, and method evidence receipts. The
posture includes sandbox mode, permission profile summary, enforcement kind, and
network posture.

## Modes

Aegis Code recognizes these sandbox modes for policy decisions:

- `read-only`
- `workspace-write`
- `danger-full-access`
- `external-sandbox`

The active mode comes from the normal CLI/config/session sandbox settings. Aegis
policy observes and reports that posture; it does not make `external-sandbox`
mean a specific capability set unless the external sandbox provider supplies
that guarantee.

## Managed Allow-List

Managed policy uses `allowed_sandbox_modes` as an allow-list:

```toml
allowed_sandbox_modes = ["read-only", "workspace-write"]
```

When an allow-list is configured, protected risky workflows are blocked if the
active sandbox mode is missing or outside the list. A command-level sandbox
override that would run without the sandbox is treated as `danger-full-access`
and is blocked unless that mode is explicitly allowed.

The allow-list is not an ordered minimum. For example, allowing
`workspace-write` does not automatically allow `danger-full-access`.

## Diagnostics

Run:

```bash
aegis doctor
```

The sandbox section reports:

- active sandbox mode
- permission profile
- enforcement kind
- network posture
- policy status
- allowed modes or `unrestricted`
- policy source and diagnostic, when present

Method evidence receipts and Aegis Engine sandbox events preserve the sandbox
posture that was active when a command or method-state update occurred.
