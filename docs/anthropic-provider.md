# Native Anthropic Provider

Aegis Code includes a built-in `anthropic` model provider that talks to
Anthropic's native Messages API instead of an OpenAI-compatible proxy.

Configure the provider by selecting it in `~/.aegis/config.toml` and exporting
an Anthropic API key:

```toml
model_provider = "anthropic"
model = "claude-sonnet-4-20250514"
```

```bash
export ANTHROPIC_API_KEY="..."
```

The built-in provider uses `https://api.anthropic.com/v1`, sends
`anthropic-version: 2023-06-01`, authenticates with `x-api-key`, and streams
turns through `POST /messages`. The static model catalog includes
`claude-sonnet-4-20250514`, `claude-opus-4-1-20250805`, and
`claude-3-5-haiku-20241022`, using Sonnet 4 as the default list priority.

The request adapter maps Aegis messages into Anthropic content blocks, including
system prompt blocks, text, image URL or base64 image inputs, tool definitions,
model tool calls, and tool results. The system prompt and final tool definition
include Anthropic ephemeral cache-control breakpoints, and returned usage keeps
both cache read and cache creation token counts.

Current limitations are deliberate. The native provider does not use Responses
WebSockets, OpenAI-hosted image generation, OpenAI web search, Responses output
schemas, remote compaction, or OpenAI-only text controls. Unsupported request
features should fail clearly instead of being passed through as lossy native
Anthropic requests.
