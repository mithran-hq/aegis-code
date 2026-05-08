use crate::common::ResponseEvent;
use crate::common::ResponseStream;
use crate::error::ApiError;
use crate::telemetry::SseTelemetry;
use codex_client::ByteStream;
use codex_client::StreamResponse;
use codex_protocol::models::ContentItem;
use codex_protocol::models::MessagePhase;
use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::TokenUsage;
use eventsource_stream::Eventsource;
use futures::StreamExt;
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::Instant;
use tokio::time::timeout;
use tracing::debug;
use tracing::trace;

const REQUEST_ID_HEADER: &str = "request-id";

pub fn spawn_anthropic_messages_stream(
    stream_response: StreamResponse,
    idle_timeout: Duration,
    telemetry: Option<Arc<dyn SseTelemetry>>,
) -> ResponseStream {
    let upstream_request_id = stream_response
        .headers
        .get(REQUEST_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let (tx_event, rx_event) = mpsc::channel::<Result<ResponseEvent, ApiError>>(1600);
    tokio::spawn(process_anthropic_sse(
        stream_response.bytes,
        tx_event,
        idle_timeout,
        telemetry,
    ));

    ResponseStream {
        rx_event,
        upstream_request_id,
    }
}

#[derive(Debug, Default)]
struct AnthropicStreamState {
    response_id: Option<String>,
    text_by_index: BTreeMap<i64, String>,
    tool_by_index: BTreeMap<i64, PartialToolUse>,
    last_usage: Option<TokenUsage>,
}

#[derive(Debug, Clone)]
struct PartialToolUse {
    id: String,
    name: String,
    partial_json: String,
}

#[derive(Debug, Deserialize)]
struct AnthropicStreamEvent {
    #[serde(rename = "type")]
    kind: String,
    message: Option<AnthropicMessageStart>,
    index: Option<i64>,
    content_block: Option<AnthropicContentBlockStart>,
    delta: Option<AnthropicDelta>,
    usage: Option<AnthropicUsage>,
    error: Option<AnthropicErrorBody>,
}

#[derive(Debug, Deserialize)]
struct AnthropicMessageStart {
    id: String,
    usage: Option<AnthropicUsage>,
}

#[derive(Debug, Deserialize)]
struct AnthropicContentBlockStart {
    #[serde(rename = "type")]
    kind: String,
    id: Option<String>,
    name: Option<String>,
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnthropicDelta {
    #[serde(rename = "type")]
    kind: Option<String>,
    text: Option<String>,
    partial_json: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnthropicUsage {
    #[serde(default)]
    input_tokens: i64,
    #[serde(default)]
    cache_read_input_tokens: i64,
    #[serde(default)]
    cache_creation_input_tokens: i64,
    #[serde(default)]
    output_tokens: i64,
}

impl From<AnthropicUsage> for TokenUsage {
    fn from(value: AnthropicUsage) -> Self {
        let total_input =
            value.input_tokens + value.cache_read_input_tokens + value.cache_creation_input_tokens;
        TokenUsage {
            input_tokens: total_input,
            cached_input_tokens: value.cache_read_input_tokens,
            cache_creation_input_tokens: value.cache_creation_input_tokens,
            output_tokens: value.output_tokens,
            reasoning_output_tokens: 0,
            total_tokens: total_input + value.output_tokens,
        }
    }
}

#[derive(Debug, Deserialize)]
struct AnthropicErrorBody {
    #[serde(rename = "type")]
    kind: String,
    message: String,
}

pub async fn process_anthropic_sse(
    stream: ByteStream,
    tx_event: mpsc::Sender<Result<ResponseEvent, ApiError>>,
    idle_timeout: Duration,
    telemetry: Option<Arc<dyn SseTelemetry>>,
) {
    let mut stream = stream.eventsource();
    let mut state = AnthropicStreamState::default();

    loop {
        let start = Instant::now();
        let response = timeout(idle_timeout, stream.next()).await;
        if let Some(t) = telemetry.as_ref() {
            t.on_sse_poll(&response, start.elapsed());
        }
        let sse = match response {
            Ok(Some(Ok(sse))) => sse,
            Ok(Some(Err(e))) => {
                debug!("Anthropic SSE error: {e:#}");
                let _ = tx_event.send(Err(ApiError::Stream(e.to_string()))).await;
                return;
            }
            Ok(None) => {
                let _ = tx_event
                    .send(Err(ApiError::Stream(
                        "stream closed before message_stop".into(),
                    )))
                    .await;
                return;
            }
            Err(_) => {
                let _ = tx_event
                    .send(Err(ApiError::Stream("idle timeout waiting for SSE".into())))
                    .await;
                return;
            }
        };

        trace!("Anthropic SSE event: {}", &sse.data);
        let event: AnthropicStreamEvent = match serde_json::from_str(&sse.data) {
            Ok(event) => event,
            Err(e) => {
                debug!(
                    "failed to parse Anthropic SSE event: {e}, data: {}",
                    &sse.data
                );
                continue;
            }
        };

        match process_anthropic_event(event, &mut state) {
            Ok(events) => {
                for event in events {
                    let is_completed = matches!(event, ResponseEvent::Completed { .. });
                    if tx_event.send(Ok(event)).await.is_err() {
                        return;
                    }
                    if is_completed {
                        return;
                    }
                }
            }
            Err(err) => {
                let _ = tx_event.send(Err(err)).await;
                return;
            }
        }
    }
}

fn process_anthropic_event(
    event: AnthropicStreamEvent,
    state: &mut AnthropicStreamState,
) -> Result<Vec<ResponseEvent>, ApiError> {
    match event.kind.as_str() {
        "message_start" => {
            if let Some(message) = event.message {
                state.response_id = Some(message.id);
                state.last_usage = message.usage.map(Into::into);
            }
            Ok(vec![ResponseEvent::Created])
        }
        "content_block_start" => {
            let Some(index) = event.index else {
                return Ok(Vec::new());
            };
            let Some(block) = event.content_block else {
                return Ok(Vec::new());
            };
            match block.kind.as_str() {
                "text" => {
                    state
                        .text_by_index
                        .insert(index, block.text.unwrap_or_default());
                }
                "tool_use" => {
                    if let (Some(id), Some(name)) = (block.id, block.name) {
                        state.tool_by_index.insert(
                            index,
                            PartialToolUse {
                                id,
                                name,
                                partial_json: String::new(),
                            },
                        );
                    }
                }
                _ => {}
            }
            Ok(Vec::new())
        }
        "content_block_delta" => {
            let Some(index) = event.index else {
                return Ok(Vec::new());
            };
            let Some(delta) = event.delta else {
                return Ok(Vec::new());
            };
            match delta.kind.as_deref() {
                Some("text_delta") => {
                    let text = delta.text.unwrap_or_default();
                    state
                        .text_by_index
                        .entry(index)
                        .or_default()
                        .push_str(&text);
                    Ok(vec![ResponseEvent::OutputTextDelta(text)])
                }
                Some("input_json_delta") => {
                    if let Some(tool) = state.tool_by_index.get_mut(&index) {
                        tool.partial_json
                            .push_str(delta.partial_json.as_deref().unwrap_or_default());
                    }
                    Ok(Vec::new())
                }
                _ => Ok(Vec::new()),
            }
        }
        "content_block_stop" => {
            let Some(index) = event.index else {
                return Ok(Vec::new());
            };
            if let Some(tool) = state.tool_by_index.remove(&index) {
                let arguments = normalized_arguments(&tool.partial_json)?;
                let (namespace, name) = decode_anthropic_tool_name(&tool.name);
                return Ok(vec![ResponseEvent::OutputItemDone(
                    ResponseItem::FunctionCall {
                        id: None,
                        name,
                        namespace,
                        arguments,
                        call_id: tool.id,
                    },
                )]);
            }
            Ok(Vec::new())
        }
        "message_delta" => {
            if let Some(usage) = event.usage {
                state.last_usage = Some(usage.into());
            }
            Ok(Vec::new())
        }
        "message_stop" => {
            let mut events = Vec::new();
            let mut text = String::new();
            for chunk in state.text_by_index.values() {
                text.push_str(chunk);
            }
            if !text.is_empty() {
                events.push(ResponseEvent::OutputItemDone(ResponseItem::Message {
                    id: None,
                    role: "assistant".to_string(),
                    content: vec![ContentItem::OutputText { text }],
                    phase: Some(MessagePhase::FinalAnswer),
                }));
            }
            events.push(ResponseEvent::Completed {
                response_id: state
                    .response_id
                    .clone()
                    .unwrap_or_else(|| "anthropic-message".to_string()),
                token_usage: state.last_usage.clone(),
                end_turn: Some(true),
            });
            Ok(events)
        }
        "ping" => Ok(Vec::new()),
        "error" => {
            let Some(error) = event.error else {
                return Err(ApiError::Stream("Anthropic stream error".to_string()));
            };
            Err(map_anthropic_error(error))
        }
        _ => Ok(Vec::new()),
    }
}

fn normalized_arguments(partial_json: &str) -> Result<String, ApiError> {
    if partial_json.trim().is_empty() {
        return Ok("{}".to_string());
    }
    let value: Value = serde_json::from_str(partial_json).map_err(|err| {
        ApiError::Stream(format!("failed to parse Anthropic tool input JSON: {err}"))
    })?;
    Ok(value.to_string())
}

fn decode_anthropic_tool_name(name: &str) -> (Option<String>, String) {
    let Some(encoded) = name.strip_prefix("ns__") else {
        return (None, name.to_string());
    };
    if let Some((namespace, tool)) = encoded.rsplit_once("__")
        && !namespace.is_empty()
        && !tool.is_empty()
    {
        return (Some(namespace.to_string()), tool.to_string());
    }
    (None, name.to_string())
}

fn map_anthropic_error(error: AnthropicErrorBody) -> ApiError {
    match error.kind.as_str() {
        "invalid_request_error" => ApiError::InvalidRequest {
            message: error.message,
        },
        "rate_limit_error" => ApiError::RateLimit(error.message),
        "overloaded_error" => ApiError::ServerOverloaded,
        "timeout_error" => ApiError::Transport(codex_client::TransportError::Timeout),
        _ => ApiError::Stream(format!("Anthropic {}: {}", error.kind, error.message)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use codex_client::TransportError;
    use futures::stream;
    use pretty_assertions::assert_eq;

    fn event(data: &str) -> Bytes {
        Bytes::from(format!("event: message\ndata: {data}\n\n"))
    }

    async fn collect(chunks: Vec<Bytes>) -> Vec<Result<ResponseEvent, ApiError>> {
        let stream = stream::iter(chunks.into_iter().map(Ok::<_, TransportError>));
        let (tx, mut rx) = mpsc::channel::<Result<ResponseEvent, ApiError>>(16);
        tokio::spawn(process_anthropic_sse(
            Box::pin(stream),
            tx,
            Duration::from_secs(5),
            None,
        ));
        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            let done = matches!(event, Ok(ResponseEvent::Completed { .. }) | Err(_));
            events.push(event);
            if done {
                break;
            }
        }
        events
    }

    #[tokio::test]
    async fn parses_text_and_cache_usage() {
        let events = collect(vec![
            event(r#"{"type":"message_start","message":{"id":"msg_1","usage":{"input_tokens":10,"cache_read_input_tokens":20,"cache_creation_input_tokens":30,"output_tokens":1}}}"#),
            event(r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#),
            event(r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"hello"}}"#),
            event(r#"{"type":"content_block_stop","index":0}"#),
            event(r#"{"type":"message_delta","usage":{"input_tokens":10,"cache_read_input_tokens":20,"cache_creation_input_tokens":30,"output_tokens":2}}"#),
            event(r#"{"type":"message_stop"}"#),
        ])
        .await;

        assert!(matches!(events[0], Ok(ResponseEvent::Created)));
        assert!(matches!(
            events[1],
            Ok(ResponseEvent::OutputTextDelta(ref text)) if text == "hello"
        ));
        let Ok(ResponseEvent::Completed {
            token_usage: Some(usage),
            ..
        }) = events.last().expect("completed")
        else {
            panic!("expected completed usage");
        };
        assert_eq!(usage.input_tokens, 60);
        assert_eq!(usage.cached_input_tokens, 20);
        assert_eq!(usage.cache_creation_input_tokens, 30);
        assert_eq!(usage.output_tokens, 2);
        assert_eq!(usage.total_tokens, 62);
    }

    #[tokio::test]
    async fn parses_tool_use_partial_json_and_decodes_namespace() {
        let events = collect(vec![
            event(r#"{"type":"message_start","message":{"id":"msg_1"}}"#),
            event(r#"{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"toolu_1","name":"ns__mcp__demo____lookup"}}"#),
            event(r#"{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"q\":"}}"#),
            event(r#"{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"\"rust\"}"}}"#),
            event(r#"{"type":"content_block_stop","index":0}"#),
            event(r#"{"type":"message_stop"}"#),
        ])
        .await;

        let tool = events
            .iter()
            .find_map(|event| match event {
                Ok(ResponseEvent::OutputItemDone(ResponseItem::FunctionCall {
                    name,
                    namespace,
                    arguments,
                    call_id,
                    ..
                })) => Some((name, namespace, arguments, call_id)),
                _ => None,
            })
            .expect("tool call");

        assert_eq!(tool.0, "lookup");
        assert_eq!(tool.1.as_deref(), Some("mcp__demo__"));
        assert_eq!(tool.2, r#"{"q":"rust"}"#);
        assert_eq!(tool.3, "toolu_1");
    }
}
