/*
This module implements the stdio-backed Aegis Agent Runtime client transport.

The v0 adapter intentionally speaks the same JSON-RPC envelope as the app-server
facade, one JSON message per stdout/stdin line. That keeps the transport small
while #23 defines the durable Aegis runtime event schema.
*/

use std::collections::HashMap;
use std::collections::VecDeque;
use std::io::Error as IoError;
use std::io::ErrorKind;
use std::io::Result as IoResult;
use std::process::Stdio;
use std::time::Duration;

use crate::AppServerEvent;
use crate::RequestResult;
use crate::SHUTDOWN_TIMEOUT;
use crate::TypedRequestError;
use crate::request_method_name;
use codex_app_server_protocol::ClientInfo;
use codex_app_server_protocol::ClientNotification;
use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::InitializeCapabilities;
use codex_app_server_protocol::InitializeParams;
use codex_app_server_protocol::JSONRPCError;
use codex_app_server_protocol::JSONRPCErrorError;
use codex_app_server_protocol::JSONRPCMessage;
use codex_app_server_protocol::JSONRPCNotification;
use codex_app_server_protocol::JSONRPCRequest;
use codex_app_server_protocol::JSONRPCResponse;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::Result as JsonRpcResult;
use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::ServerRequest;
use codex_app_server_protocol::WarningNotification;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::io::Lines;
use tokio::process::ChildStdin;
use tokio::process::ChildStdout;
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::time::timeout;
use tracing::warn;

const INITIALIZE_TIMEOUT: Duration = Duration::from_secs(10);
const CHECKPOINT_NOTIFICATION_METHOD: &str = "aegis/runtime/checkpoint";

#[derive(Debug, Clone)]
pub struct AegisRuntimeConnectArgs {
    pub command: Vec<String>,
    pub client_name: String,
    pub client_version: String,
    pub experimental_api: bool,
    pub opt_out_notification_methods: Vec<String>,
    pub channel_capacity: usize,
}

impl AegisRuntimeConnectArgs {
    fn initialize_params(&self) -> InitializeParams {
        let capabilities = InitializeCapabilities {
            experimental_api: self.experimental_api,
            opt_out_notification_methods: if self.opt_out_notification_methods.is_empty() {
                None
            } else {
                Some(self.opt_out_notification_methods.clone())
            },
        };

        InitializeParams {
            client_info: ClientInfo {
                name: self.client_name.clone(),
                title: None,
                version: self.client_version.clone(),
            },
            capabilities: Some(capabilities),
        }
    }
}

enum AegisRuntimeCommand {
    Request {
        request: Box<ClientRequest>,
        response_tx: oneshot::Sender<IoResult<RequestResult>>,
    },
    Notify {
        notification: ClientNotification,
        response_tx: oneshot::Sender<IoResult<()>>,
    },
    ResolveServerRequest {
        request_id: RequestId,
        result: JsonRpcResult,
        response_tx: oneshot::Sender<IoResult<()>>,
    },
    RejectServerRequest {
        request_id: RequestId,
        error: JSONRPCErrorError,
        response_tx: oneshot::Sender<IoResult<()>>,
    },
    Shutdown {
        response_tx: oneshot::Sender<IoResult<()>>,
    },
}

pub struct AegisRuntimeClient {
    command_tx: mpsc::Sender<AegisRuntimeCommand>,
    event_rx: mpsc::UnboundedReceiver<AppServerEvent>,
    pending_events: VecDeque<AppServerEvent>,
    worker_handle: tokio::task::JoinHandle<()>,
}

#[derive(Clone)]
pub struct AegisRuntimeRequestHandle {
    command_tx: mpsc::Sender<AegisRuntimeCommand>,
}

impl AegisRuntimeClient {
    pub async fn connect(args: AegisRuntimeConnectArgs) -> IoResult<Self> {
        let channel_capacity = args.channel_capacity.max(1);
        let command_display = command_display(&args.command);
        let (program, argv) = args.command.split_first().ok_or_else(|| {
            IoError::new(
                ErrorKind::InvalidInput,
                "aegis runtime command must not be empty",
            )
        })?;
        if program.trim().is_empty() || argv.iter().any(|part| part.trim().is_empty()) {
            return Err(IoError::new(
                ErrorKind::InvalidInput,
                "aegis runtime command must not contain empty argv elements",
            ));
        }

        let mut child = Command::new(program)
            .args(argv)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|err| {
                IoError::new(
                    err.kind(),
                    format!("failed to spawn Aegis Agent Runtime `{command_display}`: {err}"),
                )
            })?;
        let mut stdin = child.stdin.take().ok_or_else(|| {
            IoError::new(
                ErrorKind::BrokenPipe,
                format!("Aegis Agent Runtime `{command_display}` did not expose stdin"),
            )
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            IoError::new(
                ErrorKind::BrokenPipe,
                format!("Aegis Agent Runtime `{command_display}` did not expose stdout"),
            )
        })?;
        let mut stdout = BufReader::new(stdout).lines();

        let pending_events = initialize_runtime_connection(
            &mut stdin,
            &mut stdout,
            &command_display,
            args.initialize_params(),
            INITIALIZE_TIMEOUT,
        )
        .await?;

        let (command_tx, mut command_rx) = mpsc::channel::<AegisRuntimeCommand>(channel_capacity);
        let (event_tx, event_rx) = mpsc::unbounded_channel::<AppServerEvent>();
        let worker_handle = tokio::spawn(async move {
            let mut pending_requests =
                HashMap::<RequestId, oneshot::Sender<IoResult<RequestResult>>>::new();
            let mut worker_exit_error: Option<(ErrorKind, String)> = None;
            loop {
                tokio::select! {
                    command = command_rx.recv() => {
                        let Some(command) = command else {
                            let _ = child.start_kill();
                            let _ = child.wait().await;
                            break;
                        };
                        match command {
                            AegisRuntimeCommand::Request { request, response_tx } => {
                                let request_id = request_id_from_client_request(&request);
                                if pending_requests.contains_key(&request_id) {
                                    let _ = response_tx.send(Err(IoError::new(
                                        ErrorKind::InvalidInput,
                                        format!("duplicate Aegis runtime request id `{request_id}`"),
                                    )));
                                    continue;
                                }
                                pending_requests.insert(request_id.clone(), response_tx);
                                if let Err(err) = write_jsonrpc_message(
                                    &mut stdin,
                                    JSONRPCMessage::Request(jsonrpc_request_from_client_request(*request)),
                                    &command_display,
                                )
                                .await
                                {
                                    let err_message = err.to_string();
                                    let message = format!(
                                        "Aegis Agent Runtime `{command_display}` write failed: {err_message}"
                                    );
                                    if let Some(response_tx) = pending_requests.remove(&request_id) {
                                        let _ = response_tx.send(Err(err));
                                    }
                                    let _ = deliver_event(
                                        &event_tx,
                                        AppServerEvent::Disconnected {
                                            message: message.clone(),
                                        },
                                    );
                                    worker_exit_error = Some((ErrorKind::BrokenPipe, message));
                                    break;
                                }
                            }
                            AegisRuntimeCommand::Notify { notification, response_tx } => {
                                let result = write_jsonrpc_message(
                                    &mut stdin,
                                    JSONRPCMessage::Notification(
                                        jsonrpc_notification_from_client_notification(notification),
                                    ),
                                    &command_display,
                                )
                                .await;
                                let _ = response_tx.send(result);
                            }
                            AegisRuntimeCommand::ResolveServerRequest {
                                request_id,
                                result,
                                response_tx,
                            } => {
                                let result = write_jsonrpc_message(
                                    &mut stdin,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request_id,
                                        result,
                                    }),
                                    &command_display,
                                )
                                .await;
                                let _ = response_tx.send(result);
                            }
                            AegisRuntimeCommand::RejectServerRequest {
                                request_id,
                                error,
                                response_tx,
                            } => {
                                let result = write_jsonrpc_message(
                                    &mut stdin,
                                    JSONRPCMessage::Error(JSONRPCError {
                                        error,
                                        id: request_id,
                                    }),
                                    &command_display,
                                )
                                .await;
                                let _ = response_tx.send(result);
                            }
                            AegisRuntimeCommand::Shutdown { response_tx } => {
                                let _ = stdin.shutdown().await;
                                let wait_result = timeout(SHUTDOWN_TIMEOUT, child.wait())
                                    .await
                                    .map_err(|_| {
                                        let _ = child.start_kill();
                                        IoError::new(
                                            ErrorKind::TimedOut,
                                            format!("timed out waiting for Aegis Agent Runtime `{command_display}` to exit"),
                                        )
                                    })
                                    .and_then(|result| {
                                        result.map(|_| ()).map_err(|err| {
                                            IoError::new(
                                                err.kind(),
                                                format!("failed to wait for Aegis Agent Runtime `{command_display}`: {err}"),
                                            )
                                        })
                                    });
                                let _ = response_tx.send(wait_result);
                                break;
                            }
                        }
                    }
                    line = stdout.next_line() => {
                        match line {
                            Ok(Some(line)) => {
                                match serde_json::from_str::<JSONRPCMessage>(&line) {
                                    Ok(JSONRPCMessage::Response(response)) => {
                                        if let Some(response_tx) = pending_requests.remove(&response.id) {
                                            let _ = response_tx.send(Ok(Ok(response.result)));
                                        }
                                    }
                                    Ok(JSONRPCMessage::Error(error)) => {
                                        if let Some(response_tx) = pending_requests.remove(&error.id) {
                                            let _ = response_tx.send(Ok(Err(error.error)));
                                        }
                                    }
                                    Ok(JSONRPCMessage::Notification(notification)) => {
                                        if let Some(event) =
                                            app_server_event_from_notification(notification)
                                            && let Err(err) = deliver_event(&event_tx, event)
                                        {
                                            warn!(%err, "failed to deliver Aegis runtime event");
                                            break;
                                        }
                                    }
                                    Ok(JSONRPCMessage::Request(request)) => {
                                        let request_id = request.id.clone();
                                        let method = request.method.clone();
                                        match ServerRequest::try_from(request) {
                                            Ok(request) => {
                                                if let Err(err) = deliver_event(
                                                    &event_tx,
                                                    AppServerEvent::ServerRequest(request),
                                                )
                                                {
                                                    warn!(%err, "failed to deliver Aegis runtime server request");
                                                    break;
                                                }
                                            }
                                            Err(err) => {
                                                warn!(%err, method, "rejecting unknown Aegis runtime request");
                                                if let Err(reject_err) = write_jsonrpc_message(
                                                    &mut stdin,
                                                    JSONRPCMessage::Error(JSONRPCError {
                                                        error: JSONRPCErrorError {
                                                            code: -32601,
                                                            message: format!(
                                                                "unsupported Aegis runtime request `{method}`"
                                                            ),
                                                            data: None,
                                                        },
                                                        id: request_id,
                                                    }),
                                                    &command_display,
                                                )
                                                .await
                                                {
                                                    let err_message = reject_err.to_string();
                                                    let message = format!(
                                                        "Aegis Agent Runtime `{command_display}` write failed: {err_message}"
                                                    );
                                                    let _ = deliver_event(
                                                        &event_tx,
                                                        AppServerEvent::Disconnected {
                                                            message: message.clone(),
                                                        },
                                                    );
                                                    worker_exit_error =
                                                        Some((ErrorKind::BrokenPipe, message));
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                    Err(err) => {
                                        let message = format!(
                                            "Aegis Agent Runtime `{command_display}` sent invalid JSON-RPC: {err}"
                                        );
                                        let _ = deliver_event(
                                            &event_tx,
                                            AppServerEvent::Disconnected {
                                                message: message.clone(),
                                            },
                                        );
                                        worker_exit_error =
                                            Some((ErrorKind::InvalidData, message));
                                        break;
                                    }
                                }
                            }
                            Ok(None) => {
                                let message = format!(
                                    "Aegis Agent Runtime `{command_display}` closed stdout"
                                );
                                let _ = deliver_event(
                                    &event_tx,
                                    AppServerEvent::Disconnected {
                                        message: message.clone(),
                                    },
                                );
                                worker_exit_error = Some((ErrorKind::UnexpectedEof, message));
                                break;
                            }
                            Err(err) => {
                                let message = format!(
                                    "Aegis Agent Runtime `{command_display}` stdout failed: {err}"
                                );
                                let _ = deliver_event(
                                    &event_tx,
                                    AppServerEvent::Disconnected {
                                        message: message.clone(),
                                    },
                                );
                                worker_exit_error = Some((ErrorKind::InvalidData, message));
                                break;
                            }
                        }
                    }
                }
            }

            let (err_kind, err_message) = worker_exit_error.unwrap_or_else(|| {
                (
                    ErrorKind::BrokenPipe,
                    "Aegis runtime worker channel is closed".to_string(),
                )
            });
            for (_, response_tx) in pending_requests {
                let _ = response_tx.send(Err(IoError::new(err_kind, err_message.clone())));
            }
        });

        Ok(Self {
            command_tx,
            event_rx,
            pending_events: pending_events.into(),
            worker_handle,
        })
    }

    pub fn request_handle(&self) -> AegisRuntimeRequestHandle {
        AegisRuntimeRequestHandle {
            command_tx: self.command_tx.clone(),
        }
    }

    pub async fn request(&self, request: ClientRequest) -> IoResult<RequestResult> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(AegisRuntimeCommand::Request {
                request: Box::new(request),
                response_tx,
            })
            .await
            .map_err(|_| {
                IoError::new(
                    ErrorKind::BrokenPipe,
                    "Aegis runtime worker channel is closed",
                )
            })?;
        response_rx.await.map_err(|_| {
            IoError::new(
                ErrorKind::BrokenPipe,
                "Aegis runtime request channel is closed",
            )
        })?
    }

    pub async fn request_typed<T>(&self, request: ClientRequest) -> Result<T, TypedRequestError>
    where
        T: DeserializeOwned,
    {
        let method = request_method_name(&request);
        let response =
            self.request(request)
                .await
                .map_err(|source| TypedRequestError::Transport {
                    method: method.clone(),
                    source,
                })?;
        let result = response.map_err(|source| TypedRequestError::Server {
            method: method.clone(),
            source,
        })?;
        serde_json::from_value(result)
            .map_err(|source| TypedRequestError::Deserialize { method, source })
    }

    pub async fn notify(&self, notification: ClientNotification) -> IoResult<()> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(AegisRuntimeCommand::Notify {
                notification,
                response_tx,
            })
            .await
            .map_err(|_| {
                IoError::new(
                    ErrorKind::BrokenPipe,
                    "Aegis runtime worker channel is closed",
                )
            })?;
        response_rx.await.map_err(|_| {
            IoError::new(
                ErrorKind::BrokenPipe,
                "Aegis runtime notify channel is closed",
            )
        })?
    }

    pub async fn resolve_server_request(
        &self,
        request_id: RequestId,
        result: JsonRpcResult,
    ) -> IoResult<()> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(AegisRuntimeCommand::ResolveServerRequest {
                request_id,
                result,
                response_tx,
            })
            .await
            .map_err(|_| {
                IoError::new(
                    ErrorKind::BrokenPipe,
                    "Aegis runtime worker channel is closed",
                )
            })?;
        response_rx.await.map_err(|_| {
            IoError::new(
                ErrorKind::BrokenPipe,
                "Aegis runtime resolve channel is closed",
            )
        })?
    }

    pub async fn reject_server_request(
        &self,
        request_id: RequestId,
        error: JSONRPCErrorError,
    ) -> IoResult<()> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(AegisRuntimeCommand::RejectServerRequest {
                request_id,
                error,
                response_tx,
            })
            .await
            .map_err(|_| {
                IoError::new(
                    ErrorKind::BrokenPipe,
                    "Aegis runtime worker channel is closed",
                )
            })?;
        response_rx.await.map_err(|_| {
            IoError::new(
                ErrorKind::BrokenPipe,
                "Aegis runtime reject channel is closed",
            )
        })?
    }

    pub async fn next_event(&mut self) -> Option<AppServerEvent> {
        if let Some(event) = self.pending_events.pop_front() {
            return Some(event);
        }
        self.event_rx.recv().await
    }

    pub async fn shutdown(self) -> IoResult<()> {
        let Self {
            command_tx,
            event_rx,
            pending_events: _pending_events,
            worker_handle,
        } = self;
        let mut worker_handle = worker_handle;
        drop(event_rx);
        let (response_tx, response_rx) = oneshot::channel();
        if command_tx
            .send(AegisRuntimeCommand::Shutdown { response_tx })
            .await
            .is_ok()
            && let Ok(Ok(close_result)) = timeout(SHUTDOWN_TIMEOUT, response_rx).await
        {
            close_result?;
        }

        if let Err(_elapsed) = timeout(SHUTDOWN_TIMEOUT, &mut worker_handle).await {
            worker_handle.abort();
            let _ = worker_handle.await;
        }
        Ok(())
    }
}

impl AegisRuntimeRequestHandle {
    pub async fn request(&self, request: ClientRequest) -> IoResult<RequestResult> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(AegisRuntimeCommand::Request {
                request: Box::new(request),
                response_tx,
            })
            .await
            .map_err(|_| {
                IoError::new(
                    ErrorKind::BrokenPipe,
                    "Aegis runtime worker channel is closed",
                )
            })?;
        response_rx.await.map_err(|_| {
            IoError::new(
                ErrorKind::BrokenPipe,
                "Aegis runtime request channel is closed",
            )
        })?
    }

    pub async fn request_typed<T>(&self, request: ClientRequest) -> Result<T, TypedRequestError>
    where
        T: DeserializeOwned,
    {
        let method = request_method_name(&request);
        let response =
            self.request(request)
                .await
                .map_err(|source| TypedRequestError::Transport {
                    method: method.clone(),
                    source,
                })?;
        let result = response.map_err(|source| TypedRequestError::Server {
            method: method.clone(),
            source,
        })?;
        serde_json::from_value(result)
            .map_err(|source| TypedRequestError::Deserialize { method, source })
    }
}

async fn initialize_runtime_connection(
    stdin: &mut ChildStdin,
    stdout: &mut Lines<BufReader<ChildStdout>>,
    command_display: &str,
    params: InitializeParams,
    initialize_timeout: Duration,
) -> IoResult<Vec<AppServerEvent>> {
    let initialize_request_id = RequestId::String("initialize".to_string());
    let mut pending_events = Vec::new();
    write_jsonrpc_message(
        stdin,
        JSONRPCMessage::Request(jsonrpc_request_from_client_request(
            ClientRequest::Initialize {
                request_id: initialize_request_id.clone(),
                params,
            },
        )),
        command_display,
    )
    .await?;

    timeout(initialize_timeout, async {
        loop {
            match stdout.next_line().await {
                Ok(Some(line)) => {
                    let message = serde_json::from_str::<JSONRPCMessage>(&line).map_err(|err| {
                        IoError::other(format!(
                            "Aegis Agent Runtime `{command_display}` sent invalid initialize response: {err}"
                        ))
                    })?;
                    match message {
                        JSONRPCMessage::Response(response) if response.id == initialize_request_id => {
                            break Ok(());
                        }
                        JSONRPCMessage::Error(error) if error.id == initialize_request_id => {
                            break Err(IoError::other(format!(
                                "Aegis Agent Runtime `{command_display}` rejected initialize: {}",
                                error.error.message
                            )));
                        }
                        JSONRPCMessage::Notification(notification) => {
                            if let Some(event) = app_server_event_from_notification(notification) {
                                pending_events.push(event);
                            }
                        }
                        JSONRPCMessage::Request(request) => {
                            let request_id = request.id.clone();
                            let method = request.method.clone();
                            match ServerRequest::try_from(request) {
                                Ok(request) => {
                                    pending_events.push(AppServerEvent::ServerRequest(request));
                                }
                                Err(err) => {
                                    warn!(%err, method, "rejecting unknown Aegis runtime request during initialize");
                                    write_jsonrpc_message(
                                        stdin,
                                        JSONRPCMessage::Error(JSONRPCError {
                                            error: JSONRPCErrorError {
                                                code: -32601,
                                                message: format!(
                                                    "unsupported Aegis runtime request `{method}`"
                                                ),
                                                data: None,
                                            },
                                            id: request_id,
                                        }),
                                        command_display,
                                    )
                                    .await?;
                                }
                            }
                        }
                        JSONRPCMessage::Response(_) | JSONRPCMessage::Error(_) => {}
                    }
                }
                Ok(None) => {
                    break Err(IoError::new(
                        ErrorKind::UnexpectedEof,
                        format!("Aegis Agent Runtime `{command_display}` closed stdout during initialize"),
                    ));
                }
                Err(err) => {
                    break Err(IoError::other(format!(
                        "Aegis Agent Runtime `{command_display}` stdout failed during initialize: {err}"
                    )));
                }
            }
        }
    })
    .await
    .map_err(|_| {
        IoError::new(
            ErrorKind::TimedOut,
            format!("timed out waiting for initialize response from `{command_display}`"),
        )
    })??;

    write_jsonrpc_message(
        stdin,
        JSONRPCMessage::Notification(jsonrpc_notification_from_client_notification(
            ClientNotification::Initialized,
        )),
        command_display,
    )
    .await?;

    Ok(pending_events)
}

fn app_server_event_from_notification(notification: JSONRPCNotification) -> Option<AppServerEvent> {
    if notification.method == CHECKPOINT_NOTIFICATION_METHOD {
        return Some(AppServerEvent::ServerNotification(
            ServerNotification::Warning(WarningNotification {
                thread_id: checkpoint_thread_id(notification.params.as_ref()),
                message: checkpoint_message(notification.params.as_ref()),
            }),
        ));
    }
    match ServerNotification::try_from(notification) {
        Ok(notification) => Some(AppServerEvent::ServerNotification(notification)),
        Err(_) => None,
    }
}

fn checkpoint_thread_id(params: Option<&Value>) -> Option<String> {
    params?
        .get("threadId")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn checkpoint_message(params: Option<&Value>) -> String {
    let Some(params) = params else {
        return "Aegis runtime checkpoint received".to_string();
    };
    let checkpoint_id = params
        .get("checkpointId")
        .or_else(|| params.get("id"))
        .and_then(Value::as_str);
    let label = params.get("label").and_then(Value::as_str);
    match (checkpoint_id, label) {
        (Some(id), Some(label)) => format!("Aegis runtime checkpoint `{id}`: {label}"),
        (Some(id), None) => format!("Aegis runtime checkpoint `{id}` received"),
        (None, Some(label)) => format!("Aegis runtime checkpoint received: {label}"),
        (None, None) => "Aegis runtime checkpoint received".to_string(),
    }
}

fn deliver_event(
    event_tx: &mpsc::UnboundedSender<AppServerEvent>,
    event: AppServerEvent,
) -> IoResult<()> {
    event_tx.send(event).map_err(|_| {
        IoError::new(
            ErrorKind::BrokenPipe,
            "Aegis runtime event consumer channel is closed",
        )
    })
}

fn request_id_from_client_request(request: &ClientRequest) -> RequestId {
    jsonrpc_request_from_client_request(request.clone()).id
}

fn jsonrpc_request_from_client_request(request: ClientRequest) -> JSONRPCRequest {
    let value = match serde_json::to_value(request) {
        Ok(value) => value,
        Err(err) => panic!("client request should serialize: {err}"),
    };
    match serde_json::from_value(value) {
        Ok(request) => request,
        Err(err) => panic!("client request should encode as JSON-RPC request: {err}"),
    }
}

fn jsonrpc_notification_from_client_notification(
    notification: ClientNotification,
) -> JSONRPCNotification {
    let value = match serde_json::to_value(notification) {
        Ok(value) => value,
        Err(err) => panic!("client notification should serialize: {err}"),
    };
    match serde_json::from_value(value) {
        Ok(notification) => notification,
        Err(err) => panic!("client notification should encode as JSON-RPC notification: {err}"),
    }
}

async fn write_jsonrpc_message(
    stdin: &mut ChildStdin,
    message: JSONRPCMessage,
    command_display: &str,
) -> IoResult<()> {
    let payload = serde_json::to_string(&message).map_err(IoError::other)?;
    stdin.write_all(payload.as_bytes()).await.map_err(|err| {
        IoError::other(format!(
            "failed to write JSON-RPC message to Aegis Agent Runtime `{command_display}`: {err}"
        ))
    })?;
    stdin.write_all(b"\n").await.map_err(|err| {
        IoError::other(format!(
            "failed to write newline to Aegis Agent Runtime `{command_display}`: {err}"
        ))
    })?;
    stdin.flush().await.map_err(|err| {
        IoError::other(format!(
            "failed to flush Aegis Agent Runtime `{command_display}` stdin: {err}"
        ))
    })
}

fn command_display(command: &[String]) -> String {
    command.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_app_server_protocol::GetAccountParams;
    use codex_app_server_protocol::GetAccountResponse;
    use pretty_assertions::assert_eq;
    use std::fs;
    use std::path::Path;

    fn test_connect_args(command: Vec<String>) -> AegisRuntimeConnectArgs {
        AegisRuntimeConnectArgs {
            command,
            client_name: "aegis-runtime-test".to_string(),
            client_version: "0.0.0".to_string(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
            channel_capacity: 8,
        }
    }

    fn write_fake_runtime_script(dir: &Path, name: &str, body: &str) -> Vec<String> {
        let path = dir.join(name);
        fs::write(&path, body).expect("write fake runtime script");
        vec!["/bin/sh".to_string(), path.to_string_lossy().to_string()]
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn stdio_runtime_typed_request_roundtrip_works() {
        let tempdir = tempfile::tempdir().expect("create temp dir");
        let command = write_fake_runtime_script(
            tempdir.path(),
            "roundtrip.sh",
            r#"#!/bin/sh
read _initialize
printf '%s\n' '{"id":"initialize","result":{}}'
read _initialized
read _request
printf '%s\n' '{"id":1,"result":{"account":null,"requiresOpenaiAuth":false}}'
while read _line; do :; done
"#,
        );
        let client = AegisRuntimeClient::connect(test_connect_args(command))
            .await
            .expect("runtime client should connect");

        let response: GetAccountResponse = client
            .request_typed(ClientRequest::GetAccount {
                request_id: RequestId::Integer(1),
                params: GetAccountParams {
                    refresh_token: false,
                },
            })
            .await
            .expect("typed request should succeed");
        assert_eq!(response.account, None);
        assert!(!response.requires_openai_auth);
        client.shutdown().await.expect("shutdown should succeed");
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn stdio_runtime_invalid_json_surfaces_disconnect_event() {
        let tempdir = tempfile::tempdir().expect("create temp dir");
        let command = write_fake_runtime_script(
            tempdir.path(),
            "invalid-json.sh",
            r#"#!/bin/sh
read _initialize
printf '%s\n' '{"id":"initialize","result":{}}'
read _initialized
printf '%s\n' 'not json'
while read _line; do :; done
"#,
        );
        let mut client = AegisRuntimeClient::connect(test_connect_args(command))
            .await
            .expect("runtime client should connect");

        let event = client
            .next_event()
            .await
            .expect("disconnect event should be delivered");
        match event {
            AppServerEvent::Disconnected { message } => {
                assert!(message.contains("sent invalid JSON-RPC"));
            }
            other => panic!("expected disconnect event, got {other:?}"),
        }
        client.shutdown().await.expect("shutdown should succeed");
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn stdio_runtime_checkpoint_notification_maps_to_warning() {
        let tempdir = tempfile::tempdir().expect("create temp dir");
        let command = write_fake_runtime_script(
            tempdir.path(),
            "checkpoint.sh",
            r#"#!/bin/sh
read _initialize
printf '%s\n' '{"id":"initialize","result":{}}'
read _initialized
printf '%s\n' '{"method":"aegis/runtime/checkpoint","params":{"threadId":"thread-1","checkpointId":"checkpoint-1","label":"saved"}}'
while read _line; do :; done
"#,
        );
        let mut client = AegisRuntimeClient::connect(test_connect_args(command))
            .await
            .expect("runtime client should connect");

        let event = client
            .next_event()
            .await
            .expect("checkpoint event should be delivered");
        match event {
            AppServerEvent::ServerNotification(ServerNotification::Warning(warning)) => {
                assert_eq!(warning.thread_id.as_deref(), Some("thread-1"));
                assert_eq!(
                    warning.message,
                    "Aegis runtime checkpoint `checkpoint-1`: saved"
                );
            }
            other => panic!("expected warning notification, got {other:?}"),
        }
        client.shutdown().await.expect("shutdown should succeed");
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn stdio_runtime_server_request_resolution_roundtrip_works() {
        let tempdir = tempfile::tempdir().expect("create temp dir");
        let command = write_fake_runtime_script(
            tempdir.path(),
            "server-request.sh",
            r#"#!/bin/sh
read _initialize
printf '%s\n' '{"id":"initialize","result":{}}'
read _initialized
printf '%s\n' '{"id":"srv-1","method":"item/tool/requestUserInput","params":{"threadId":"thread-1","turnId":"turn-1","itemId":"call-1","questions":[{"id":"question-1","header":"Mode","question":"Pick one","isOther":false,"isSecret":false,"options":[]}]}}'
read _response
printf '%s\n' '{"method":"aegis/runtime/checkpoint","params":{"threadId":"thread-1","checkpointId":"resolved","label":"server request resolved"}}'
while read _line; do :; done
"#,
        );
        let mut client = AegisRuntimeClient::connect(test_connect_args(command))
            .await
            .expect("runtime client should connect");

        let event = client
            .next_event()
            .await
            .expect("server request should be delivered");
        let request_id = match event {
            AppServerEvent::ServerRequest(ServerRequest::ToolRequestUserInput {
                request_id,
                params,
            }) => {
                assert_eq!(params.thread_id, "thread-1");
                request_id
            }
            other => panic!("expected server request, got {other:?}"),
        };
        client
            .resolve_server_request(request_id, serde_json::json!({"answers": {}}))
            .await
            .expect("server request resolution should be sent");

        let event = tokio::time::timeout(Duration::from_secs(2), client.next_event())
            .await
            .expect("resolved checkpoint should arrive")
            .expect("resolved checkpoint should be delivered");
        match event {
            AppServerEvent::ServerNotification(ServerNotification::Warning(warning)) => {
                assert_eq!(
                    warning.message,
                    "Aegis runtime checkpoint `resolved`: server request resolved"
                );
            }
            other => panic!("expected resolved checkpoint warning, got {other:?}"),
        }
        client.shutdown().await.expect("shutdown should succeed");
    }

    #[tokio::test]
    async fn stdio_runtime_empty_command_is_rejected() {
        let err = match AegisRuntimeClient::connect(test_connect_args(Vec::new())).await {
            Ok(_) => panic!("empty command should fail"),
            Err(err) => err,
        };
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert!(err.to_string().contains("must not be empty"));
    }
}
