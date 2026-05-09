# Sample Configuration

This starter config is safe to paste into `~/.aegis/config.toml`. It does not
contain raw API keys or secrets.

```toml
# Select the built-in OpenAI Responses-compatible provider.
model_provider = "openai"
model = "gpt-5.4"

# Keep risky workflows inside the expected sandbox postures.
allowed_sandbox_modes = ["read-only", "workspace-write"]

# Optional local-provider default for `aegis --oss`.
oss_provider = "ollama"

# Context packs are explicit absolute paths. Only promoted packs affect prompt
# assembly, and candidate or retired packs are ignored fail-closed.
context_pack_paths = [
  "/Users/you/.aegis/context-packs/user-method.toml",
  "/Users/you/src/project/.aegis/project-policy.toml",
]

[aegis_engine]
enabled = true
failure_mode = "best-effort"
buffer_capacity = 256

[profiles.anthropic]
model_provider = "anthropic"
model = "claude-sonnet-4-20250514"

[profiles.local]
model_provider = "ollama"
model = "gpt-oss:20b"
oss_provider = "ollama"

[features.aegis_agent_runtime]
enabled = false
command = ["aegis-agent-runtime", "stdio"]
failure_mode = "fallback"
```

Set provider credentials outside the file:

```bash
export OPENAI_API_KEY="..."
export ANTHROPIC_API_KEY="..."
```

Then verify selection and secret presence without printing secret values:

```bash
aegis doctor
aegis --profile anthropic doctor
aegis --oss --local-provider ollama doctor
```
