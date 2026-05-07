# Architecture

Aegis Code is a coding agent harness derived from Codex.

The harness owns the control surfaces that prompt-only methods cannot enforce:
method state, evidence receipts, sensitive tool mediation, sandbox posture,
provider routing, session resume validity, and asynchronous learning from
runtime events.

## Product Boundary

| Layer | Responsibility |
| --- | --- |
| Aegis Code | Coding loop, method gates, prompts, tools, evidence, review, session state |
| Aegis Secret | Authority decisions for sensitive local commands and secrets |
| Aegis Engine | Asynchronous event triage, drift intelligence, candidate context packs |
| Aegis Agent Runtime | Optional execution, sandbox, session, and tool substrate |

Aegis Code can integrate with the other Aegis projects, but it should not absorb
their product responsibilities.

## Method State

The method record is:

```text
Intent -> Claims -> Assumptions -> Falsifiers -> Evidence -> Gates -> Review -> Closure
```

The record must survive session resume. A completed statement only counts when
its falsifiers and evidence remain inspectable after the agent's short-term
memory has gone.

## Prompt And Context Layers

Prompt assembly should be deterministic:

1. built-in Aegis Code base guidance
2. user context pack
3. project context pack
4. promoted learned context pack
5. current task facts

Unpromoted candidate packs are visible to humans and tools, but they do not
change active behavior.

## Aegis Engine Loop

The v1 learning loop is asynchronous:

```text
aegis-code -> structured events -> aegis-daemon -> alerts/intelligence
  -> candidate context pack -> promotion gate -> future session
```

Runtime events can warn the current session, but persistent instruction changes
apply only after promotion and only at a new session or resume boundary.

## Sensitive Tools

Aegis Code should mediate configured sensitive commands through Aegis Secret
when available. The request should include argv, cwd, repo, task scope, current
method state, and the reason the command is sensitive.

## Provider Strategy

The first provider contract should preserve upstream OpenAI-compatible behavior.
Native Anthropic support is a first-class track because provider-native prompt
caching and tool semantics matter for production cost and correctness.
