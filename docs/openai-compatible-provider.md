# OpenAI-Compatible Providers

Aegis Code preserves the inherited OpenAI Responses-compatible provider path.
Use it for OpenAI itself, Azure OpenAI Responses endpoints, and custom services
that expose an OpenAI-compatible `POST /responses` streaming API.

## Built-In OpenAI

The built-in provider id is `openai` and uses the Responses wire API:

```toml
model_provider = "openai"
model = "gpt-5.4"
```

Authenticate with either a stored API key or the environment:

```bash
printenv OPENAI_API_KEY | aegis login --with-api-key
# or
export OPENAI_API_KEY="..."
```

If `OPENAI_ORGANIZATION` or `OPENAI_PROJECT` is set, Aegis forwards them as
OpenAI organization and project headers. The built-in OpenAI provider supports
Responses streaming, Responses WebSockets when enabled by the provider, hosted
tools, reasoning controls, text controls, and output schemas for models that
support them.

## Custom Responses-Compatible Providers

Add custom providers under `model_providers` in `~/.aegis/config.toml` and
select them with `model_provider`:

```toml
model_provider = "openai-custom"
model = "gpt-5.4"

[model_providers.openai-custom]
name = "OpenAI custom"
base_url = "https://api.openai.com/v1"
env_key = "OPENAI_API_KEY"
wire_api = "responses"
request_max_retries = 4
stream_max_retries = 10
stream_idle_timeout_ms = 300000
websocket_connect_timeout_ms = 15000
```

`env_key` is the name of the environment variable that contains the bearer
token. Aegis sends it as `Authorization: Bearer <value>` and does not persist or
print the value. Use `http_headers` for static provider headers and
`env_http_headers` for header values read from environment variables.

Azure-shaped Responses endpoints can add query parameters:

```toml
[model_providers.azure-responses]
name = "Azure"
base_url = "https://example.openai.azure.com/openai"
env_key = "AZURE_OPENAI_API_KEY"
wire_api = "responses"
query_params = { api-version = "2025-04-01-preview" }
```

## Diagnostics

Run `aegis doctor` to confirm the selected provider and model:

```bash
aegis doctor
aegis doctor --json
```

The provider section reports the selected provider id, display name, model,
wire API, base URL, websocket support, OpenAI-auth requirement, and configured
env var name. It reports whether the env var is present, but it never prints the
secret value.

## Boundaries

This page covers OpenAI Responses-compatible providers. Native Anthropic support
uses the built-in `anthropic` provider and is documented separately in
[Native Anthropic Provider](anthropic-provider.md). Local OSS provider setup is
tracked separately from this OpenAI-compatible path.
