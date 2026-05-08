# Provider Abstraction Review

This review records the provider seams that exist after the upstream source
import and before native provider work begins. It is scoped to issue #27 and is
intentionally docs-only: provider behavior should remain unchanged until the
follow-up implementation tasks land.

External Anthropic requirements were checked against the official Claude API
docs on 2026-05-08. The relevant surfaces are the Messages API, streaming
messages, tool use, prompt caching, token usage, and API errors:

- https://platform.claude.com/docs/en/api/messages
- https://platform.claude.com/docs/en/build-with-claude/streaming
- https://platform.claude.com/docs/en/agents-and-tools/tool-use/overview
- https://platform.claude.com/docs/en/build-with-claude/prompt-caching
- https://platform.claude.com/docs/en/api/errors
- https://platform.claude.com/docs/en/api/messages/count_tokens

## Current Provider Shape

Provider metadata lives in `codex-rs/model-provider-info/src/lib.rs`.
`ModelProviderInfo` describes a provider as a base URL plus auth, headers,
retry, timeout, WebSocket, and wire-protocol settings. Today `WireApi` has a
single supported value, `responses`; legacy `chat` config is rejected during
deserialization. Built-in providers are OpenAI, Amazon Bedrock, Ollama, and LM
Studio.

OpenAI is the primary first-party provider. It uses the Responses API, requires
OpenAI or ChatGPT auth, sends the package version header, supports OpenAI
organization and project env headers, and enables Responses-over-WebSocket.
Configured custom providers are merged with the built-ins as additional
OpenAI-compatible providers. Built-in providers are not generally overrideable;
Amazon Bedrock only allows `aws.profile` and `aws.region` to be changed.

Amazon Bedrock is not native Bedrock model invocation in this tree. The runtime
provider in `codex-rs/model-provider/src/amazon_bedrock/` targets Bedrock's
OpenAI-compatible Mantle endpoint, resolves AWS auth separately, exposes a
static model catalog, and disables namespace tools, image generation, and web
search through provider capabilities.

Ollama and LM Studio are local OSS providers created by
`create_oss_provider`. They use an OpenAI-compatible `/v1` base URL, default
ports `11434` and `1234`, optional `CODEX_OSS_PORT` and `CODEX_OSS_BASE_URL`
overrides, no auth, no WebSocket support, and the same `responses` wire shape as
remote compatible providers.

The runtime provider abstraction lives in
`codex-rs/model-provider/src/provider.rs`. `ModelProvider` currently adapts
metadata into a `codex_api::Provider`, an auth provider, an account state, a
capability set, and a model manager. It does not own model request construction,
stream parsing, tool conversion, or provider-native event semantics. Except for
the Amazon Bedrock special case, `ConfiguredModelProvider` is a generic
OpenAI-compatible adapter over `ModelProviderInfo`.

## Responses API Assumptions

Model execution is centered in `codex-rs/core/src/client.rs`.
`ModelClientSession::build_responses_request` directly constructs an OpenAI
Responses-shaped request from `Prompt`: `instructions`, `input`,
Responses-formatted tools, `tool_choice = "auto"`, `parallel_tool_calls`,
reasoning parameters, text/output-schema controls, `stream = true`,
`prompt_cache_key`, service tier, and client metadata. The Azure Responses path
is special-cased through `provider.is_azure_responses_endpoint()`.

The stream path switches only on `WireApi::Responses`. If WebSockets are enabled
for the provider and have not been disabled by fixture or fallback state, the
session uses the Responses WebSocket client; otherwise it uses HTTP SSE through
the Responses endpoint. Stream fallback, retry accounting, idle timeout, and
telemetry all assume Responses-style terminal events and provider retry
settings.

The wire structs and parsers are in `codex-rs/codex-api/src/`. The request,
response event, compaction, memories, realtime, SSE, and WebSocket modules are
all OpenAI/Responses-shaped. Usage accounting currently flows from Responses
events into token fields for input, cached input, output, reasoning output, and
total tokens. That is enough for OpenAI-compatible providers, but it is not a
provider-neutral usage model.

Tool specs are also Responses-shaped. `codex-rs/tools/src/tool_spec.rs`
serializes `ToolSpec` variants with OpenAI Responses `type` tags such as
`function`, `namespace`, `tool_search`, `local_shell`, `image_generation`,
`web_search`, and `custom`. There is no conversion layer for provider-native
tool schemas or provider-native tool-result content blocks.

Auth is provider metadata driven. `codex-rs/model-provider/src/auth.rs`
supports env-key bearer auth, experimental literal bearer tokens,
command-backed auth through `AuthManager::external_bearer_only`, first-party
Codex auth snapshots, and unauthenticated local/custom providers. Amazon
Bedrock adds AWS SigV4 or Bedrock bearer-token handling in its own module.

## Native Anthropic Requirements

Native Anthropic support needs a provider-native wire path instead of another
OpenAI-compatible provider entry. At minimum, the provider layer needs either a
new `WireApi` variant plus execution branches, or a higher-level request/stream
trait that lets a provider translate Aegis prompts into provider-native calls.
The implementation task should decide that shape in #28, but it must not depend
on Anthropic pretending to be OpenAI Responses.

Prompt conversion needs to map Aegis/Codex prompt data into the Anthropic
Messages API. That includes preserving the system prompt, user and assistant
message ordering, model selection, max token and reasoning-related settings
where supported, and any output constraints that have a safe native equivalent.
Unsupported OpenAI-only fields should fail clearly or be omitted deliberately.

Tool conversion must be explicit. Anthropic client tools use request-level tool
definitions, model-emitted `tool_use` content blocks, and caller-supplied
`tool_result` content blocks. Native support therefore needs round-trip tests
for tool definitions, tool-call IDs, JSON inputs, parallel tool use decisions,
tool results, and failure cases. OpenAI-only tool types such as namespace tools,
local shell, image generation, web search, and custom/freeform tools need a
capability decision before exposure.

Streaming requires a native Anthropic event parser. Anthropic streams use SSE
events such as `message_start`, `content_block_start`,
`content_block_delta`, `content_block_stop`, `message_delta`, `message_stop`,
`ping`, and in-stream `error`. Tool input can arrive as partial JSON deltas, so
the parser must accumulate tool-input fragments and emit stable internal tool
calls only after the block is complete.

Prompt caching must be modeled as Anthropic cache controls, not only as the
OpenAI `prompt_cache_key`. Anthropic caching can apply to tools, system, and
messages in that order, with explicit block-level breakpoints and current
automatic-cache behavior. The native provider must preserve cache read and
cache creation usage fields when returned, including `cache_read_input_tokens`
and `cache_creation_input_tokens`, so cost and evidence records do not discard
provider-native data.

Errors and diagnostics need provider-native mapping. Anthropic reports typed
JSON errors with request IDs and status classes such as invalid request,
authentication, permission, not found, request too large, rate limit, billing,
timeout, API, and overloaded. Streaming can surface errors after an HTTP 200,
so error mapping must cover both request setup failures and in-stream failures.

Model discovery and configuration need a native path. The current default model
manager assumes an OpenAI-compatible `/models` endpoint unless a static catalog
is supplied. Native Anthropic support should either provide a static catalog or
an Anthropic-specific discovery path, and diagnostics must show the selected
provider, model, endpoint family, and any provider capabilities disabled for
that model.

## Compatibility Risks And Tasks

| Risk | Task |
| --- | --- |
| Adding Anthropic as a custom OpenAI-compatible provider would leave message, tool, streaming, cache, usage, and error semantics lossy. | #28 Implement native Anthropic provider support. |
| Reworking request construction could regress inherited OpenAI Responses-compatible behavior, including config, env vars, streaming, tools, errors, and prompt formatting. | #29 Preserve OpenAI-compatible provider support. |
| Local providers may not support every Responses feature used by Aegis prompt layers, tools, diagnostics, or streaming paths. | #30 Preserve local OSS provider support. |
| Context packs and future policy could make provider selection opaque or override explicit CLI choices. | #31 Add provider routing policy. |
| Provider-native capabilities could expose unsupported tools or hide unsupported features unless diagnostics name the chosen provider, model, endpoint, and capability decisions. | #28, #29, #30, and #31 share this risk across their acceptance criteria. |

No additional implementation issue is required from this review. The known
compatibility risks are covered by #28 through #31. If implementation later
discovers a risk outside those task scopes, create or update the appropriate
GitHub task before expanding provider code.
