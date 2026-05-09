# Method Workflow

Aegis Code makes coding-agent work falsifiable by carrying method state,
evidence, review, and closure through the normal coding loop. In this
repository, GitHub child task issues remain the source of truth for
implementation scope.

## Issue Train

The issue train has one parent plan issue and one implementation task issue for
each independently shippable slice. Validate the train before landing work:

```bash
aegis issue-train validate --repo mithran-hq/aegis-code --parent 1
```

The validator checks parent-child structure, task readiness, task scope, and
closure evidence expectations. It does not replace reading the child task issue;
the task issue remains the implementation contract.

## Method State

The method record follows this shape:

```text
Intent -> Claims -> Assumptions -> Falsifiers -> Evidence -> Gates -> Review -> Closure
```

The record is useful only when it survives memory loss and resume. A completed
claim should cite evidence, a falsifier should remain inspectable, and closure
should reference the successful evidence receipts that justify it.

Interactive sessions surface method status in the TUI. Non-interactive runs can
load and write a method-state artifact:

```bash
aegis exec \
  --method-state method-state.json \
  --method-state-output artifacts/method-state.json \
  --json \
  "implement the task and run local verification"
```

## Evidence Receipts

Evidence receipts record what command or artifact supports a claim. Command
receipts include the command, working directory, git state, exit status, output
summary, artifact references, session metadata, sandbox posture, and redaction
status.

Successful required evidence must have a successful receipt. Free-form notes are
not enough to close required evidence gates.

## Sensitive Commands

Aegis Code mediates configured sensitive local commands through Aegis Secret
when available. Typical sensitive commands include `gh`, `gcloud`, `aws`,
`kubectl`, and `terraform`.

The Aegis Secret policy request is context-only. It summarizes argv, cwd, repo,
task scope, method state, sandbox posture, risk, and expected evidence. It must
not include raw secrets, full prompts, full conversation history, raw
environment maps, or raw command output.

See [Aegis Secret Policy Contract](aegis-secret-policy.md).

## Adversarial Review

Before committing completed task work, run an adversarial review over the task
changes. The CLI supports review mode:

```bash
aegis review --uncommitted
```

Use review findings as blocking feedback until they are fixed or explicitly
shown to be non-blocking. Review findings can be recorded in method state and
used as closure evidence when the task is complete.

## PR Readiness

Before merging or closing a task, validate the PR or current branch against the
method-state artifact:

```bash
aegis pr-readiness validate \
  --repo mithran-hq/aegis-code \
  --method-state artifacts/method-state.json
```

Allowed path prefixes can narrow the expected change surface:

```bash
aegis pr-readiness validate \
  --repo mithran-hq/aegis-code \
  --method-state artifacts/method-state.json \
  --allowed-path docs/
```

The readiness check is a final consistency pass. It does not make an incomplete
issue complete; missing evidence, open falsifiers, or unresolved blocking review
findings should be fixed first.

## Closure

Close a task only after the completed work is landed according to repository
policy. Then reconcile the task issue, parent plan issue, and sibling issues
against the landed state. If a sibling issue was satisfied incidentally, close
it with evidence. If it is not clearly complete, leave it open and state the
remaining gap.
