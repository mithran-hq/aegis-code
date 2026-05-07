use codex_shell_command::bash::parse_shell_lc_plain_commands;
use codex_shell_command::bash::parse_shell_lc_single_command_prefix;
use futures::future::BoxFuture;
use std::collections::BTreeSet;
use std::path::Path;
use tokio::process::Command;

const AEGIS_SECRET_EXE: &str = "aegis-secret";
const SENSITIVE_COMMANDS: [&str; 5] = ["gh", "gcloud", "aws", "kubectl", "terraform"];

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct SensitiveCommand {
    pub(crate) name: String,
    pub(crate) args: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) enum SensitiveCommandAnalysis {
    NotSensitive,
    Single(SensitiveCommand),
    Reject(String),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) enum AegisSecretMediation {
    NotSensitive,
    Mediate {
        command_name: String,
        command: Vec<String>,
    },
    Reject(String),
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum AegisSecretBrokerError {
    #[error("the '{executable}' CLI was not found on PATH")]
    Missing { executable: String },
    #[error("{message}")]
    Unavailable { message: String },
    #[error("{message}")]
    #[allow(dead_code)]
    Denied { message: String },
}

pub(crate) trait AegisSecretBroker: Send + Sync {
    fn wrapped_commands(&self) -> BoxFuture<'_, Result<BTreeSet<String>, AegisSecretBrokerError>>;

    fn mediated_command(&self, name: &str, args: &[String]) -> Vec<String>;
}

#[derive(Debug, Default)]
pub(crate) struct LocalAegisSecretBroker;

impl AegisSecretBroker for LocalAegisSecretBroker {
    fn wrapped_commands(&self) -> BoxFuture<'_, Result<BTreeSet<String>, AegisSecretBrokerError>> {
        Box::pin(async move {
            let output = Command::new(AEGIS_SECRET_EXE)
                .args(["command", "list"])
                .output()
                .await
                .map_err(|err| {
                    if err.kind() == std::io::ErrorKind::NotFound {
                        AegisSecretBrokerError::Missing {
                            executable: AEGIS_SECRET_EXE.to_string(),
                        }
                    } else {
                        AegisSecretBrokerError::Unavailable {
                            message: format!("failed to query Aegis Secret command list: {err}"),
                        }
                    }
                })?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let detail = if !stderr.is_empty() {
                    stderr
                } else if !stdout.is_empty() {
                    stdout
                } else {
                    format!("exited with status {}", output.status)
                };
                return Err(AegisSecretBrokerError::Unavailable {
                    message: format!("Aegis Secret command discovery failed: {detail}"),
                });
            }

            let stdout = String::from_utf8_lossy(&output.stdout);
            Ok(stdout
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(ToOwned::to_owned)
                .collect())
        })
    }

    fn mediated_command(&self, name: &str, args: &[String]) -> Vec<String> {
        let mut command = vec![
            AEGIS_SECRET_EXE.to_string(),
            "run".to_string(),
            name.to_string(),
            "--".to_string(),
        ];
        command.extend(args.iter().cloned());
        command
    }
}

pub(crate) fn analyze_sensitive_command(command: &[String]) -> SensitiveCommandAnalysis {
    let commands = commands_for_mediation(command);
    let mut sensitive_matches = commands
        .commands
        .iter()
        .filter_map(|command| sensitive_command_from_plain_command(command))
        .collect::<Vec<_>>();

    match sensitive_matches.len() {
        0 => {
            if let Some(name) = commands
                .commands
                .iter()
                .find_map(|command| unsupported_wrapper_sensitive_token(command))
                .or_else(|| unsupported_wrapper_sensitive_token(command))
            {
                return SensitiveCommandAnalysis::Reject(format!(
                    "Aegis Secret blocks direct execution because sensitive command '{name}' \
                     appears inside an unsupported command wrapper. Run '{name}' as its own tool \
                     call so Aegis Secret can mediate it."
                ));
            }
            SensitiveCommandAnalysis::NotSensitive
        }
        1 if commands.commands.len() == 1 => {
            SensitiveCommandAnalysis::Single(sensitive_matches.remove(0))
        }
        _ => {
            let name = sensitive_matches
                .first()
                .map(|command| command.name.as_str())
                .unwrap_or("sensitive command");
            SensitiveCommandAnalysis::Reject(format!(
                "Aegis Secret blocks direct execution because sensitive command '{name}' appears \
                 in a compound shell command. Run the sensitive command as its own tool call so \
                 Aegis Secret can mediate it."
            ))
        }
    }
}

pub(crate) async fn mediate_command(
    broker: &dyn AegisSecretBroker,
    command: &[String],
) -> AegisSecretMediation {
    let sensitive_command = match analyze_sensitive_command(command) {
        SensitiveCommandAnalysis::NotSensitive => return AegisSecretMediation::NotSensitive,
        SensitiveCommandAnalysis::Reject(message) => return AegisSecretMediation::Reject(message),
        SensitiveCommandAnalysis::Single(command) => command,
    };

    match broker.wrapped_commands().await {
        Ok(commands) if commands.contains(&sensitive_command.name) => {
            tracing::info!(
                command_name = %sensitive_command.name,
                "routing sensitive command through Aegis Secret"
            );
            AegisSecretMediation::Mediate {
                command_name: sensitive_command.name.clone(),
                command: broker.mediated_command(&sensitive_command.name, &sensitive_command.args),
            }
        }
        Ok(_) => AegisSecretMediation::Reject(format!(
            "Aegis Secret does not expose a wrapper for sensitive command '{}'; direct execution \
             is blocked.",
            sensitive_command.name
        )),
        Err(err) => AegisSecretMediation::Reject(format_broker_error(&sensitive_command.name, err)),
    }
}

fn format_broker_error(name: &str, err: AegisSecretBrokerError) -> String {
    match err {
        AegisSecretBrokerError::Missing { executable } => format!(
            "Aegis Secret is required to mediate sensitive command '{name}', but the '{executable}' \
             CLI was not found on PATH. Install or configure Aegis Secret before running '{name}'."
        ),
        AegisSecretBrokerError::Unavailable { message } => format!(
            "Aegis Secret is unavailable while checking sensitive command '{name}': {message}"
        ),
        AegisSecretBrokerError::Denied { message } => {
            format!("Aegis Secret denied sensitive command '{name}': {message}")
        }
    }
}

struct MediationCommands {
    commands: Vec<Vec<String>>,
}

fn commands_for_mediation(command: &[String]) -> MediationCommands {
    if let Some(commands) = parse_shell_lc_plain_commands(command)
        && !commands.is_empty()
    {
        return MediationCommands { commands };
    }

    if let Some(single_command) = parse_shell_lc_single_command_prefix(command) {
        return MediationCommands {
            commands: vec![single_command],
        };
    }

    MediationCommands {
        commands: vec![command.to_vec()],
    }
}

fn sensitive_command_from_plain_command(command: &[String]) -> Option<SensitiveCommand> {
    let (index, name) = effective_program(command)?;
    if !is_sensitive_command(&name) {
        return None;
    }

    Some(SensitiveCommand {
        name,
        args: command.iter().skip(index + 1).cloned().collect(),
    })
}

fn effective_program(command: &[String]) -> Option<(usize, String)> {
    let first = command.first()?;
    command_basename(first).map(|name| (0, name))
}

fn unsupported_wrapper_sensitive_token(command: &[String]) -> Option<String> {
    let wrapper = command.first().and_then(|arg| command_basename(arg))?;
    if !matches!(
        wrapper.as_str(),
        "env" | "sudo" | "command" | "nice" | "nohup" | "xargs"
    ) {
        return None;
    }

    command
        .iter()
        .skip(1)
        .filter(|arg| !arg.contains('=') && !arg.starts_with('-'))
        .filter_map(|arg| command_basename(arg))
        .find(|name| is_sensitive_command(name))
}

fn command_basename(command: &str) -> Option<String> {
    Path::new(command)
        .file_name()
        .and_then(|name| name.to_str())
        .map(ToOwned::to_owned)
}

fn is_sensitive_command(name: &str) -> bool {
    SENSITIVE_COMMANDS.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::FutureExt;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeBroker {
        commands: BTreeSet<String>,
        error: Option<AegisSecretBrokerError>,
        calls: Mutex<Vec<Vec<String>>>,
    }

    impl FakeBroker {
        fn exposing(commands: &[&str]) -> Self {
            Self {
                commands: commands.iter().map(|command| command.to_string()).collect(),
                error: None,
                calls: Mutex::new(Vec::new()),
            }
        }

        fn with_error(error: AegisSecretBrokerError) -> Self {
            Self {
                commands: BTreeSet::new(),
                error: Some(error),
                calls: Mutex::new(Vec::new()),
            }
        }
    }

    impl AegisSecretBroker for FakeBroker {
        fn wrapped_commands(
            &self,
        ) -> BoxFuture<'_, Result<BTreeSet<String>, AegisSecretBrokerError>> {
            async move {
                if let Some(error) = &self.error {
                    return Err(match error {
                        AegisSecretBrokerError::Missing { executable } => {
                            AegisSecretBrokerError::Missing {
                                executable: executable.clone(),
                            }
                        }
                        AegisSecretBrokerError::Unavailable { message } => {
                            AegisSecretBrokerError::Unavailable {
                                message: message.clone(),
                            }
                        }
                        AegisSecretBrokerError::Denied { message } => {
                            AegisSecretBrokerError::Denied {
                                message: message.clone(),
                            }
                        }
                    });
                }
                Ok(self.commands.clone())
            }
            .boxed()
        }

        fn mediated_command(&self, name: &str, args: &[String]) -> Vec<String> {
            let mut command = vec!["fake-aegis-secret".to_string(), name.to_string()];
            command.extend(args.iter().cloned());
            self.calls.lock().unwrap().push(command.clone());
            command
        }
    }

    #[tokio::test]
    async fn mediates_direct_sensitive_command() {
        let broker = FakeBroker::exposing(&["gh"]);
        let mediation = mediate_command(
            &broker,
            &["gh".to_string(), "issue".to_string(), "view".to_string()],
        )
        .await;

        assert_eq!(
            mediation,
            AegisSecretMediation::Mediate {
                command_name: "gh".to_string(),
                command: vec![
                    "fake-aegis-secret".to_string(),
                    "gh".to_string(),
                    "issue".to_string(),
                    "view".to_string()
                ]
            }
        );
    }

    #[tokio::test]
    async fn mediates_simple_shell_wrapped_sensitive_command() {
        let broker = FakeBroker::exposing(&["kubectl"]);
        let mediation = mediate_command(
            &broker,
            &[
                "bash".to_string(),
                "-lc".to_string(),
                "kubectl get pods".to_string(),
            ],
        )
        .await;

        assert_eq!(
            mediation,
            AegisSecretMediation::Mediate {
                command_name: "kubectl".to_string(),
                command: vec![
                    "fake-aegis-secret".to_string(),
                    "kubectl".to_string(),
                    "get".to_string(),
                    "pods".to_string()
                ]
            }
        );
    }

    #[tokio::test]
    async fn rejects_compound_sensitive_shell_command() {
        let broker = FakeBroker::exposing(&["gh"]);
        let mediation = mediate_command(
            &broker,
            &[
                "bash".to_string(),
                "-lc".to_string(),
                "gh issue view && echo done".to_string(),
            ],
        )
        .await;

        assert!(
            matches!(mediation, AegisSecretMediation::Reject(message) if message.contains("compound shell command"))
        );
        assert!(broker.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn rejects_when_wrapper_is_missing() {
        let broker = FakeBroker::exposing(&["aws"]);
        let mediation = mediate_command(&broker, &["gh".to_string(), "issue".to_string()]).await;

        assert!(
            matches!(mediation, AegisSecretMediation::Reject(message) if message.contains("does not expose a wrapper"))
        );
    }

    #[tokio::test]
    async fn reports_broker_denial_as_rejection() {
        let broker = FakeBroker::with_error(AegisSecretBrokerError::Denied {
            message: "Touch ID denied".to_string(),
        });
        let mediation =
            mediate_command(&broker, &["terraform".to_string(), "plan".to_string()]).await;

        assert!(
            matches!(mediation, AegisSecretMediation::Reject(message) if message.contains("denied sensitive command 'terraform'"))
        );
    }

    #[tokio::test]
    async fn leaves_non_sensitive_commands_alone() {
        let broker = FakeBroker::exposing(&["gh"]);
        let mediation = mediate_command(&broker, &["cargo".to_string(), "test".to_string()]).await;

        assert_eq!(mediation, AegisSecretMediation::NotSensitive);
        assert!(broker.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn allows_sensitive_names_as_plain_arguments() {
        let broker = FakeBroker::exposing(&["gh"]);
        let mediation = mediate_command(
            &broker,
            &["cargo".to_string(), "test".to_string(), "gh".to_string()],
        )
        .await;

        assert_eq!(mediation, AegisSecretMediation::NotSensitive);
        assert!(broker.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn rejects_sensitive_command_inside_unsupported_wrapper() {
        let broker = FakeBroker::exposing(&["gh"]);
        let mediation = mediate_command(
            &broker,
            &[
                "env".to_string(),
                "GITHUB_TOKEN=x".to_string(),
                "gh".to_string(),
                "issue".to_string(),
            ],
        )
        .await;

        assert!(
            matches!(mediation, AegisSecretMediation::Reject(message) if message.contains("unsupported command wrapper"))
        );
        assert!(broker.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn rejects_sensitive_command_inside_parsed_unsupported_wrapper() {
        let broker = FakeBroker::exposing(&["gh"]);
        let mediation = mediate_command(
            &broker,
            &[
                "bash".to_string(),
                "-lc".to_string(),
                "sudo gh issue view".to_string(),
            ],
        )
        .await;

        assert!(
            matches!(mediation, AegisSecretMediation::Reject(message) if message.contains("unsupported command wrapper"))
        );
        assert!(broker.calls.lock().unwrap().is_empty());
    }
}
