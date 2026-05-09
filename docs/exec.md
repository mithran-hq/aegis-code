# Non-interactive mode

`aegis exec` runs Aegis Code without the TUI for scripts and CI.

Prompts can be passed as a positional argument, through stdin, or with `-` to
force stdin:

```bash
aegis exec "summarize the pending diff"
aegis exec - < prompt.txt
```

For CI runs that need method gates and evidence receipts, pass a method-state
artifact:

```bash
aegis exec \
  --method-state method-state.json \
  --method-state-output artifacts/method-state.json \
  --json \
  "implement the task and run local verification"
```

`--method-state` loads the JSON method state before the first turn. When the run
finishes, Aegis writes the updated state, including any evidence receipts, back
to the same file unless `--method-state-output` is provided.

Exit codes are stable for automation:

| Code | Classification      |
| ---- | ------------------- |
| 0    | success             |
| 20   | method gate failure |
| 21   | tool denial         |
| 22   | provider failure    |
| 23   | internal error      |

With `--json`, the stream includes Aegis preflight decisions and ends with an
`exec.completed` event containing the exit code, classification, thread id, turn
id, message, and method-state output path when present.
