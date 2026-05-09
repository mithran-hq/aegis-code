# Authentication

Aegis Code supports the inherited OpenAI login and API-key flows, plus
provider-specific environment variables for native Anthropic and local OSS
providers.

## OpenAI

Use the built-in `openai` provider with the Responses wire API:

```toml
model_provider = "openai"
model = "gpt-5.4"
```

Authenticate with a stored API key:

```bash
export OPENAI_API_KEY="..."
printenv OPENAI_API_KEY | aegis login --with-api-key
```

Or keep the key only in the environment:

```bash
export OPENAI_API_KEY="..."
aegis doctor
```

If `OPENAI_ORGANIZATION` or `OPENAI_PROJECT` is set, Aegis forwards those values
as provider headers. `aegis doctor` reports whether `OPENAI_API_KEY` is present,
but it does not print the value.

## Anthropic

Use the built-in native Anthropic provider:

```toml
model_provider = "anthropic"
model = "claude-sonnet-4-20250514"
```

Set the API key in the environment before starting Aegis:

```bash
export ANTHROPIC_API_KEY="..."
aegis doctor
```

See [Native Anthropic Provider](anthropic-provider.md) for supported behavior
and current limits.

## Custom Providers

Custom Responses-compatible providers use `env_key` to name the environment
variable that holds the bearer token:

```toml
[model_providers.openai-custom]
name = "OpenAI custom"
base_url = "https://api.openai.com/v1"
env_key = "OPENAI_API_KEY"
wire_api = "responses"
```

Aegis reads the named environment variable at runtime. Do not put literal API
keys in `~/.aegis/config.toml`.

## Local OSS Providers

Ollama and LM Studio do not require OpenAI login or API-key auth:

```bash
ollama serve
aegis --oss --local-provider ollama doctor
```

See [Local OSS Providers](local-oss-providers.md) for endpoint variables,
readiness checks, and default models.
