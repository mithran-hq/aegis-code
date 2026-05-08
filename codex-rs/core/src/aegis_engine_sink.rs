use crate::config::AegisEngineConfig;
use crate::config::AegisEngineFailureMode;
use crate::config::AegisEngineMirrorConfig;
use codex_protocol::aegis_safety_event::AegisSafetyEvent;
use std::io;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::Mutex;
use tokio::fs::File;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::process::Child;
use tokio::process::Command;
use tokio::sync::mpsc;
#[cfg(test)]
use tokio::sync::oneshot;
use tracing::warn;

#[derive(Clone)]
pub(crate) struct AegisEngineSink {
    inner: Option<Arc<AegisEngineSinkInner>>,
}

struct AegisEngineSinkInner {
    sender: mpsc::Sender<AegisEngineSinkCommand>,
    terminal_failure: Mutex<Option<String>>,
    failure_mode: AegisEngineFailureMode,
}

enum AegisEngineSinkCommand {
    Event(AegisSafetyEvent),
    #[cfg(test)]
    Flush(oneshot::Sender<io::Result<()>>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AegisEngineSinkError {
    message: String,
}

impl std::fmt::Display for AegisEngineSinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for AegisEngineSinkError {}

impl AegisEngineSinkError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl AegisEngineSink {
    pub(crate) async fn start(config: &AegisEngineConfig) -> io::Result<(Self, Vec<String>)> {
        if !config.enabled {
            return Ok((Self::disabled(), Vec::new()));
        }

        let mut diagnostics = Vec::new();
        let jsonl = match open_append_create(&config.jsonl_path).await {
            Ok(file) => file,
            Err(err) if config.failure_mode == AegisEngineFailureMode::BestEffort => {
                diagnostics.push(format!(
                    "Aegis Engine event sink disabled: failed to open JSONL log `{}`: {err}",
                    config.jsonl_path.display()
                ));
                return Ok((Self::disabled(), diagnostics));
            }
            Err(err) => return Err(err),
        };

        let mirror = match open_mirror(&config.mirror).await {
            Ok(mirror) => mirror,
            Err(err) if config.failure_mode == AegisEngineFailureMode::BestEffort => {
                diagnostics.push(format!(
                    "Aegis Engine mirror disabled: failed to initialize mirror: {err}"
                ));
                None
            }
            Err(err) => return Err(err),
        };

        let (sender, receiver) = mpsc::channel(config.buffer_capacity);
        let inner = Arc::new(AegisEngineSinkInner {
            sender,
            terminal_failure: Mutex::new(None),
            failure_mode: config.failure_mode,
        });
        tokio::spawn(run_sink_worker(receiver, jsonl, mirror, Arc::clone(&inner)));
        Ok((Self { inner: Some(inner) }, diagnostics))
    }

    pub(crate) fn disabled() -> Self {
        Self { inner: None }
    }

    pub(crate) fn record(&self, event: AegisSafetyEvent) -> Result<(), AegisEngineSinkError> {
        let Some(inner) = self.inner.as_ref() else {
            return Ok(());
        };
        if let Some(failure) = inner.terminal_failure() {
            return Err(AegisEngineSinkError::new(format!(
                "Aegis Engine event sink is unavailable: {failure}"
            )));
        }
        inner
            .sender
            .try_send(AegisEngineSinkCommand::Event(event))
            .map_err(|err| match err {
                mpsc::error::TrySendError::Full(_) => {
                    AegisEngineSinkError::new("Aegis Engine event sink queue is full")
                }
                mpsc::error::TrySendError::Closed(_) => {
                    AegisEngineSinkError::new("Aegis Engine event sink worker stopped")
                }
            })
    }

    pub(crate) fn required(&self) -> bool {
        self.inner
            .as_ref()
            .is_some_and(|inner| inner.failure_mode == AegisEngineFailureMode::Require)
    }

    #[cfg(test)]
    pub(crate) async fn flush(&self) -> io::Result<()> {
        let Some(inner) = self.inner.as_ref() else {
            return Ok(());
        };
        let (tx, rx) = oneshot::channel();
        inner
            .sender
            .send(AegisEngineSinkCommand::Flush(tx))
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "sink worker stopped"))?;
        rx.await
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "sink worker stopped"))?
    }

    #[cfg(test)]
    fn with_sender_for_test(sender: mpsc::Sender<AegisEngineSinkCommand>) -> Self {
        Self {
            inner: Some(Arc::new(AegisEngineSinkInner {
                sender,
                terminal_failure: Mutex::new(None),
                failure_mode: AegisEngineFailureMode::BestEffort,
            })),
        }
    }
}

impl AegisEngineSinkInner {
    fn terminal_failure(&self) -> Option<String> {
        self.terminal_failure
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    fn mark_failed(&self, err: &io::Error) {
        let mut guard = self
            .terminal_failure
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if guard.is_none() {
            *guard = Some(err.to_string());
        }
    }
}

enum MirrorWriter {
    DaemonStdin {
        child: Child,
        stdin: tokio::process::ChildStdin,
    },
    Pipe(File),
}

async fn open_append_create(path: &Path) -> io::Result<File> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await
}

async fn open_mirror(config: &AegisEngineMirrorConfig) -> io::Result<Option<MirrorWriter>> {
    match config {
        AegisEngineMirrorConfig::None => Ok(None),
        AegisEngineMirrorConfig::DaemonStdin { command } => {
            let mut child = Command::new(&command[0])
                .args(&command[1..])
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
            let stdin = child.stdin.take().ok_or_else(|| {
                io::Error::new(io::ErrorKind::BrokenPipe, "daemon stdin was not available")
            })?;
            Ok(Some(MirrorWriter::DaemonStdin { child, stdin }))
        }
        AegisEngineMirrorConfig::Pipe { path } => {
            let file = OpenOptions::new().append(true).open(path).await?;
            Ok(Some(MirrorWriter::Pipe(file)))
        }
    }
}

async fn run_sink_worker(
    mut receiver: mpsc::Receiver<AegisEngineSinkCommand>,
    mut jsonl: File,
    mut mirror: Option<MirrorWriter>,
    inner: Arc<AegisEngineSinkInner>,
) {
    while let Some(command) = receiver.recv().await {
        match command {
            AegisEngineSinkCommand::Event(event) => {
                if let Err(err) = write_event(&mut jsonl, mirror.as_mut(), &event).await {
                    warn!(error = %err, "Aegis Engine event sink write failed");
                    inner.mark_failed(&err);
                    break;
                }
            }
            #[cfg(test)]
            AegisEngineSinkCommand::Flush(ack) => {
                let result = flush_writers(&mut jsonl, mirror.as_mut()).await;
                let _ = ack.send(result);
            }
        }
    }
}

async fn write_event(
    jsonl: &mut File,
    mirror: Option<&mut MirrorWriter>,
    event: &AegisSafetyEvent,
) -> io::Result<()> {
    let mut line = serde_json::to_vec(event).map_err(io::Error::other)?;
    line.push(b'\n');
    jsonl.write_all(&line).await?;
    jsonl.flush().await?;
    if let Some(mirror) = mirror {
        if let Err(err) = write_mirror(mirror, &line).await {
            warn!(error = %err, "Aegis Engine mirror write failed");
        }
    }
    Ok(())
}

async fn write_mirror(mirror: &mut MirrorWriter, line: &[u8]) -> io::Result<()> {
    match mirror {
        MirrorWriter::DaemonStdin { stdin, .. } => {
            stdin.write_all(line).await?;
            stdin.flush().await
        }
        MirrorWriter::Pipe(file) => {
            file.write_all(line).await?;
            file.flush().await
        }
    }
}

#[cfg(test)]
async fn flush_writers(jsonl: &mut File, mirror: Option<&mut MirrorWriter>) -> io::Result<()> {
    jsonl.flush().await?;
    if let Some(mirror) = mirror {
        match mirror {
            MirrorWriter::DaemonStdin { stdin, .. } => stdin.flush().await?,
            MirrorWriter::Pipe(file) => file.flush().await?,
        }
    }
    Ok(())
}

impl Drop for MirrorWriter {
    fn drop(&mut self) {
        if let MirrorWriter::DaemonStdin { child, .. } = self {
            let _ = child.start_kill();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_protocol::aegis_safety_event::AegisSafetyEventCategory;
    use codex_protocol::aegis_safety_event::AegisSafetySeverityHint;
    use tempfile::TempDir;

    fn test_event(summary: &str) -> AegisSafetyEvent {
        AegisSafetyEvent::new(
            AegisSafetyEventCategory::Runtime,
            AegisSafetySeverityHint::Info,
            summary.to_string(),
            vec!["test".to_string()],
            Default::default(),
            Vec::new(),
        )
    }

    #[tokio::test]
    async fn jsonl_sink_writes_valid_events() {
        let tmp = TempDir::new().unwrap();
        let config = AegisEngineConfig {
            enabled: true,
            jsonl_path: tmp.path().join("events.jsonl"),
            alerts_path: tmp.path().join("alerts.jsonl"),
            candidate_inputs_path: tmp.path().join("candidate-pack-inputs.jsonl"),
            alert_stale_after_seconds: 86_400,
            buffer_capacity: 8,
            failure_mode: AegisEngineFailureMode::BestEffort,
            mirror: AegisEngineMirrorConfig::None,
        };
        let (sink, diagnostics) = AegisEngineSink::start(&config).await.unwrap();
        assert!(diagnostics.is_empty());
        sink.record(test_event("runtime checkpoint")).unwrap();
        sink.flush().await.unwrap();

        let contents = tokio::fs::read_to_string(&config.jsonl_path).await.unwrap();
        let lines: Vec<_> = contents.lines().collect();
        assert_eq!(lines.len(), 1);
        let parsed: AegisSafetyEvent = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(parsed.summary, "runtime checkpoint");
    }

    #[tokio::test]
    async fn disabled_sink_is_noop() {
        let tmp = TempDir::new().unwrap();
        let config = AegisEngineConfig {
            enabled: false,
            jsonl_path: tmp.path().join("events.jsonl"),
            alerts_path: tmp.path().join("alerts.jsonl"),
            candidate_inputs_path: tmp.path().join("candidate-pack-inputs.jsonl"),
            alert_stale_after_seconds: 86_400,
            buffer_capacity: 8,
            failure_mode: AegisEngineFailureMode::BestEffort,
            mirror: AegisEngineMirrorConfig::None,
        };
        let (sink, diagnostics) = AegisEngineSink::start(&config).await.unwrap();
        assert!(diagnostics.is_empty());
        sink.record(test_event("ignored")).unwrap();
        assert!(!config.jsonl_path.exists());
    }

    #[tokio::test]
    async fn best_effort_start_failure_is_visible_and_non_fatal() {
        let tmp = TempDir::new().unwrap();
        let parent_file = tmp.path().join("not-a-dir");
        tokio::fs::write(&parent_file, "x").await.unwrap();
        let config = AegisEngineConfig {
            enabled: true,
            jsonl_path: parent_file.join("events.jsonl"),
            alerts_path: parent_file.join("alerts.jsonl"),
            candidate_inputs_path: parent_file.join("candidate-pack-inputs.jsonl"),
            alert_stale_after_seconds: 86_400,
            buffer_capacity: 8,
            failure_mode: AegisEngineFailureMode::BestEffort,
            mirror: AegisEngineMirrorConfig::None,
        };
        let (sink, diagnostics) = AegisEngineSink::start(&config).await.unwrap();
        assert_eq!(diagnostics.len(), 1);
        sink.record(test_event("dropped")).unwrap();
    }

    #[tokio::test]
    async fn required_start_failure_errors() {
        let tmp = TempDir::new().unwrap();
        let parent_file = tmp.path().join("not-a-dir");
        tokio::fs::write(&parent_file, "x").await.unwrap();
        let config = AegisEngineConfig {
            enabled: true,
            jsonl_path: parent_file.join("events.jsonl"),
            alerts_path: parent_file.join("alerts.jsonl"),
            candidate_inputs_path: parent_file.join("candidate-pack-inputs.jsonl"),
            alert_stale_after_seconds: 86_400,
            buffer_capacity: 8,
            failure_mode: AegisEngineFailureMode::Require,
            mirror: AegisEngineMirrorConfig::None,
        };
        assert!(AegisEngineSink::start(&config).await.is_err());
    }

    #[test]
    fn queue_overflow_is_reported() {
        let (sender, _receiver) = mpsc::channel(1);
        let sink = AegisEngineSink::with_sender_for_test(sender);
        sink.record(test_event("one")).unwrap();
        let err = sink.record(test_event("two")).unwrap_err();
        assert!(err.to_string().contains("queue is full"));
    }

    #[tokio::test]
    async fn pipe_mirror_receives_events() {
        let tmp = TempDir::new().unwrap();
        let pipe_path = tmp.path().join("mirror.jsonl");
        tokio::fs::write(&pipe_path, "").await.unwrap();
        let config = AegisEngineConfig {
            enabled: true,
            jsonl_path: tmp.path().join("events.jsonl"),
            alerts_path: tmp.path().join("alerts.jsonl"),
            candidate_inputs_path: tmp.path().join("candidate-pack-inputs.jsonl"),
            alert_stale_after_seconds: 86_400,
            buffer_capacity: 8,
            failure_mode: AegisEngineFailureMode::BestEffort,
            mirror: AegisEngineMirrorConfig::Pipe {
                path: pipe_path.clone(),
            },
        };
        let (sink, diagnostics) = AegisEngineSink::start(&config).await.unwrap();
        assert!(diagnostics.is_empty());
        sink.record(test_event("mirrored")).unwrap();
        sink.flush().await.unwrap();

        let contents = tokio::fs::read_to_string(pipe_path).await.unwrap();
        assert!(contents.contains("mirrored"));
    }
}
