use clap::Args;
use clap::CommandFactory;
use clap::Parser;
use clap::ValueEnum;
use clap_complete::Shell;
use clap_complete::generate;
use codex_arg0::Arg0DispatchPaths;
use codex_arg0::arg0_dispatch_or_else;
use codex_chatgpt::apply_command::ApplyCommand;
use codex_chatgpt::apply_command::run_apply_command;
use codex_cli::LandlockCommand;
use codex_cli::SeatbeltCommand;
use codex_cli::WindowsCommand;
use codex_cli::read_access_token_from_stdin;
use codex_cli::read_api_key_from_stdin;
use codex_cli::run_login_status;
use codex_cli::run_login_with_access_token;
use codex_cli::run_login_with_api_key;
use codex_cli::run_login_with_chatgpt;
use codex_cli::run_login_with_device_code;
use codex_cli::run_logout;
use codex_cloud_tasks::Cli as CloudTasksCli;
use codex_core::context_packs::ContextPackDiagnostic;
use codex_core::context_packs::ContextPackDiagnosticStatus;
use codex_core::context_packs::ContextPackInspection;
use codex_core::context_packs::ContextPackKind;
use codex_core::context_packs::ContextPackLifecycleAction;
use codex_core::context_packs::ContextPackLifecycleResult;
use codex_core::context_packs::PromotionStatus;
use codex_core::context_packs::context_pack_lineage;
use codex_core::context_packs::inspect_context_pack_path;
use codex_core::context_packs::promote_context_pack;
use codex_core::context_packs::retire_context_pack;
use codex_core::context_packs::rollback_context_pack;
use codex_core::issue_train::FindingSeverity;
use codex_core::issue_train::IssueSnapshot;
use codex_core::issue_train::IssueState;
use codex_core::issue_train::IssueTrainReport;
use codex_core::issue_train::IssueTrainSnapshot;
use codex_core::issue_train::parse_parent_child_refs;
use codex_core::issue_train::validate_issue_train;
use codex_core::learned_pack_compiler::LearnedPackCompileResult;
use codex_core::learned_pack_compiler::LearnedPackCompilerOptions;
use codex_core::learned_pack_compiler::compile_learned_pack_candidates;
use codex_core::pr_readiness::PrReadinessReport;
use codex_core::pr_readiness::PrReadinessSnapshot;
use codex_core::pr_readiness::PullRequestSnapshot;
use codex_core::pr_readiness::parse_allowed_paths_from_pr_body;
use codex_core::pr_readiness::parse_closing_issue_refs;
use codex_core::pr_readiness::validate_pr_readiness;
use codex_exec::Cli as ExecCli;
use codex_exec::Command as ExecCommand;
use codex_exec::ReviewArgs;
use codex_execpolicy::ExecPolicyCheckCommand;
use codex_protocol::method_state::MethodState;
use codex_responses_api_proxy::Args as ResponsesApiProxyArgs;
use codex_rollout_trace::REDUCED_STATE_FILE_NAME;
use codex_rollout_trace::replay_bundle;
use codex_state::StateRuntime;
use codex_state::state_db_path;
use codex_tui::AppExitInfo;
use codex_tui::Cli as TuiCli;
use codex_tui::ExitReason;
use codex_tui::UpdateAction;
use codex_utils_absolute_path::AbsolutePathBuf;
use codex_utils_cli::CliConfigOverrides;
use owo_colors::OwoColorize;
use serde::Deserialize;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::collections::BTreeSet;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;
use supports_color::Stream;
use toml_edit::Array;
use toml_edit::Item as TomlItem;

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod app_cmd;
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod desktop_app;
mod marketplace_cmd;
mod mcp_cmd;
#[cfg(not(windows))]
mod wsl_paths;

use crate::marketplace_cmd::MarketplaceCli;
use crate::mcp_cmd::McpCli;

use codex_core::build_models_manager;
use codex_core::config::Config;
use codex_core::config::ConfigOverrides;
use codex_core::config::codex_import::CodexConfigImportOptions;
use codex_core::config::codex_import::CodexConfigImportReport;
use codex_core::config::codex_import::apply_codex_config_import;
use codex_core::config::codex_import::default_codex_config_path;
use codex_core::config::codex_import::preview_codex_config_import;
use codex_core::config::edit::ConfigEdit;
use codex_core::config::edit::ConfigEditsBuilder;
use codex_core::config::find_codex_home;
use codex_features::FEATURES;
use codex_features::Stage;
use codex_features::is_known_feature_key;
use codex_login::AuthManager;
use codex_memories_write::clear_memory_roots_contents;
use codex_models_manager::bundled_models_response;
use codex_models_manager::manager::RefreshStrategy;
use codex_protocol::protocol::AskForApproval;
use codex_protocol::user_input::UserInput;
use codex_terminal_detection::TerminalName;

/// Aegis Code CLI
///
/// If no subcommand is specified, options will be forwarded to the interactive CLI.
#[derive(Debug, Parser)]
#[clap(
    author,
    version,
    // If a sub‑command is given, ignore requirements of the default args.
    subcommand_negates_reqs = true,
    // The executable is sometimes invoked via a platform-specific name, but
    // help output should always use the generic command name that users run.
    bin_name = "aegis",
    override_usage = "aegis [OPTIONS] [PROMPT]\n       aegis [OPTIONS] <COMMAND> [ARGS]"
)]
struct MultitoolCli {
    #[clap(flatten)]
    pub config_overrides: CliConfigOverrides,

    #[clap(flatten)]
    pub feature_toggles: FeatureToggles,

    #[clap(flatten)]
    remote: InteractiveRemoteOptions,

    #[clap(flatten)]
    interactive: TuiCli,

    #[clap(subcommand)]
    subcommand: Option<Subcommand>,
}

#[derive(Debug, clap::Subcommand)]
enum Subcommand {
    /// Run Aegis Code non-interactively.
    #[clap(visible_alias = "e")]
    Exec(ExecCli),

    /// Run a code review non-interactively.
    Review(ReviewArgs),

    /// Manage login.
    Login(LoginCommand),

    /// Remove stored authentication credentials.
    Logout(LogoutCommand),

    /// Manage external MCP servers for Aegis Code.
    Mcp(McpCli),

    /// Manage Aegis Code plugins.
    Plugin(PluginCli),

    /// Start Aegis Code as an MCP server (stdio).
    McpServer,

    /// [experimental] Run the app server or related tooling.
    AppServer(AppServerCommand),

    /// Launch the Aegis desktop app (opens the app installer if missing).
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    App(app_cmd::AppCommand),

    /// Generate shell completion scripts.
    Completion(CompletionCommand),

    /// Update Aegis Code to the latest version.
    Update,

    /// Inspect local configuration and context pack status.
    Doctor(DoctorCommand),

    /// Manage Aegis Code configuration.
    Config(ConfigCommand),

    /// Validate parent-plan and child-task GitHub issue trains.
    IssueTrain(IssueTrainCommand),

    /// Validate pull request readiness against method evidence.
    PrReadiness(PrReadinessCommand),

    /// Manage configured context-pack lifecycle state.
    ContextPack(ContextPackCommand),

    /// Run commands within an Aegis-provided sandbox.
    Sandbox(SandboxArgs),

    /// Debugging tools.
    Debug(DebugCommand),

    /// Execpolicy tooling.
    #[clap(hide = true)]
    Execpolicy(ExecpolicyCommand),

    /// Apply the latest diff produced by Aegis as a `git apply` to your local working tree.
    #[clap(visible_alias = "a")]
    Apply(ApplyCommand),

    /// Resume a previous interactive session (picker by default; use --last to continue the most recent).
    Resume(ResumeCommand),

    /// Fork a previous interactive session (picker by default; use --last to fork the most recent).
    Fork(ForkCommand),

    /// [EXPERIMENTAL] Browse cloud tasks and apply changes locally.
    #[clap(name = "cloud", alias = "cloud-tasks")]
    Cloud(CloudTasksCli),

    /// Internal: run the responses API proxy.
    #[clap(hide = true)]
    ResponsesApiProxy(ResponsesApiProxyArgs),

    /// Internal: relay stdio to a Unix domain socket.
    #[clap(hide = true, name = "stdio-to-uds")]
    StdioToUds(StdioToUdsCommand),

    /// [EXPERIMENTAL] Run the standalone exec-server service.
    ExecServer(ExecServerCommand),

    /// Inspect feature flags.
    Features(FeaturesCli),
}

#[derive(Debug, Parser)]
#[command(bin_name = "aegis plugin")]
struct PluginCli {
    #[clap(flatten)]
    pub config_overrides: CliConfigOverrides,

    #[command(subcommand)]
    subcommand: PluginSubcommand,
}

#[derive(Debug, clap::Subcommand)]
enum PluginSubcommand {
    /// Manage plugin marketplaces for Aegis Code.
    Marketplace(MarketplaceCli),
}

#[derive(Debug, Parser)]
struct CompletionCommand {
    /// Shell to generate completions for
    #[clap(value_enum, default_value_t = Shell::Bash)]
    shell: Shell,
}

#[derive(Debug, Parser)]
struct DoctorCommand {
    /// Emit machine-readable JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Parser)]
#[command(bin_name = "aegis config")]
struct ConfigCommand {
    #[command(subcommand)]
    subcommand: ConfigSubcommand,
}

#[derive(Debug, clap::Subcommand)]
enum ConfigSubcommand {
    /// Preview or import safe settings from a Codex config.toml.
    ImportCodex(ConfigImportCodexCommand),
}

#[derive(Debug, Parser)]
struct ConfigImportCodexCommand {
    /// Source Codex config.toml. Defaults to ~/.codex/config.toml.
    #[arg(long = "from", value_name = "PATH")]
    source: Option<PathBuf>,

    /// Destination Aegis config.toml. Defaults to $AEGIS_HOME/config.toml.
    #[arg(long = "to", value_name = "PATH")]
    destination: Option<PathBuf>,

    /// Apply the import. Without this flag, only a preview is printed.
    #[arg(long)]
    apply: bool,

    /// Include literal prompt settings. Prompt file paths are never imported.
    #[arg(long = "include-prompts")]
    include_prompts: bool,

    /// Emit machine-readable JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Parser)]
#[command(bin_name = "aegis issue-train")]
struct IssueTrainCommand {
    #[command(subcommand)]
    subcommand: IssueTrainSubcommand,
}

#[derive(Debug, clap::Subcommand)]
enum IssueTrainSubcommand {
    /// Validate parent-plan and child-task issue readiness.
    Validate(IssueTrainValidateCommand),
}

#[derive(Debug, Clone, Parser)]
struct IssueTrainValidateCommand {
    /// GitHub repository in OWNER/REPO form. Defaults to the current repository.
    #[arg(long)]
    repo: Option<String>,

    /// Parent plan issue number. Defaults to the single open issue labeled aegis-code:plan.
    #[arg(long)]
    parent: Option<u64>,

    /// Emit machine-readable JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Parser)]
#[command(bin_name = "aegis pr-readiness")]
struct PrReadinessCommand {
    #[command(subcommand)]
    subcommand: PrReadinessSubcommand,
}

#[derive(Debug, clap::Subcommand)]
enum PrReadinessSubcommand {
    /// Validate a pull request before it closes a task issue.
    Validate(PrReadinessValidateCommand),
}

#[derive(Debug, Clone, Parser)]
struct PrReadinessValidateCommand {
    /// GitHub repository in OWNER/REPO form. Defaults to the current repository.
    #[arg(long)]
    repo: Option<String>,

    /// Pull request number. Defaults to the current branch's pull request.
    #[arg(long)]
    pr: Option<u64>,

    /// Method-state JSON artifact produced for this PR.
    #[arg(long = "method-state")]
    method_state: PathBuf,

    /// Allowed changed path prefix. May be provided multiple times.
    #[arg(long = "allowed-path")]
    allowed_paths: Vec<String>,

    /// Emit machine-readable JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Parser)]
#[command(bin_name = "aegis context-pack")]
struct ContextPackCommand {
    #[command(subcommand)]
    subcommand: ContextPackSubcommand,
}

#[derive(Debug, clap::Subcommand)]
enum ContextPackSubcommand {
    /// List configured context packs.
    List(ContextPackListCommand),
    /// Compile inactive learned candidate packs from Aegis Engine signals.
    CompileCandidates(ContextPackCompileCandidatesCommand),
    /// Inspect one configured context pack.
    Inspect(ContextPackInspectCommand),
    /// Promote a learned candidate pack.
    Promote(ContextPackPromoteCommand),
    /// Retire a configured context pack.
    Retire(ContextPackRetireCommand),
    /// Restore the prior promoted learned pack.
    Rollback(ContextPackRollbackCommand),
    /// Show learned-pack active lineage.
    Lineage(ContextPackLineageCommand),
}

#[derive(Debug, Parser)]
struct ContextPackListCommand {
    #[arg(long)]
    json: bool,
    #[arg(long, value_enum)]
    kind: Option<ContextPackKindArg>,
    #[arg(long, value_enum, default_value_t = ContextPackStatusArg::All)]
    status: ContextPackStatusArg,
}

#[derive(Debug, Parser)]
struct ContextPackCompileCandidatesCommand {
    /// Aegis Engine runtime event JSONL path.
    #[arg(long)]
    events: Option<PathBuf>,
    /// Aegis Engine candidate-pack input JSONL path.
    #[arg(long = "alert-inputs")]
    alert_inputs: Option<PathBuf>,
    /// Directory where candidate context-pack TOML files are written.
    #[arg(long = "output-dir")]
    output_dir: Option<PathBuf>,
    /// Minimum repeated evidence refs required before a candidate is emitted.
    #[arg(long = "min-support", default_value_t = 2)]
    min_support: usize,
    /// Emit machine-readable JSON.
    #[arg(long)]
    json: bool,
    /// Do not write candidate files or register context-pack paths.
    #[arg(long)]
    dry_run: bool,
    /// Write candidates without adding them to context_pack_paths.
    #[arg(long = "no-register")]
    no_register: bool,
}

#[derive(Debug, Serialize)]
struct LearnedPackCompileCliResult {
    compile: LearnedPackCompileResult,
    registered_paths: Vec<String>,
}

#[derive(Debug, Parser)]
struct ContextPackInspectCommand {
    selector: String,
    #[arg(long)]
    json: bool,
    #[arg(long)]
    show_guidance: bool,
}

#[derive(Debug, Parser)]
struct ContextPackPromoteCommand {
    selector: String,
    #[arg(long = "evidence", required = true, num_args = 1..)]
    evidence: Vec<String>,
    #[arg(long)]
    actor: Option<String>,
    #[arg(long)]
    reason: Option<String>,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Parser)]
struct ContextPackRetireCommand {
    selector: String,
    #[arg(long)]
    reason: String,
    #[arg(long)]
    actor: Option<String>,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Parser)]
struct ContextPackRollbackCommand {
    selector: Option<String>,
    #[arg(long)]
    reason: String,
    #[arg(long)]
    actor: Option<String>,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Parser)]
struct ContextPackLineageCommand {
    selector: Option<String>,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum ContextPackKindArg {
    User,
    Project,
    Learned,
}

impl From<ContextPackKindArg> for ContextPackKind {
    fn from(value: ContextPackKindArg) -> Self {
        match value {
            ContextPackKindArg::User => ContextPackKind::User,
            ContextPackKindArg::Project => ContextPackKind::Project,
            ContextPackKindArg::Learned => ContextPackKind::Learned,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum ContextPackStatusArg {
    Candidate,
    Promoted,
    Retired,
    Invalid,
    Unreadable,
    All,
}

#[derive(Debug, Parser)]
struct DebugCommand {
    #[command(subcommand)]
    subcommand: DebugSubcommand,
}

#[derive(Debug, clap::Subcommand)]
enum DebugSubcommand {
    /// Render the raw model catalog as JSON.
    Models(DebugModelsCommand),

    /// Tooling: helps debug the app server.
    AppServer(DebugAppServerCommand),

    /// Render the model-visible prompt input list as JSON.
    PromptInput(DebugPromptInputCommand),

    /// Render the redacted active prompt layer list as JSON.
    PromptLayers(DebugPromptLayersCommand),

    /// Replay a rollout trace bundle and write reduced state JSON.
    #[clap(hide = true)]
    TraceReduce(DebugTraceReduceCommand),

    /// Internal: reset local memory state for a fresh start.
    #[clap(hide = true)]
    ClearMemories,
}

#[derive(Debug, Parser)]
struct DebugAppServerCommand {
    #[command(subcommand)]
    subcommand: DebugAppServerSubcommand,
}

#[derive(Debug, clap::Subcommand)]
enum DebugAppServerSubcommand {
    // Send message to app server V2.
    SendMessageV2(DebugAppServerSendMessageV2Command),
}

#[derive(Debug, Parser)]
struct DebugAppServerSendMessageV2Command {
    #[arg(value_name = "USER_MESSAGE", required = true)]
    user_message: String,
}

#[derive(Debug, Parser)]
struct DebugPromptInputCommand {
    /// Optional user prompt to append after session context.
    #[arg(value_name = "PROMPT")]
    prompt: Option<String>,

    /// Optional image(s) to attach to the user prompt.
    #[arg(long = "image", short = 'i', value_name = "FILE", value_delimiter = ',', num_args = 1..)]
    images: Vec<PathBuf>,
}

#[derive(Debug, Parser)]
struct DebugPromptLayersCommand {
    /// Optional user prompt to resolve the same session context as prompt-input.
    #[arg(value_name = "PROMPT")]
    prompt: Option<String>,

    /// Optional image(s) to attach to the user prompt.
    #[arg(long = "image", short = 'i', value_name = "FILE", value_delimiter = ',', num_args = 1..)]
    images: Vec<PathBuf>,
}

#[derive(Debug, Parser)]
struct DebugModelsCommand {
    /// Skip refresh and dump only the bundled catalog shipped with this binary.
    #[arg(long = "bundled", default_value_t = false)]
    bundled: bool,
}

#[derive(Debug, Parser)]
struct DebugTraceReduceCommand {
    /// Trace bundle directory containing manifest.json and trace.jsonl.
    #[arg(value_name = "TRACE_BUNDLE")]
    trace_bundle: PathBuf,

    /// Output path for reduced RolloutTrace JSON. Defaults to TRACE_BUNDLE/state.json.
    #[arg(long = "output", short = 'o', value_name = "FILE")]
    output: Option<PathBuf>,
}

#[derive(Debug, Parser)]
struct ResumeCommand {
    /// Conversation/session id (UUID) or thread name. UUIDs take precedence if it parses.
    /// If omitted, use --last to pick the most recent recorded session.
    #[arg(value_name = "SESSION_ID")]
    session_id: Option<String>,

    /// Continue the most recent session without showing the picker.
    #[arg(long = "last", default_value_t = false)]
    last: bool,

    /// Show all sessions (disables cwd filtering and shows CWD column).
    #[arg(long = "all", default_value_t = false)]
    all: bool,

    /// Include non-interactive sessions in the resume picker and --last selection.
    #[arg(long = "include-non-interactive", default_value_t = false)]
    include_non_interactive: bool,

    #[clap(flatten)]
    remote: InteractiveRemoteOptions,

    #[clap(flatten)]
    config_overrides: TuiCli,
}

#[derive(Debug, Parser)]
struct ForkCommand {
    /// Conversation/session id (UUID). When provided, forks this session.
    /// If omitted, use --last to pick the most recent recorded session.
    #[arg(value_name = "SESSION_ID")]
    session_id: Option<String>,

    /// Fork the most recent session without showing the picker.
    #[arg(long = "last", default_value_t = false, conflicts_with = "session_id")]
    last: bool,

    /// Show all sessions (disables cwd filtering and shows CWD column).
    #[arg(long = "all", default_value_t = false)]
    all: bool,

    #[clap(flatten)]
    remote: InteractiveRemoteOptions,

    #[clap(flatten)]
    config_overrides: TuiCli,
}

#[derive(Debug, Parser)]
struct SandboxArgs {
    #[command(subcommand)]
    cmd: SandboxCommand,
}

#[derive(Debug, clap::Subcommand)]
enum SandboxCommand {
    /// Run a command under Seatbelt (macOS only).
    #[clap(visible_alias = "seatbelt")]
    Macos(SeatbeltCommand),

    /// Run a command under the Linux sandbox (bubblewrap by default).
    #[clap(visible_alias = "landlock")]
    Linux(LandlockCommand),

    /// Run a command under Windows restricted token (Windows only).
    Windows(WindowsCommand),
}

#[derive(Debug, Parser)]
struct ExecpolicyCommand {
    #[command(subcommand)]
    sub: ExecpolicySubcommand,
}

#[derive(Debug, clap::Subcommand)]
enum ExecpolicySubcommand {
    /// Check execpolicy files against a command.
    #[clap(name = "check")]
    Check(ExecPolicyCheckCommand),
}

#[derive(Debug, Parser)]
struct LoginCommand {
    #[clap(skip)]
    config_overrides: CliConfigOverrides,

    #[arg(
        long = "with-api-key",
        help = "Read the API key from stdin (e.g. `printenv OPENAI_API_KEY | aegis login --with-api-key`)"
    )]
    with_api_key: bool,

    #[arg(
        long = "with-access-token",
        help = "Read the access token from stdin (e.g. `printenv CODEX_ACCESS_TOKEN | aegis login --with-access-token`)"
    )]
    with_access_token: bool,

    #[arg(
        long = "api-key",
        num_args = 0..=1,
        default_missing_value = "",
        value_name = "API_KEY",
        help = "(deprecated) Previously accepted the API key directly; now exits with guidance to use --with-api-key",
        hide = true
    )]
    api_key: Option<String>,

    #[arg(long = "device-auth")]
    use_device_code: bool,

    /// EXPERIMENTAL: Use custom OAuth issuer base URL (advanced)
    /// Override the OAuth issuer base URL (advanced)
    #[arg(long = "experimental_issuer", value_name = "URL", hide = true)]
    issuer_base_url: Option<String>,

    /// EXPERIMENTAL: Use custom OAuth client ID (advanced)
    #[arg(long = "experimental_client-id", value_name = "CLIENT_ID", hide = true)]
    client_id: Option<String>,

    #[command(subcommand)]
    action: Option<LoginSubcommand>,
}

#[derive(Debug, clap::Subcommand)]
enum LoginSubcommand {
    /// Show login status.
    Status,
}

#[derive(Debug, Parser)]
struct LogoutCommand {
    #[clap(skip)]
    config_overrides: CliConfigOverrides,
}

#[derive(Debug, Parser)]
struct AppServerCommand {
    /// Omit to run the app server; specify a subcommand for tooling.
    #[command(subcommand)]
    subcommand: Option<AppServerSubcommand>,

    /// Transport endpoint URL. Supported values: `stdio://` (default),
    /// `unix://`, `unix://PATH`, `ws://IP:PORT`, `off`.
    #[arg(
        long = "listen",
        value_name = "URL",
        default_value = codex_app_server::AppServerTransport::DEFAULT_LISTEN_URL
    )]
    listen: codex_app_server::AppServerTransport,

    /// Controls whether analytics are enabled by default.
    ///
    /// Analytics are disabled by default for app-server. Users have to explicitly opt in
    /// via the `analytics` section in the config.toml file.
    ///
    /// However, for first-party use cases like the VSCode IDE extension, we default analytics
    /// to be enabled by default by setting this flag. Users can still opt out by setting this
    /// in their config.toml:
    ///
    /// ```toml
    /// [analytics]
    /// enabled = false
    /// ```
    ///
    /// See https://developers.openai.com/codex/config-advanced/#metrics for more details.
    #[arg(long = "analytics-default-enabled")]
    analytics_default_enabled: bool,

    #[command(flatten)]
    auth: codex_app_server::AppServerWebsocketAuthArgs,
}

#[derive(Debug, Parser)]
struct ExecServerCommand {
    /// Transport endpoint URL. Supported values: `ws://IP:PORT` (default), `stdio`, `stdio://`.
    #[arg(long = "listen", value_name = "URL", conflicts_with = "remote")]
    listen: Option<String>,

    /// Register this exec-server as a remote executor using the given base URL.
    #[arg(long = "remote", value_name = "URL", requires = "executor_id")]
    remote: Option<String>,

    /// Executor id to attach to when registering remotely.
    #[arg(long = "executor-id", value_name = "ID")]
    executor_id: Option<String>,

    /// Human-readable executor name.
    #[arg(long = "name", value_name = "NAME")]
    name: Option<String>,
}

#[derive(Debug, clap::Subcommand)]
#[allow(clippy::enum_variant_names)]
enum AppServerSubcommand {
    /// Proxy stdio bytes to the running app-server control socket.
    Proxy(AppServerProxyCommand),

    /// [experimental] Generate TypeScript bindings for the app server protocol.
    GenerateTs(GenerateTsCommand),

    /// [experimental] Generate JSON Schema for the app server protocol.
    GenerateJsonSchema(GenerateJsonSchemaCommand),

    /// [internal] Generate internal JSON Schema artifacts for Codex tooling.
    #[clap(hide = true)]
    GenerateInternalJsonSchema(GenerateInternalJsonSchemaCommand),
}

#[derive(Debug, Args)]
struct AppServerProxyCommand {
    /// Path to the app-server Unix domain socket to connect to.
    #[arg(long = "sock", value_name = "SOCKET_PATH", value_parser = parse_socket_path)]
    socket_path: Option<AbsolutePathBuf>,
}

#[derive(Debug, Args)]
struct GenerateTsCommand {
    /// Output directory where .ts files will be written
    #[arg(short = 'o', long = "out", value_name = "DIR")]
    out_dir: PathBuf,

    /// Optional path to the Prettier executable to format generated files
    #[arg(short = 'p', long = "prettier", value_name = "PRETTIER_BIN")]
    prettier: Option<PathBuf>,

    /// Include experimental methods and fields in the generated output
    #[arg(long = "experimental", default_value_t = false)]
    experimental: bool,
}

#[derive(Debug, Args)]
struct GenerateJsonSchemaCommand {
    /// Output directory where the schema bundle will be written
    #[arg(short = 'o', long = "out", value_name = "DIR")]
    out_dir: PathBuf,

    /// Include experimental methods and fields in the generated output
    #[arg(long = "experimental", default_value_t = false)]
    experimental: bool,
}

#[derive(Debug, Args)]
struct GenerateInternalJsonSchemaCommand {
    /// Output directory where internal JSON Schema artifacts will be written
    #[arg(short = 'o', long = "out", value_name = "DIR")]
    out_dir: PathBuf,
}

#[derive(Debug, Parser)]
struct StdioToUdsCommand {
    /// Path to the Unix domain socket to connect to.
    #[arg(value_name = "SOCKET_PATH", value_parser = parse_socket_path)]
    socket_path: AbsolutePathBuf,
}

fn parse_socket_path(raw: &str) -> Result<AbsolutePathBuf, String> {
    AbsolutePathBuf::relative_to_current_dir(raw)
        .map_err(|err| format!("failed to resolve socket path `{raw}`: {err}"))
}

fn format_exit_messages(exit_info: AppExitInfo, color_enabled: bool) -> Vec<String> {
    let AppExitInfo {
        token_usage,
        thread_id: conversation_id,
        ..
    } = exit_info;

    let mut lines = Vec::new();
    if !token_usage.is_zero() {
        lines.push(token_usage.to_string());
    }

    if let Some(resume_cmd) =
        codex_core::util::resume_command(/*thread_name*/ None, conversation_id)
    {
        let command = if color_enabled {
            resume_cmd.cyan().to_string()
        } else {
            resume_cmd
        };
        lines.push(format!("To continue this session, run {command}"));
    }

    lines
}

/// Handle the app exit and print the results. Optionally run the update action.
fn handle_app_exit(exit_info: AppExitInfo) -> anyhow::Result<()> {
    match exit_info.exit_reason {
        ExitReason::Fatal(message) => {
            eprintln!("ERROR: {message}");
            std::process::exit(1);
        }
        ExitReason::UserRequested => { /* normal exit */ }
    }

    let update_action = exit_info.update_action;
    let color_enabled = supports_color::on(Stream::Stdout).is_some();
    for line in format_exit_messages(exit_info, color_enabled) {
        println!("{line}");
    }
    if let Some(action) = update_action {
        run_update_action(action)?;
    }
    Ok(())
}

/// Run the update action and print the result.
fn run_update_action(action: UpdateAction) -> anyhow::Result<()> {
    println!();
    let cmd_str = action.command_str();
    println!("Updating Aegis Code via `{cmd_str}`...");

    let status = {
        #[cfg(windows)]
        {
            if action == UpdateAction::StandaloneWindows {
                let (cmd, args) = action.command_args();
                // Run the standalone PowerShell installer with PowerShell
                // itself. Routing this through `cmd.exe /C` would parse
                // PowerShell metacharacters like `|` before PowerShell sees
                // the installer command.
                std::process::Command::new(cmd).args(args).status()?
            } else {
                // On Windows, run via cmd.exe so .CMD/.BAT are correctly resolved (PATHEXT semantics).
                std::process::Command::new("cmd")
                    .args(["/C", &cmd_str])
                    .status()?
            }
        }
        #[cfg(not(windows))]
        {
            let (cmd, args) = action.command_args();
            let command_path = crate::wsl_paths::normalize_for_wsl(cmd);
            let normalized_args: Vec<String> = args
                .iter()
                .map(crate::wsl_paths::normalize_for_wsl)
                .collect();
            std::process::Command::new(&command_path)
                .args(&normalized_args)
                .status()?
        }
    };
    if !status.success() {
        anyhow::bail!("`{cmd_str}` failed with status {status}");
    }
    println!("\n🎉 Update ran successfully! Please restart Aegis Code.");
    Ok(())
}

fn run_update_command() -> anyhow::Result<()> {
    #[cfg(debug_assertions)]
    {
        anyhow::bail!(
            "`aegis update` is not available in debug builds. Install a release build of Aegis Code to use this command."
        );
    }

    #[cfg(not(debug_assertions))]
    {
        let Some(action) = codex_tui::get_update_action() else {
            anyhow::bail!(
                "Could not detect the Aegis Code installation method. Please update manually from the Aegis Code release artifacts."
            );
        };
        run_update_action(action)
    }
}

fn run_execpolicycheck(cmd: ExecPolicyCheckCommand) -> anyhow::Result<()> {
    cmd.run()
}

async fn run_debug_app_server_command(cmd: DebugAppServerCommand) -> anyhow::Result<()> {
    match cmd.subcommand {
        DebugAppServerSubcommand::SendMessageV2(cmd) => {
            let codex_bin = std::env::current_exe()?;
            codex_app_server_test_client::send_message_v2(&codex_bin, &[], cmd.user_message, &None)
                .await
        }
    }
}

#[derive(Debug, Default, Parser, Clone)]
struct FeatureToggles {
    /// Enable a feature (repeatable). Equivalent to `-c features.<name>=true`.
    #[arg(long = "enable", value_name = "FEATURE", action = clap::ArgAction::Append, global = true)]
    enable: Vec<String>,

    /// Disable a feature (repeatable). Equivalent to `-c features.<name>=false`.
    #[arg(long = "disable", value_name = "FEATURE", action = clap::ArgAction::Append, global = true)]
    disable: Vec<String>,
}

#[derive(Debug, Default, Parser, Clone)]
struct InteractiveRemoteOptions {
    /// Connect the TUI to a remote app server websocket endpoint.
    ///
    /// Accepted forms: `ws://host:port` or `wss://host:port`.
    #[arg(long = "remote", value_name = "ADDR")]
    remote: Option<String>,

    /// Name of the environment variable containing the bearer token to send to
    /// a remote app server websocket.
    #[arg(long = "remote-auth-token-env", value_name = "ENV_VAR")]
    remote_auth_token_env: Option<String>,
}

impl FeatureToggles {
    fn to_overrides(&self) -> anyhow::Result<Vec<String>> {
        let mut v = Vec::new();
        for feature in &self.enable {
            Self::validate_feature(feature)?;
            v.push(format!("features.{feature}=true"));
        }
        for feature in &self.disable {
            Self::validate_feature(feature)?;
            v.push(format!("features.{feature}=false"));
        }
        Ok(v)
    }

    fn validate_feature(feature: &str) -> anyhow::Result<()> {
        if is_known_feature_key(feature) {
            Ok(())
        } else {
            anyhow::bail!("Unknown feature flag: {feature}")
        }
    }
}

#[derive(Debug, Parser)]
struct FeaturesCli {
    #[command(subcommand)]
    sub: FeaturesSubcommand,
}

#[derive(Debug, Parser)]
enum FeaturesSubcommand {
    /// List known features with their stage and effective state.
    List,
    /// Enable a feature in config.toml.
    Enable(FeatureSetArgs),
    /// Disable a feature in config.toml.
    Disable(FeatureSetArgs),
}

#[derive(Debug, Parser)]
struct FeatureSetArgs {
    /// Feature key to update (for example: unified_exec).
    feature: String,
}

fn stage_str(stage: Stage) -> &'static str {
    match stage {
        Stage::UnderDevelopment => "under development",
        Stage::Experimental { .. } => "experimental",
        Stage::Stable => "stable",
        Stage::Deprecated => "deprecated",
        Stage::Removed => "removed",
    }
}

fn main() -> anyhow::Result<()> {
    arg0_dispatch_or_else(|arg0_paths: Arg0DispatchPaths| async move {
        cli_main(arg0_paths).await?;
        Ok(())
    })
}

async fn cli_main(arg0_paths: Arg0DispatchPaths) -> anyhow::Result<()> {
    let MultitoolCli {
        config_overrides: mut root_config_overrides,
        feature_toggles,
        remote,
        mut interactive,
        subcommand,
    } = MultitoolCli::parse();

    // Fold --enable/--disable into config overrides so they flow to all subcommands.
    let toggle_overrides = feature_toggles.to_overrides()?;
    root_config_overrides.raw_overrides.extend(toggle_overrides);
    let root_remote = remote.remote;
    let root_remote_auth_token_env = remote.remote_auth_token_env;

    match subcommand {
        None => {
            prepend_config_flags(
                &mut interactive.config_overrides,
                root_config_overrides.clone(),
            );
            let exit_info = run_interactive_tui(
                interactive,
                root_remote.clone(),
                root_remote_auth_token_env.clone(),
                arg0_paths.clone(),
            )
            .await?;
            handle_app_exit(exit_info)?;
        }
        Some(Subcommand::Exec(mut exec_cli)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "exec",
            )?;
            exec_cli
                .shared
                .inherit_exec_root_options(&interactive.shared);
            prepend_config_flags(
                &mut exec_cli.config_overrides,
                root_config_overrides.clone(),
            );
            codex_exec::run_main(exec_cli, arg0_paths.clone()).await?;
        }
        Some(Subcommand::Review(review_args)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "review",
            )?;
            let mut exec_cli = ExecCli::try_parse_from(["aegis", "exec"])?;
            exec_cli.command = Some(ExecCommand::Review(review_args));
            prepend_config_flags(
                &mut exec_cli.config_overrides,
                root_config_overrides.clone(),
            );
            codex_exec::run_main(exec_cli, arg0_paths.clone()).await?;
        }
        Some(Subcommand::McpServer) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "mcp-server",
            )?;
            codex_mcp_server::run_main(arg0_paths.clone(), root_config_overrides).await?;
        }
        Some(Subcommand::Mcp(mut mcp_cli)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "mcp",
            )?;
            // Propagate any root-level config overrides (e.g. `-c key=value`).
            prepend_config_flags(&mut mcp_cli.config_overrides, root_config_overrides.clone());
            mcp_cli.run().await?;
        }
        Some(Subcommand::Plugin(plugin_cli)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "plugin",
            )?;
            let PluginCli {
                mut config_overrides,
                subcommand,
            } = plugin_cli;
            prepend_config_flags(&mut config_overrides, root_config_overrides.clone());
            match subcommand {
                PluginSubcommand::Marketplace(mut marketplace_cli) => {
                    prepend_config_flags(&mut marketplace_cli.config_overrides, config_overrides);
                    marketplace_cli.run().await?;
                }
            }
        }
        Some(Subcommand::AppServer(app_server_cli)) => {
            let AppServerCommand {
                subcommand,
                listen,
                analytics_default_enabled,
                auth,
            } = app_server_cli;
            reject_remote_mode_for_app_server_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                subcommand.as_ref(),
            )?;
            match subcommand {
                None => {
                    let transport = listen;
                    let auth = auth.try_into_settings()?;
                    codex_app_server::run_main_with_transport(
                        arg0_paths.clone(),
                        root_config_overrides,
                        codex_config::LoaderOverrides::default(),
                        analytics_default_enabled,
                        transport,
                        codex_protocol::protocol::SessionSource::VSCode,
                        auth,
                    )
                    .await?;
                }
                Some(AppServerSubcommand::Proxy(proxy_cli)) => {
                    let socket_path = match proxy_cli.socket_path {
                        Some(socket_path) => socket_path,
                        None => {
                            let codex_home = find_codex_home()?;
                            codex_app_server::app_server_control_socket_path(&codex_home)?
                        }
                    };
                    codex_stdio_to_uds::run(socket_path.as_path()).await?;
                }
                Some(AppServerSubcommand::GenerateTs(gen_cli)) => {
                    let options = codex_app_server_protocol::GenerateTsOptions {
                        experimental_api: gen_cli.experimental,
                        ..Default::default()
                    };
                    codex_app_server_protocol::generate_ts_with_options(
                        &gen_cli.out_dir,
                        gen_cli.prettier.as_deref(),
                        options,
                    )?;
                }
                Some(AppServerSubcommand::GenerateJsonSchema(gen_cli)) => {
                    codex_app_server_protocol::generate_json_with_experimental(
                        &gen_cli.out_dir,
                        gen_cli.experimental,
                    )?;
                }
                Some(AppServerSubcommand::GenerateInternalJsonSchema(gen_cli)) => {
                    codex_app_server_protocol::generate_internal_json_schema(&gen_cli.out_dir)?;
                }
            }
        }
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        Some(Subcommand::App(app_cli)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "app",
            )?;
            app_cmd::run_app(app_cli).await?;
        }
        Some(Subcommand::Resume(ResumeCommand {
            session_id,
            last,
            all,
            include_non_interactive,
            remote,
            config_overrides,
        })) => {
            interactive = finalize_resume_interactive(
                interactive,
                root_config_overrides.clone(),
                session_id,
                last,
                all,
                include_non_interactive,
                config_overrides,
            );
            let exit_info = run_interactive_tui(
                interactive,
                remote.remote.or(root_remote.clone()),
                remote
                    .remote_auth_token_env
                    .or(root_remote_auth_token_env.clone()),
                arg0_paths.clone(),
            )
            .await?;
            handle_app_exit(exit_info)?;
        }
        Some(Subcommand::Fork(ForkCommand {
            session_id,
            last,
            all,
            remote,
            config_overrides,
        })) => {
            interactive = finalize_fork_interactive(
                interactive,
                root_config_overrides.clone(),
                session_id,
                last,
                all,
                config_overrides,
            );
            let exit_info = run_interactive_tui(
                interactive,
                remote.remote.or(root_remote.clone()),
                remote
                    .remote_auth_token_env
                    .or(root_remote_auth_token_env.clone()),
                arg0_paths.clone(),
            )
            .await?;
            handle_app_exit(exit_info)?;
        }
        Some(Subcommand::Login(mut login_cli)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "login",
            )?;
            prepend_config_flags(
                &mut login_cli.config_overrides,
                root_config_overrides.clone(),
            );
            match login_cli.action {
                Some(LoginSubcommand::Status) => {
                    run_login_status(login_cli.config_overrides).await;
                }
                None => {
                    if login_cli.with_api_key && login_cli.with_access_token {
                        eprintln!(
                            "Choose one login credential source: --with-api-key or --with-access-token."
                        );
                        std::process::exit(1);
                    } else if login_cli.use_device_code {
                        run_login_with_device_code(
                            login_cli.config_overrides,
                            login_cli.issuer_base_url,
                            login_cli.client_id,
                        )
                        .await;
                    } else if login_cli.api_key.is_some() {
                        eprintln!(
                            "The --api-key flag is no longer supported. Pipe the key instead, e.g. `printenv OPENAI_API_KEY | aegis login --with-api-key`."
                        );
                        std::process::exit(1);
                    } else if login_cli.with_api_key {
                        let api_key = read_api_key_from_stdin();
                        run_login_with_api_key(login_cli.config_overrides, api_key).await;
                    } else if login_cli.with_access_token {
                        let access_token = read_access_token_from_stdin();
                        run_login_with_access_token(login_cli.config_overrides, access_token).await;
                    } else {
                        run_login_with_chatgpt(login_cli.config_overrides).await;
                    }
                }
            }
        }
        Some(Subcommand::Logout(mut logout_cli)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "logout",
            )?;
            prepend_config_flags(
                &mut logout_cli.config_overrides,
                root_config_overrides.clone(),
            );
            run_logout(logout_cli.config_overrides).await;
        }
        Some(Subcommand::Completion(completion_cli)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "completion",
            )?;
            print_completion(completion_cli);
        }
        Some(Subcommand::Update) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "update",
            )?;
            run_update_command()?;
        }
        Some(Subcommand::Cloud(mut cloud_cli)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "cloud",
            )?;
            prepend_config_flags(
                &mut cloud_cli.config_overrides,
                root_config_overrides.clone(),
            );
            codex_cloud_tasks::run_main(cloud_cli, arg0_paths.codex_linux_sandbox_exe.clone())
                .await?;
        }
        Some(Subcommand::Sandbox(sandbox_args)) => match sandbox_args.cmd {
            SandboxCommand::Macos(mut seatbelt_cli) => {
                reject_remote_mode_for_subcommand(
                    root_remote.as_deref(),
                    root_remote_auth_token_env.as_deref(),
                    "sandbox macos",
                )?;
                prepend_config_flags(
                    &mut seatbelt_cli.config_overrides,
                    root_config_overrides.clone(),
                );
                codex_cli::run_command_under_seatbelt(
                    seatbelt_cli,
                    arg0_paths.codex_linux_sandbox_exe.clone(),
                )
                .await?;
            }
            SandboxCommand::Linux(mut landlock_cli) => {
                reject_remote_mode_for_subcommand(
                    root_remote.as_deref(),
                    root_remote_auth_token_env.as_deref(),
                    "sandbox linux",
                )?;
                prepend_config_flags(
                    &mut landlock_cli.config_overrides,
                    root_config_overrides.clone(),
                );
                codex_cli::run_command_under_landlock(
                    landlock_cli,
                    arg0_paths.codex_linux_sandbox_exe.clone(),
                )
                .await?;
            }
            SandboxCommand::Windows(mut windows_cli) => {
                reject_remote_mode_for_subcommand(
                    root_remote.as_deref(),
                    root_remote_auth_token_env.as_deref(),
                    "sandbox windows",
                )?;
                prepend_config_flags(
                    &mut windows_cli.config_overrides,
                    root_config_overrides.clone(),
                );
                codex_cli::run_command_under_windows(
                    windows_cli,
                    arg0_paths.codex_linux_sandbox_exe.clone(),
                )
                .await?;
            }
        },
        Some(Subcommand::Doctor(cmd)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "doctor",
            )?;
            run_doctor_command(cmd, root_config_overrides, interactive).await?;
        }
        Some(Subcommand::Config(cmd)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "config",
            )?;
            run_config_command(cmd)?;
        }
        Some(Subcommand::IssueTrain(cmd)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "issue-train",
            )?;
            run_issue_train_command(cmd)?;
        }
        Some(Subcommand::PrReadiness(cmd)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "pr-readiness",
            )?;
            run_pr_readiness_command(cmd)?;
        }
        Some(Subcommand::ContextPack(cmd)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "context-pack",
            )?;
            run_context_pack_command(cmd, root_config_overrides, interactive).await?;
        }
        Some(Subcommand::Debug(DebugCommand { subcommand })) => match subcommand {
            DebugSubcommand::Models(cmd) => {
                reject_remote_mode_for_subcommand(
                    root_remote.as_deref(),
                    root_remote_auth_token_env.as_deref(),
                    "debug models",
                )?;
                run_debug_models_command(cmd, root_config_overrides).await?;
            }
            DebugSubcommand::AppServer(cmd) => {
                reject_remote_mode_for_subcommand(
                    root_remote.as_deref(),
                    root_remote_auth_token_env.as_deref(),
                    "debug app-server",
                )?;
                run_debug_app_server_command(cmd).await?;
            }
            DebugSubcommand::PromptInput(cmd) => {
                reject_remote_mode_for_subcommand(
                    root_remote.as_deref(),
                    root_remote_auth_token_env.as_deref(),
                    "debug prompt-input",
                )?;
                run_debug_prompt_input_command(
                    cmd,
                    root_config_overrides,
                    interactive,
                    arg0_paths.clone(),
                )
                .await?;
            }
            DebugSubcommand::PromptLayers(cmd) => {
                reject_remote_mode_for_subcommand(
                    root_remote.as_deref(),
                    root_remote_auth_token_env.as_deref(),
                    "debug prompt-layers",
                )?;
                run_debug_prompt_layers_command(
                    cmd,
                    root_config_overrides,
                    interactive,
                    arg0_paths.clone(),
                )
                .await?;
            }
            DebugSubcommand::TraceReduce(cmd) => {
                reject_remote_mode_for_subcommand(
                    root_remote.as_deref(),
                    root_remote_auth_token_env.as_deref(),
                    "debug trace-reduce",
                )?;
                run_debug_trace_reduce_command(cmd).await?;
            }
            DebugSubcommand::ClearMemories => {
                reject_remote_mode_for_subcommand(
                    root_remote.as_deref(),
                    root_remote_auth_token_env.as_deref(),
                    "debug clear-memories",
                )?;
                run_debug_clear_memories_command(&root_config_overrides, &interactive).await?;
            }
        },
        Some(Subcommand::Execpolicy(ExecpolicyCommand { sub })) => match sub {
            ExecpolicySubcommand::Check(cmd) => {
                reject_remote_mode_for_subcommand(
                    root_remote.as_deref(),
                    root_remote_auth_token_env.as_deref(),
                    "execpolicy check",
                )?;
                run_execpolicycheck(cmd)?
            }
        },
        Some(Subcommand::Apply(mut apply_cli)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "apply",
            )?;
            prepend_config_flags(
                &mut apply_cli.config_overrides,
                root_config_overrides.clone(),
            );
            run_apply_command(apply_cli, /*cwd*/ None).await?;
        }
        Some(Subcommand::ResponsesApiProxy(args)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "responses-api-proxy",
            )?;
            tokio::task::spawn_blocking(move || codex_responses_api_proxy::run_main(args))
                .await??;
        }
        Some(Subcommand::StdioToUds(cmd)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "stdio-to-uds",
            )?;
            let socket_path = cmd.socket_path;
            codex_stdio_to_uds::run(socket_path.as_path()).await?;
        }
        Some(Subcommand::ExecServer(cmd)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "exec-server",
            )?;
            run_exec_server_command(cmd, &arg0_paths).await?;
        }
        Some(Subcommand::Features(FeaturesCli { sub })) => match sub {
            FeaturesSubcommand::List => {
                reject_remote_mode_for_subcommand(
                    root_remote.as_deref(),
                    root_remote_auth_token_env.as_deref(),
                    "features list",
                )?;
                // Respect root-level `-c` overrides plus top-level flags like `--profile`.
                let mut cli_kv_overrides = root_config_overrides
                    .parse_overrides()
                    .map_err(anyhow::Error::msg)?;

                // Honor `--search` via the canonical web_search mode.
                if interactive.web_search {
                    cli_kv_overrides.push((
                        "web_search".to_string(),
                        toml::Value::String("live".to_string()),
                    ));
                }

                // Thread through relevant top-level flags (at minimum, `--profile`).
                let overrides = ConfigOverrides {
                    config_profile: interactive.config_profile.clone(),
                    ..Default::default()
                };

                let config = Config::load_with_cli_overrides_and_harness_overrides(
                    cli_kv_overrides,
                    overrides,
                )
                .await?;
                let mut rows = Vec::with_capacity(FEATURES.len());
                let mut name_width = 0;
                let mut stage_width = 0;
                for def in FEATURES {
                    let name = def.key;
                    let stage = stage_str(def.stage);
                    let enabled = config.features.enabled(def.id);
                    name_width = name_width.max(name.len());
                    stage_width = stage_width.max(stage.len());
                    rows.push((name, stage, enabled));
                }
                rows.sort_unstable_by_key(|(name, _, _)| *name);

                for (name, stage, enabled) in rows {
                    println!("{name:<name_width$}  {stage:<stage_width$}  {enabled}");
                }
            }
            FeaturesSubcommand::Enable(FeatureSetArgs { feature }) => {
                reject_remote_mode_for_subcommand(
                    root_remote.as_deref(),
                    root_remote_auth_token_env.as_deref(),
                    "features enable",
                )?;
                enable_feature_in_config(&interactive, &feature).await?;
            }
            FeaturesSubcommand::Disable(FeatureSetArgs { feature }) => {
                reject_remote_mode_for_subcommand(
                    root_remote.as_deref(),
                    root_remote_auth_token_env.as_deref(),
                    "features disable",
                )?;
                disable_feature_in_config(&interactive, &feature).await?;
            }
        },
    }

    Ok(())
}

async fn run_exec_server_command(
    cmd: ExecServerCommand,
    arg0_paths: &Arg0DispatchPaths,
) -> anyhow::Result<()> {
    let codex_self_exe = arg0_paths
        .codex_self_exe
        .clone()
        .ok_or_else(|| anyhow::anyhow!("Codex executable path is not configured"))?;
    let runtime_paths = codex_exec_server::ExecServerRuntimePaths::new(
        codex_self_exe,
        arg0_paths.codex_linux_sandbox_exe.clone(),
    )?;
    if let Some(base_url) = cmd.remote {
        let executor_id = cmd
            .executor_id
            .ok_or_else(|| anyhow::anyhow!("--executor-id is required when --remote is set"))?;
        let mut remote_config =
            codex_exec_server::RemoteExecutorConfig::new(base_url, executor_id)?;
        if let Some(name) = cmd.name {
            remote_config.name = name;
        }
        codex_exec_server::run_remote_executor(remote_config, runtime_paths).await?;
        return Ok(());
    }
    let listen_url = cmd
        .listen
        .as_deref()
        .unwrap_or(codex_exec_server::DEFAULT_LISTEN_URL);
    codex_exec_server::run_main(listen_url, runtime_paths)
        .await
        .map_err(anyhow::Error::from_boxed)
}

async fn enable_feature_in_config(interactive: &TuiCli, feature: &str) -> anyhow::Result<()> {
    FeatureToggles::validate_feature(feature)?;
    let codex_home = find_codex_home()?;
    ConfigEditsBuilder::new(&codex_home)
        .with_profile(interactive.config_profile.as_deref())
        .set_feature_enabled(feature, /*enabled*/ true)
        .apply()
        .await?;
    println!("Enabled feature `{feature}` in config.toml.");
    maybe_print_under_development_feature_warning(&codex_home, interactive, feature);
    Ok(())
}

async fn disable_feature_in_config(interactive: &TuiCli, feature: &str) -> anyhow::Result<()> {
    FeatureToggles::validate_feature(feature)?;
    let codex_home = find_codex_home()?;
    ConfigEditsBuilder::new(&codex_home)
        .with_profile(interactive.config_profile.as_deref())
        .set_feature_enabled(feature, /*enabled*/ false)
        .apply()
        .await?;
    println!("Disabled feature `{feature}` in config.toml.");
    Ok(())
}

fn maybe_print_under_development_feature_warning(
    codex_home: &std::path::Path,
    interactive: &TuiCli,
    feature: &str,
) {
    if interactive.config_profile.is_some() {
        return;
    }

    let Some(spec) = FEATURES.iter().find(|spec| spec.key == feature) else {
        return;
    };
    if !matches!(spec.stage, Stage::UnderDevelopment) {
        return;
    }

    let config_path = codex_home.join(codex_config::CONFIG_TOML_FILE);
    eprintln!(
        "Under-development features enabled: {feature}. Under-development features are incomplete and may behave unpredictably. To suppress this warning, set `suppress_unstable_features_warning = true` in {}.",
        config_path.display()
    );
}

async fn run_debug_trace_reduce_command(cmd: DebugTraceReduceCommand) -> anyhow::Result<()> {
    let output = cmd
        .output
        .unwrap_or_else(|| cmd.trace_bundle.join(REDUCED_STATE_FILE_NAME));

    let trace = replay_bundle(&cmd.trace_bundle)?;
    let reduced_json = serde_json::to_vec_pretty(&trace)?;
    tokio::fs::write(&output, reduced_json).await?;
    println!("{}", output.display());

    Ok(())
}

async fn run_doctor_command(
    cmd: DoctorCommand,
    root_config_overrides: CliConfigOverrides,
    interactive: TuiCli,
) -> anyhow::Result<()> {
    let config = load_config_for_local_command(root_config_overrides, interactive).await?;
    let report = codex_core::doctor::build_doctor_report(&config);

    if cmd.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!(
            "{}",
            codex_core::doctor::format_doctor_report_human(&report)
        );
    }

    Ok(())
}

fn run_config_command(cmd: ConfigCommand) -> anyhow::Result<()> {
    match cmd.subcommand {
        ConfigSubcommand::ImportCodex(cmd) => run_config_import_codex_command(cmd)?,
    }
    Ok(())
}

fn run_config_import_codex_command(cmd: ConfigImportCodexCommand) -> anyhow::Result<()> {
    let codex_home = find_codex_home()?;
    let source = cmd.source.unwrap_or(default_codex_config_path()?);
    let destination = cmd.destination.unwrap_or_else(|| {
        codex_home
            .join(codex_config::CONFIG_TOML_FILE)
            .to_path_buf()
    });
    let options = CodexConfigImportOptions {
        source,
        destination,
        include_prompts: cmd.include_prompts,
        apply: cmd.apply,
    };
    let report = if cmd.apply {
        apply_codex_config_import(&options)?
    } else {
        preview_codex_config_import(&options)?
    };

    if cmd.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_config_import_report(&report);
    }

    Ok(())
}

fn print_config_import_report(report: &CodexConfigImportReport) {
    println!("Codex config import");
    println!("Source: {}", report.source.display());
    println!("Destination: {}", report.destination.display());

    if !report.source_exists {
        println!("No Codex config found; no changes to apply.");
        return;
    }

    if report.imports.is_empty() {
        println!("No supported settings to import.");
    } else {
        println!("Settings to import:");
        for entry in &report.imports {
            println!("  - {}", entry.key_path);
        }
    }

    if !report.skipped.is_empty() {
        println!("Skipped settings:");
        for entry in &report.skipped {
            println!("  - {} ({})", entry.key_path, entry.reason);
        }
    }

    if report.applied {
        if report.changed {
            println!("Applied import to {}.", report.destination.display());
        } else {
            println!("No changes were written.");
        }
    } else if report.changed {
        println!("Preview only; re-run with --apply to write these settings.");
    } else {
        println!("Preview only; no changes would be written.");
    }
}

fn run_issue_train_command(cmd: IssueTrainCommand) -> anyhow::Result<()> {
    match cmd.subcommand {
        IssueTrainSubcommand::Validate(cmd) => {
            let runner = ProcessGhRunner;
            let report = validate_issue_train_from_github(&cmd, &runner)?;
            if cmd.json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print!("{}", format_issue_train_report_human(&report));
            }

            if !report.valid {
                std::process::exit(1);
            }
        }
    }

    Ok(())
}

fn run_pr_readiness_command(cmd: PrReadinessCommand) -> anyhow::Result<()> {
    match cmd.subcommand {
        PrReadinessSubcommand::Validate(cmd) => {
            let runner = ProcessGhRunner;
            let report = validate_pr_readiness_from_github(&cmd, &runner)?;
            if cmd.json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print!("{}", format_pr_readiness_report_human(&report));
            }

            if !report.valid {
                std::process::exit(1);
            }
        }
    }

    Ok(())
}

fn validate_pr_readiness_from_github(
    cmd: &PrReadinessValidateCommand,
    runner: &dyn GhRunner,
) -> anyhow::Result<PrReadinessReport> {
    let snapshot = load_pr_readiness_snapshot(cmd, runner)?;
    Ok(validate_pr_readiness(&snapshot))
}

fn load_pr_readiness_snapshot(
    cmd: &PrReadinessValidateCommand,
    runner: &dyn GhRunner,
) -> anyhow::Result<PrReadinessSnapshot> {
    let repo = resolve_github_repo(cmd.repo.as_deref(), runner)?;
    let pr_number = resolve_pr_number(&repo, cmd.pr, runner)?;
    let pull_request = fetch_github_pr(&repo, pr_number, runner)?;
    let mut allowed_paths = cmd.allowed_paths.clone();
    allowed_paths.extend(parse_allowed_paths_from_pr_body(&pull_request.body));
    let closing_refs = parse_closing_issue_refs(&pull_request.body);
    let linked_issue = if closing_refs.len() == 1 {
        fetch_github_issue(&repo, closing_refs[0], runner).ok()
    } else {
        None
    };
    let parent_number = resolve_parent_issue_number(&repo, None, runner)?;
    let parent_issue = Some(fetch_github_issue(&repo, parent_number, runner)?);
    let child_issues = parent_issue
        .as_ref()
        .map(|parent| fetch_parent_child_issues(&repo, parent, runner))
        .unwrap_or_default();
    let method_state = load_method_state_json(&cmd.method_state)?;

    Ok(PrReadinessSnapshot {
        repository: repo,
        pull_request,
        linked_issue,
        parent_issue,
        child_issues,
        method_state: Some(method_state),
        allowed_paths,
    })
}

fn resolve_pr_number(repo: &str, pr: Option<u64>, runner: &dyn GhRunner) -> anyhow::Result<u64> {
    if let Some(pr) = pr {
        return Ok(pr);
    }

    let args = ["pr", "view", "--repo", repo, "--json", "number"];
    let pr: GhPrNumber = run_gh_json(runner, &args, "resolve pull request")?;
    Ok(pr.number)
}

fn fetch_github_pr(
    repo: &str,
    pr_number: u64,
    runner: &dyn GhRunner,
) -> anyhow::Result<PullRequestSnapshot> {
    let number = pr_number.to_string();
    let args = [
        "pr",
        "view",
        number.as_str(),
        "--repo",
        repo,
        "--json",
        "number,title,body,headRefOid,headRefName,baseRefName,files",
    ];
    let pr: GhPr = run_gh_json(runner, &args, "load pull request")?;
    Ok(pr.into_snapshot())
}

fn fetch_parent_child_issues(
    repo: &str,
    parent: &IssueSnapshot,
    runner: &dyn GhRunner,
) -> Vec<IssueSnapshot> {
    let mut children = Vec::new();
    let mut seen = BTreeSet::new();
    for child_ref in parse_parent_child_refs(&parent.body) {
        if !seen.insert(child_ref.issue_number) {
            continue;
        }
        if let Ok(child) = fetch_github_issue(repo, child_ref.issue_number, runner) {
            children.push(child);
        }
    }
    children
}

fn load_method_state_json(path: &PathBuf) -> anyhow::Result<MethodState> {
    let text = std::fs::read_to_string(path).map_err(|error| {
        anyhow::anyhow!(
            "failed to read method-state JSON {}: {error}",
            path.display()
        )
    })?;
    serde_json::from_str(&text).map_err(|error| {
        anyhow::anyhow!(
            "failed to parse method-state JSON {}: {error}",
            path.display()
        )
    })
}

fn format_pr_readiness_report_human(report: &PrReadinessReport) -> String {
    let status = if report.valid { "ready" } else { "not ready" };
    let issue = report
        .linked_issue_number
        .map(|number| format!("#{number}"))
        .unwrap_or_else(|| "none".to_string());
    let mut output = format!(
        "PR #{}: {status}\nLinked issue: {issue}\nChanged files: {}\n",
        report.pr_number, report.changed_file_count
    );

    if report.findings.is_empty() {
        output.push_str("No findings.\n");
        return output;
    }

    for finding in &report.findings {
        let severity = match finding.severity {
            FindingSeverity::Error => "error",
            FindingSeverity::Warning => "warning",
        };
        let subject = finding.subject.as_deref().unwrap_or("pr");
        output.push_str(&format!(
            "[{severity}] {subject} {}: {}\n  fix: {}\n",
            finding.code, finding.message, finding.remediation
        ));
    }

    output
}

fn validate_issue_train_from_github(
    cmd: &IssueTrainValidateCommand,
    runner: &dyn GhRunner,
) -> anyhow::Result<IssueTrainReport> {
    let snapshot = load_issue_train_snapshot(cmd, runner)?;
    Ok(validate_issue_train(&snapshot))
}

fn load_issue_train_snapshot(
    cmd: &IssueTrainValidateCommand,
    runner: &dyn GhRunner,
) -> anyhow::Result<IssueTrainSnapshot> {
    let repo = resolve_github_repo(cmd.repo.as_deref(), runner)?;
    let parent_number = resolve_parent_issue_number(&repo, cmd.parent, runner)?;
    let parent = fetch_github_issue(&repo, parent_number, runner)?;
    let mut children = Vec::new();
    let mut seen = BTreeSet::new();

    for child_ref in parse_parent_child_refs(&parent.body) {
        if !seen.insert(child_ref.issue_number) {
            continue;
        }
        if let Ok(child) = fetch_github_issue(&repo, child_ref.issue_number, runner) {
            children.push(child);
        }
    }

    Ok(IssueTrainSnapshot { parent, children })
}

fn resolve_github_repo(repo: Option<&str>, runner: &dyn GhRunner) -> anyhow::Result<String> {
    if let Some(repo) = repo {
        return Ok(repo.to_string());
    }

    let output: GhRepoView = run_gh_json(
        runner,
        &["repo", "view", "--json", "nameWithOwner"],
        "resolve repo",
    )?;
    Ok(output.name_with_owner)
}

fn resolve_parent_issue_number(
    repo: &str,
    parent: Option<u64>,
    runner: &dyn GhRunner,
) -> anyhow::Result<u64> {
    if let Some(parent) = parent {
        return Ok(parent);
    }

    let args = [
        "issue",
        "list",
        "--repo",
        repo,
        "--state",
        "open",
        "--label",
        codex_core::issue_train::PLAN_LABEL,
        "--json",
        "number",
        "--limit",
        "50",
    ];
    let plans: Vec<GhIssueListItem> = run_gh_json(runner, &args, "find parent plan issue")?;
    match plans.as_slice() {
        [plan] => Ok(plan.number),
        [] => anyhow::bail!(
            "no open parent plan issue found; pass --parent or label exactly one open issue with {}",
            codex_core::issue_train::PLAN_LABEL
        ),
        _ => anyhow::bail!(
            "multiple open parent plan issues found; pass --parent to choose one explicitly"
        ),
    }
}

fn fetch_github_issue(
    repo: &str,
    issue_number: u64,
    runner: &dyn GhRunner,
) -> anyhow::Result<IssueSnapshot> {
    let number = issue_number.to_string();
    let args = [
        "issue",
        "view",
        number.as_str(),
        "--repo",
        repo,
        "--json",
        "number,title,state,body,labels",
    ];
    let issue: GhIssue = run_gh_json(runner, &args, "load issue")?;
    Ok(issue.into_snapshot())
}

fn run_gh_json<T: DeserializeOwned>(
    runner: &dyn GhRunner,
    args: &[&str],
    context: &str,
) -> anyhow::Result<T> {
    let args = args
        .iter()
        .map(|arg| (*arg).to_string())
        .collect::<Vec<_>>();
    let output = runner.run(&args)?;
    if !output.success {
        anyhow::bail!("gh failed to {context}: {}", output.stderr.trim());
    }
    serde_json::from_str(&output.stdout)
        .map_err(|error| anyhow::anyhow!("failed to parse gh JSON for {context}: {error}"))
}

fn format_issue_train_report_human(report: &IssueTrainReport) -> String {
    let status = if report.valid { "ready" } else { "not ready" };
    let mut output = format!(
        "Issue train #{}: {status}\nChild issues: {}\n",
        report.parent_issue, report.child_count
    );

    if report.findings.is_empty() {
        output.push_str("No findings.\n");
        return output;
    }

    for finding in &report.findings {
        let severity = match finding.severity {
            FindingSeverity::Error => "error",
            FindingSeverity::Warning => "warning",
        };
        let issue = finding
            .issue_number
            .map(|number| format!("#{number}"))
            .unwrap_or_else(|| "train".to_string());
        output.push_str(&format!(
            "[{severity}] {issue} {}: {}\n  fix: {}\n",
            finding.code, finding.message, finding.remediation
        ));
    }

    output
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GhOutput {
    stdout: String,
    stderr: String,
    success: bool,
}

trait GhRunner {
    fn run(&self, args: &[String]) -> anyhow::Result<GhOutput>;
}

struct ProcessGhRunner;

impl GhRunner for ProcessGhRunner {
    fn run(&self, args: &[String]) -> anyhow::Result<GhOutput> {
        let output = ProcessCommand::new("gh").args(args).output()?;
        Ok(GhOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            success: output.status.success(),
        })
    }
}

#[derive(Debug, Deserialize)]
struct GhRepoView {
    #[serde(rename = "nameWithOwner")]
    name_with_owner: String,
}

#[derive(Debug, Deserialize)]
struct GhIssueListItem {
    number: u64,
}

#[derive(Debug, Deserialize)]
struct GhLabel {
    name: String,
}

#[derive(Debug, Deserialize)]
struct GhIssue {
    number: u64,
    title: String,
    state: String,
    body: Option<String>,
    #[serde(default)]
    labels: Vec<GhLabel>,
}

impl GhIssue {
    fn into_snapshot(self) -> IssueSnapshot {
        IssueSnapshot {
            number: self.number,
            title: self.title,
            state: if self.state.eq_ignore_ascii_case("closed") {
                IssueState::Closed
            } else {
                IssueState::Open
            },
            body: self.body.unwrap_or_default(),
            labels: self.labels.into_iter().map(|label| label.name).collect(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct GhPrNumber {
    number: u64,
}

#[derive(Debug, Deserialize)]
struct GhPrFile {
    path: String,
}

#[derive(Debug, Deserialize)]
struct GhPr {
    number: u64,
    title: String,
    body: Option<String>,
    #[serde(rename = "headRefOid")]
    head_ref_oid: String,
    #[serde(rename = "headRefName")]
    head_ref_name: String,
    #[serde(rename = "baseRefName")]
    base_ref_name: String,
    #[serde(default)]
    files: Vec<GhPrFile>,
}

impl GhPr {
    fn into_snapshot(self) -> PullRequestSnapshot {
        PullRequestSnapshot {
            number: self.number,
            title: self.title,
            body: self.body.unwrap_or_default(),
            head_sha: self.head_ref_oid,
            base_ref: self.base_ref_name,
            head_ref: self.head_ref_name,
            changed_files: self.files.into_iter().map(|file| file.path).collect(),
        }
    }
}

async fn run_context_pack_command(
    cmd: ContextPackCommand,
    root_config_overrides: CliConfigOverrides,
    interactive: TuiCli,
) -> anyhow::Result<()> {
    let config = load_config_for_local_command(root_config_overrides, interactive).await?;
    let paths = configured_context_pack_paths(&config)?;

    match cmd.subcommand {
        ContextPackSubcommand::List(cmd) => {
            let mut diagnostics = config.context_packs.diagnostics().to_vec();
            if let Some(kind) = cmd.kind {
                let kind = ContextPackKind::from(kind);
                diagnostics.retain(|diagnostic| diagnostic.kind == Some(kind));
            }
            if cmd.status != ContextPackStatusArg::All {
                diagnostics.retain(|diagnostic| diagnostic_status_matches(diagnostic, cmd.status));
            }
            if cmd.json {
                println!("{}", serde_json::to_string_pretty(&diagnostics)?);
            } else {
                print_context_pack_list(&diagnostics);
            }
        }
        ContextPackSubcommand::CompileCandidates(cmd) => {
            let output_dir = resolve_context_pack_cli_path(
                &config,
                cmd.output_dir.unwrap_or_else(|| {
                    config
                        .codex_home
                        .join("context-packs")
                        .join("learned-candidates")
                        .to_path_buf()
                }),
            );
            let options = LearnedPackCompilerOptions {
                events_path: resolve_context_pack_cli_path(
                    &config,
                    cmd.events
                        .unwrap_or_else(|| config.aegis_engine.jsonl_path.clone()),
                ),
                alert_inputs_path: resolve_context_pack_cli_path(
                    &config,
                    cmd.alert_inputs
                        .unwrap_or_else(|| config.aegis_engine.candidate_inputs_path.clone()),
                ),
                output_dir,
                repository: repository_name_for_context_pack(&config),
                min_support: cmd.min_support,
                now: context_pack_timestamp(),
                dry_run: cmd.dry_run,
            };
            let result = compile_learned_pack_candidates(&options)?;
            let registered_paths = if cmd.dry_run || cmd.no_register {
                Vec::new()
            } else {
                register_context_pack_candidate_paths(&config, &result)?
            };
            if cmd.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&LearnedPackCompileCliResult {
                        compile: result,
                        registered_paths,
                    })?
                );
            } else {
                print_learned_pack_compile_result(&result, &registered_paths, cmd.no_register);
            }
        }
        ContextPackSubcommand::Inspect(cmd) => {
            let path = resolve_context_pack_path(&config, &cmd.selector)?;
            let inspection = inspect_context_pack_path(&path, cmd.show_guidance)?;
            if cmd.json {
                println!("{}", serde_json::to_string_pretty(&inspection)?);
            } else {
                print_context_pack_inspection(&inspection);
            }
        }
        ContextPackSubcommand::Promote(cmd) => {
            let actor = cmd.actor.unwrap_or_else(default_context_pack_actor);
            let now = context_pack_timestamp();
            let result = promote_context_pack(
                &paths,
                &cmd.selector,
                &actor,
                &cmd.evidence,
                cmd.reason.as_deref(),
                &now,
                cmd.dry_run,
            )?;
            print_context_pack_lifecycle_result(&result);
        }
        ContextPackSubcommand::Retire(cmd) => {
            let actor = cmd.actor.unwrap_or_else(default_context_pack_actor);
            let now = context_pack_timestamp();
            let result = retire_context_pack(
                &paths,
                &cmd.selector,
                &actor,
                &cmd.reason,
                &now,
                cmd.dry_run,
            )?;
            print_context_pack_lifecycle_result(&result);
        }
        ContextPackSubcommand::Rollback(cmd) => {
            let actor = cmd.actor.unwrap_or_else(default_context_pack_actor);
            let now = context_pack_timestamp();
            let result = rollback_context_pack(
                &paths,
                cmd.selector.as_deref(),
                &actor,
                &cmd.reason,
                &now,
                cmd.dry_run,
            )?;
            print_context_pack_lifecycle_result(&result);
        }
        ContextPackSubcommand::Lineage(cmd) => {
            let lineage = context_pack_lineage(&paths, cmd.selector.as_deref())?;
            if cmd.json {
                println!("{}", serde_json::to_string_pretty(&lineage)?);
            } else {
                for entry in lineage {
                    let previous = entry.previous_pack_id.unwrap_or_else(|| "none".to_string());
                    let broken = entry
                        .broken_previous_pack_id
                        .map(|pack_id| format!(" broken_previous={pack_id}"))
                        .unwrap_or_default();
                    println!(
                        "{}  {:?}  previous={}{}",
                        entry.pack_id, entry.status, previous, broken
                    );
                }
            }
        }
    }

    Ok(())
}

async fn load_config_for_local_command(
    root_config_overrides: CliConfigOverrides,
    interactive: TuiCli,
) -> anyhow::Result<Config> {
    let shared = interactive.shared.into_inner();
    let mut cli_kv_overrides = root_config_overrides
        .parse_overrides()
        .map_err(anyhow::Error::msg)?;
    if interactive.web_search {
        cli_kv_overrides.push((
            "web_search".to_string(),
            toml::Value::String("live".to_string()),
        ));
    }

    let approval_policy = if shared.dangerously_bypass_approvals_and_sandbox {
        Some(AskForApproval::Never)
    } else {
        interactive.approval_policy.map(Into::into)
    };
    let sandbox_mode = if shared.dangerously_bypass_approvals_and_sandbox {
        Some(codex_protocol::config_types::SandboxMode::DangerFullAccess)
    } else {
        shared.sandbox_mode.map(Into::into)
    };
    let overrides = ConfigOverrides {
        model: shared.model,
        config_profile: shared.config_profile,
        approval_policy,
        sandbox_mode,
        cwd: shared.cwd,
        show_raw_agent_reasoning: shared.oss.then_some(true),
        ephemeral: Some(true),
        additional_writable_roots: shared.add_dir,
        ..Default::default()
    };
    Ok(Config::load_with_cli_overrides_and_harness_overrides(cli_kv_overrides, overrides).await?)
}

fn configured_context_pack_paths(config: &Config) -> anyhow::Result<Vec<AbsolutePathBuf>> {
    config
        .context_packs
        .diagnostics()
        .iter()
        .map(|diagnostic| {
            AbsolutePathBuf::try_from(PathBuf::from(&diagnostic.path)).map_err(|_| {
                anyhow::anyhow!(
                    "configured context pack path is not absolute: {}",
                    diagnostic.path
                )
            })
        })
        .collect()
}

fn resolve_context_pack_path(config: &Config, selector: &str) -> anyhow::Result<AbsolutePathBuf> {
    let trimmed = selector.trim();
    if trimmed.is_empty() {
        anyhow::bail!("context pack selector must not be empty");
    }

    let matches = config
        .context_packs
        .diagnostics()
        .iter()
        .filter(|diagnostic| {
            diagnostic.pack_id.as_deref() == Some(trimmed) || diagnostic.path == trimmed
        })
        .collect::<Vec<_>>();

    match matches.as_slice() {
        [diagnostic] => AbsolutePathBuf::try_from(PathBuf::from(&diagnostic.path)).map_err(|_| {
            anyhow::anyhow!(
                "configured context pack path is not absolute: {}",
                diagnostic.path
            )
        }),
        [] => anyhow::bail!("no configured context pack matches `{trimmed}`"),
        _ => anyhow::bail!("multiple configured context packs match `{trimmed}`"),
    }
}

fn resolve_context_pack_cli_path(config: &Config, path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        config.cwd.join(path).to_path_buf()
    }
}

fn repository_name_for_context_pack(config: &Config) -> String {
    config
        .cwd
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("aegis-code")
        .to_string()
}

fn register_context_pack_candidate_paths(
    config: &Config,
    result: &LearnedPackCompileResult,
) -> anyhow::Result<Vec<String>> {
    let mut paths = configured_context_pack_paths(config)?;
    let mut registered = Vec::new();
    for candidate in &result.candidates {
        let path = AbsolutePathBuf::try_from(PathBuf::from(&candidate.path))
            .map_err(|_| anyhow::anyhow!("candidate path is not absolute: {}", candidate.path))?;
        if paths.iter().any(|existing| existing == &path) {
            continue;
        }
        registered.push(path.display().to_string());
        paths.push(path);
    }
    if registered.is_empty() {
        return Ok(registered);
    }

    ConfigEditsBuilder::new(config.codex_home.as_path())
        .with_edits([ConfigEdit::SetPath {
            segments: vec!["context_pack_paths".to_string()],
            value: string_array(paths.iter().map(|path| path.display().to_string())),
        }])
        .apply_blocking()?;
    Ok(registered)
}

fn string_array(values: impl IntoIterator<Item = String>) -> TomlItem {
    let mut array = Array::new();
    for value in values {
        array.push(value);
    }
    TomlItem::Value(array.into())
}

fn print_learned_pack_compile_result(
    result: &LearnedPackCompileResult,
    registered_paths: &[String],
    no_register: bool,
) {
    if result.dry_run {
        println!("Dry run; no files changed.");
    }
    if result.candidates.is_empty() {
        println!(
            "No learned context-pack candidates met min support {}.",
            result.min_support
        );
    } else {
        for candidate in &result.candidates {
            println!(
                "candidate {}  {:?}  support={}  {}",
                candidate.pack_id, candidate.group_kind, candidate.support_count, candidate.path
            );
        }
    }
    for skipped in &result.skipped_groups {
        println!(
            "skipped {:?} support={}: {}",
            skipped.group_kind, skipped.support_count, skipped.reason
        );
    }
    for diagnostic in &result.diagnostics {
        eprintln!("diagnostic: {diagnostic}");
    }
    if !registered_paths.is_empty() {
        println!(
            "Registered {} context-pack path(s).",
            registered_paths.len()
        );
    } else if no_register && !result.candidates.is_empty() {
        println!("Registration skipped by --no-register.");
    }
}

fn diagnostic_status_matches(
    diagnostic: &ContextPackDiagnostic,
    status: ContextPackStatusArg,
) -> bool {
    match status {
        ContextPackStatusArg::Candidate => {
            diagnostic.diagnostic_status() == ContextPackDiagnosticStatus::Candidate
        }
        ContextPackStatusArg::Promoted => {
            diagnostic.diagnostic_status() == ContextPackDiagnosticStatus::Promoted
        }
        ContextPackStatusArg::Retired => {
            diagnostic.diagnostic_status() == ContextPackDiagnosticStatus::Retired
        }
        ContextPackStatusArg::Invalid => {
            diagnostic.diagnostic_status() == ContextPackDiagnosticStatus::Invalid
        }
        ContextPackStatusArg::Unreadable => {
            diagnostic.diagnostic_status() == ContextPackDiagnosticStatus::Unreadable
        }
        ContextPackStatusArg::All => true,
    }
}

fn print_context_pack_list(diagnostics: &[ContextPackDiagnostic]) {
    if diagnostics.is_empty() {
        println!("No configured context packs match.");
        return;
    }

    for diagnostic in diagnostics {
        let pack_id = diagnostic.pack_id.as_deref().unwrap_or("unknown");
        println!(
            "{}  {}  {}  active={}  {}",
            pack_id,
            kind_label(diagnostic.kind),
            diagnostic_status_label(diagnostic.diagnostic_status()),
            diagnostic.active,
            diagnostic.path
        );
        if !diagnostic.active {
            println!("  reason: {}", diagnostic.reason);
        }
    }
}

fn print_context_pack_inspection(inspection: &ContextPackInspection) {
    println!("{}  {}", inspection.pack_id, inspection.path);
    println!(
        "kind={} schema={} status={}",
        kind_label(Some(inspection.kind)),
        inspection.schema_version,
        promotion_status_label(inspection.promotion.status)
    );
    println!("name={}", inspection.name);
    if let Some(description) = &inspection.description {
        println!("description={description}");
    }
    if let Some(promoted_at) = &inspection.promotion.promoted_at
        && !promoted_at.is_empty()
    {
        println!("promoted_at={promoted_at}");
    }
    if let Some(promoted_by) = &inspection.promotion.promoted_by
        && !promoted_by.is_empty()
    {
        println!("promoted_by={promoted_by}");
    }
    if !inspection.promotion.source_evidence.is_empty() {
        println!(
            "source_evidence={}",
            inspection.promotion.source_evidence.join(", ")
        );
    }
    if let Some(retired_at) = &inspection.promotion.retired_at {
        println!("retired_at={retired_at}");
    }
    if let Some(retired_by) = &inspection.promotion.retired_by {
        println!("retired_by={retired_by}");
    }
    if let Some(reason) = &inspection.promotion.retire_reason {
        println!("retire_reason={reason}");
    }
    if let Some(rollback) = &inspection.rollback {
        println!(
            "rollback_previous={}",
            rollback.previous_pack_id.as_deref().unwrap_or("")
        );
        println!(
            "rollback_reason={}",
            rollback.reason.as_deref().unwrap_or("")
        );
    }
    if let Some(provenance) = &inspection.provenance {
        if let Some(author) = &provenance.author {
            println!("provenance_author={author}");
        }
        if let Some(source) = &provenance.source {
            println!("provenance_source={source}");
        }
        if !provenance.source_refs.is_empty() {
            println!("source_refs={}", provenance.source_refs.join(", "));
        }
    }
    for requirement in &inspection.evidence_requirements {
        println!("evidence:{}  {}", requirement.id, requirement.description);
    }
    if let Some(provider_defaults) = &inspection.provider_defaults {
        if let Some(preferred) = &provider_defaults.preferred {
            println!("provider_preferred={preferred}");
        }
        if !provider_defaults.fallbacks.is_empty() {
            println!(
                "provider_fallbacks={}",
                provider_defaults.fallbacks.join(", ")
            );
        }
    }
    if let Some(guidance) = &inspection.guidance {
        for item in guidance {
            println!("guidance:{} [{}]", item.id, item.category);
            println!("{}", item.text);
            if !item.falsifiers.is_empty() {
                println!("falsifiers={}", item.falsifiers.join(" | "));
            }
        }
    }
}

fn print_context_pack_lifecycle_result(result: &ContextPackLifecycleResult) {
    if result.dry_run {
        println!("Dry run; no files changed.");
    }
    if result.changes.is_empty() {
        println!("No context-pack lifecycle changes.");
        return;
    }

    for change in &result.changes {
        println!(
            "{} {}: {} -> {} ({})",
            lifecycle_action_label(change.action),
            change.pack_id,
            promotion_status_label(change.from),
            promotion_status_label(change.to),
            change.path
        );
    }
}

fn default_context_pack_actor() -> String {
    let name = git_config_value("user.name");
    let email = git_config_value("user.email");
    match (name, email) {
        (Some(name), Some(email)) => format!("{name} <{email}>"),
        (Some(name), None) => name,
        (None, Some(email)) => email,
        (None, None) => std::env::var("USER").unwrap_or_else(|_| "unknown".to_string()),
    }
}

fn git_config_value(key: &str) -> Option<String> {
    let output = ProcessCommand::new("git")
        .args(["config", "--get", key])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn context_pack_timestamp() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn kind_label(kind: Option<ContextPackKind>) -> &'static str {
    match kind {
        Some(ContextPackKind::User) => "user",
        Some(ContextPackKind::Project) => "project",
        Some(ContextPackKind::Learned) => "learned",
        None => "unknown",
    }
}

fn diagnostic_status_label(status: ContextPackDiagnosticStatus) -> &'static str {
    match status {
        ContextPackDiagnosticStatus::Candidate => "candidate",
        ContextPackDiagnosticStatus::Promoted => "promoted",
        ContextPackDiagnosticStatus::Retired => "retired",
        ContextPackDiagnosticStatus::Invalid => "invalid",
        ContextPackDiagnosticStatus::Unreadable => "unreadable",
    }
}

fn promotion_status_label(status: PromotionStatus) -> &'static str {
    match status {
        PromotionStatus::Candidate => "candidate",
        PromotionStatus::Promoted => "promoted",
        PromotionStatus::Retired => "retired",
    }
}

fn lifecycle_action_label(action: ContextPackLifecycleAction) -> &'static str {
    match action {
        ContextPackLifecycleAction::Promote => "promote",
        ContextPackLifecycleAction::Retire => "retire",
        ContextPackLifecycleAction::RollbackRestore => "rollback-restore",
    }
}

async fn run_debug_prompt_input_command(
    cmd: DebugPromptInputCommand,
    root_config_overrides: CliConfigOverrides,
    interactive: TuiCli,
    arg0_paths: Arg0DispatchPaths,
) -> anyhow::Result<()> {
    let shared = interactive.shared.into_inner();
    let mut cli_kv_overrides = root_config_overrides
        .parse_overrides()
        .map_err(anyhow::Error::msg)?;
    if interactive.web_search {
        cli_kv_overrides.push((
            "web_search".to_string(),
            toml::Value::String("live".to_string()),
        ));
    }

    let approval_policy = if shared.dangerously_bypass_approvals_and_sandbox {
        Some(AskForApproval::Never)
    } else {
        interactive.approval_policy.map(Into::into)
    };
    let sandbox_mode = if shared.dangerously_bypass_approvals_and_sandbox {
        Some(codex_protocol::config_types::SandboxMode::DangerFullAccess)
    } else {
        shared.sandbox_mode.map(Into::into)
    };
    let overrides = ConfigOverrides {
        model: shared.model,
        config_profile: shared.config_profile,
        approval_policy,
        sandbox_mode,
        cwd: shared.cwd,
        codex_self_exe: arg0_paths.codex_self_exe,
        codex_linux_sandbox_exe: arg0_paths.codex_linux_sandbox_exe,
        main_execve_wrapper_exe: arg0_paths.main_execve_wrapper_exe,
        show_raw_agent_reasoning: shared.oss.then_some(true),
        ephemeral: Some(true),
        additional_writable_roots: shared.add_dir,
        ..Default::default()
    };
    let config =
        Config::load_with_cli_overrides_and_harness_overrides(cli_kv_overrides, overrides).await?;

    let mut input = shared
        .images
        .into_iter()
        .chain(cmd.images)
        .map(|path| UserInput::LocalImage { path })
        .collect::<Vec<_>>();
    if let Some(prompt) = cmd.prompt.or(interactive.prompt) {
        input.push(UserInput::Text {
            text: prompt.replace("\r\n", "\n").replace('\r', "\n"),
            text_elements: Vec::new(),
        });
    }

    let prompt_input = codex_core::build_prompt_input(config, input, /*state_db*/ None).await?;
    println!("{}", serde_json::to_string_pretty(&prompt_input)?);

    Ok(())
}

async fn run_debug_prompt_layers_command(
    cmd: DebugPromptLayersCommand,
    root_config_overrides: CliConfigOverrides,
    interactive: TuiCli,
    arg0_paths: Arg0DispatchPaths,
) -> anyhow::Result<()> {
    let shared = interactive.shared.into_inner();
    let mut cli_kv_overrides = root_config_overrides
        .parse_overrides()
        .map_err(anyhow::Error::msg)?;
    if interactive.web_search {
        cli_kv_overrides.push((
            "web_search".to_string(),
            toml::Value::String("live".to_string()),
        ));
    }

    let approval_policy = if shared.dangerously_bypass_approvals_and_sandbox {
        Some(AskForApproval::Never)
    } else {
        interactive.approval_policy.map(Into::into)
    };
    let sandbox_mode = if shared.dangerously_bypass_approvals_and_sandbox {
        Some(codex_protocol::config_types::SandboxMode::DangerFullAccess)
    } else {
        shared.sandbox_mode.map(Into::into)
    };
    let overrides = ConfigOverrides {
        model: shared.model,
        config_profile: shared.config_profile,
        approval_policy,
        sandbox_mode,
        cwd: shared.cwd,
        codex_self_exe: arg0_paths.codex_self_exe,
        codex_linux_sandbox_exe: arg0_paths.codex_linux_sandbox_exe,
        main_execve_wrapper_exe: arg0_paths.main_execve_wrapper_exe,
        show_raw_agent_reasoning: shared.oss.then_some(true),
        ephemeral: Some(true),
        additional_writable_roots: shared.add_dir,
        ..Default::default()
    };
    let config =
        Config::load_with_cli_overrides_and_harness_overrides(cli_kv_overrides, overrides).await?;

    let mut input = shared
        .images
        .into_iter()
        .chain(cmd.images)
        .map(|path| UserInput::LocalImage { path })
        .collect::<Vec<_>>();
    if let Some(prompt) = cmd.prompt.or(interactive.prompt) {
        input.push(UserInput::Text {
            text: prompt.replace("\r\n", "\n").replace('\r', "\n"),
            text_elements: Vec::new(),
        });
    }

    let prompt_layers = codex_core::build_prompt_layers(config, input, /*state_db*/ None).await?;
    println!("{}", serde_json::to_string_pretty(&prompt_layers)?);

    Ok(())
}

async fn run_debug_models_command(
    cmd: DebugModelsCommand,
    root_config_overrides: CliConfigOverrides,
) -> anyhow::Result<()> {
    let catalog = if cmd.bundled {
        bundled_models_response()?
    } else {
        let cli_overrides = root_config_overrides
            .parse_overrides()
            .map_err(anyhow::Error::msg)?;
        let config = Config::load_with_cli_overrides(cli_overrides).await?;
        let auth_manager =
            AuthManager::shared_from_config(&config, /*enable_codex_api_key_env*/ true).await;
        let models_manager = build_models_manager(&config, auth_manager);
        models_manager
            .raw_model_catalog(RefreshStrategy::OnlineIfUncached)
            .await
    };

    serde_json::to_writer(std::io::stdout(), &catalog)?;
    println!();
    Ok(())
}

async fn run_debug_clear_memories_command(
    root_config_overrides: &CliConfigOverrides,
    interactive: &TuiCli,
) -> anyhow::Result<()> {
    let cli_kv_overrides = root_config_overrides
        .parse_overrides()
        .map_err(anyhow::Error::msg)?;
    let overrides = ConfigOverrides {
        config_profile: interactive.config_profile.clone(),
        ..Default::default()
    };
    let config =
        Config::load_with_cli_overrides_and_harness_overrides(cli_kv_overrides, overrides).await?;

    let state_path = state_db_path(config.sqlite_home.as_path());
    let mut cleared_state_db = false;
    if tokio::fs::try_exists(&state_path).await? {
        let state_db =
            StateRuntime::init(config.sqlite_home.clone(), config.model_provider_id.clone())
                .await?;
        state_db.clear_memory_data().await?;
        cleared_state_db = true;
    }

    clear_memory_roots_contents(&config.codex_home).await?;

    let mut message = if cleared_state_db {
        format!("Cleared memory state from {}.", state_path.display())
    } else {
        format!("No state db found at {}.", state_path.display())
    };
    message.push_str(&format!(
        " Cleared memory directories under {}.",
        config.codex_home.display()
    ));

    println!("{message}");

    Ok(())
}

/// Prepend root-level overrides so they have lower precedence than
/// CLI-specific ones specified after the subcommand (if any).
fn prepend_config_flags(
    subcommand_config_overrides: &mut CliConfigOverrides,
    cli_config_overrides: CliConfigOverrides,
) {
    subcommand_config_overrides
        .raw_overrides
        .splice(0..0, cli_config_overrides.raw_overrides);
}

fn reject_remote_mode_for_subcommand(
    remote: Option<&str>,
    remote_auth_token_env: Option<&str>,
    subcommand: &str,
) -> anyhow::Result<()> {
    if let Some(remote) = remote {
        anyhow::bail!(
            "`--remote {remote}` is only supported for interactive TUI commands, not `aegis {subcommand}`"
        );
    }
    if remote_auth_token_env.is_some() {
        anyhow::bail!(
            "`--remote-auth-token-env` is only supported for interactive TUI commands, not `aegis {subcommand}`"
        );
    }
    Ok(())
}

fn reject_remote_mode_for_app_server_subcommand(
    remote: Option<&str>,
    remote_auth_token_env: Option<&str>,
    subcommand: Option<&AppServerSubcommand>,
) -> anyhow::Result<()> {
    let subcommand_name = match subcommand {
        None => "app-server",
        Some(AppServerSubcommand::Proxy(_)) => "app-server proxy",
        Some(AppServerSubcommand::GenerateTs(_)) => "app-server generate-ts",
        Some(AppServerSubcommand::GenerateJsonSchema(_)) => "app-server generate-json-schema",
        Some(AppServerSubcommand::GenerateInternalJsonSchema(_)) => {
            "app-server generate-internal-json-schema"
        }
    };
    reject_remote_mode_for_subcommand(remote, remote_auth_token_env, subcommand_name)
}

fn read_remote_auth_token_from_env_var_with<F>(
    env_var_name: &str,
    get_var: F,
) -> anyhow::Result<String>
where
    F: FnOnce(&str) -> Result<String, std::env::VarError>,
{
    let auth_token = get_var(env_var_name)
        .map_err(|_| anyhow::anyhow!("environment variable `{env_var_name}` is not set"))?;
    let auth_token = auth_token.trim().to_string();
    if auth_token.is_empty() {
        anyhow::bail!("environment variable `{env_var_name}` is empty");
    }
    Ok(auth_token)
}

fn read_remote_auth_token_from_env_var(env_var_name: &str) -> anyhow::Result<String> {
    read_remote_auth_token_from_env_var_with(env_var_name, |name| std::env::var(name))
}

async fn run_interactive_tui(
    mut interactive: TuiCli,
    remote: Option<String>,
    remote_auth_token_env: Option<String>,
    arg0_paths: Arg0DispatchPaths,
) -> std::io::Result<AppExitInfo> {
    if let Some(prompt) = interactive.prompt.take() {
        // Normalize CRLF/CR to LF so CLI-provided text can't leak `\r` into TUI state.
        interactive.prompt = Some(prompt.replace("\r\n", "\n").replace('\r', "\n"));
    }

    let terminal_info = codex_terminal_detection::terminal_info();
    if terminal_info.name == TerminalName::Dumb {
        if !(std::io::stdin().is_terminal() && std::io::stderr().is_terminal()) {
            return Ok(AppExitInfo::fatal(
                "TERM is set to \"dumb\". Refusing to start the interactive TUI because no terminal is available for a confirmation prompt (stdin/stderr is not a TTY). Run in a supported terminal or unset TERM.",
            ));
        }

        eprintln!(
            "WARNING: TERM is set to \"dumb\". Codex's interactive TUI may not work in this terminal."
        );
        if !confirm("Continue anyway? [y/N]: ")? {
            return Ok(AppExitInfo::fatal(
                "Refusing to start the interactive TUI because TERM is set to \"dumb\". Run in a supported terminal or unset TERM.",
            ));
        }
    }

    let normalized_remote = remote
        .as_deref()
        .map(codex_tui::normalize_remote_addr)
        .transpose()
        .map_err(std::io::Error::other)?;
    if remote_auth_token_env.is_some() && normalized_remote.is_none() {
        return Ok(AppExitInfo::fatal(
            "`--remote-auth-token-env` requires `--remote`.",
        ));
    }
    let remote_auth_token = remote_auth_token_env
        .as_deref()
        .map(read_remote_auth_token_from_env_var)
        .transpose()
        .map_err(std::io::Error::other)?;
    codex_tui::run_main(
        interactive,
        arg0_paths,
        codex_config::LoaderOverrides::default(),
        normalized_remote,
        remote_auth_token,
    )
    .await
}

fn confirm(prompt: &str) -> std::io::Result<bool> {
    eprintln!("{prompt}");

    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let answer = input.trim();
    Ok(answer.eq_ignore_ascii_case("y") || answer.eq_ignore_ascii_case("yes"))
}

/// Build the final `TuiCli` for a `aegis resume` invocation.
fn finalize_resume_interactive(
    mut interactive: TuiCli,
    root_config_overrides: CliConfigOverrides,
    session_id: Option<String>,
    last: bool,
    show_all: bool,
    include_non_interactive: bool,
    resume_cli: TuiCli,
) -> TuiCli {
    // Start with the parsed interactive CLI so resume shares the same
    // configuration surface area as `aegis` without additional flags.
    let resume_session_id = session_id;
    interactive.resume_picker = resume_session_id.is_none() && !last;
    interactive.resume_last = last;
    interactive.resume_session_id = resume_session_id;
    interactive.resume_show_all = show_all;
    interactive.resume_include_non_interactive = include_non_interactive;

    // Merge resume-scoped flags and overrides with highest precedence.
    merge_interactive_cli_flags(&mut interactive, resume_cli);

    // Propagate any root-level config overrides (e.g. `-c key=value`).
    prepend_config_flags(&mut interactive.config_overrides, root_config_overrides);

    interactive
}

/// Build the final `TuiCli` for a `aegis fork` invocation.
fn finalize_fork_interactive(
    mut interactive: TuiCli,
    root_config_overrides: CliConfigOverrides,
    session_id: Option<String>,
    last: bool,
    show_all: bool,
    fork_cli: TuiCli,
) -> TuiCli {
    // Start with the parsed interactive CLI so fork shares the same
    // configuration surface area as `aegis` without additional flags.
    let fork_session_id = session_id;
    interactive.fork_picker = fork_session_id.is_none() && !last;
    interactive.fork_last = last;
    interactive.fork_session_id = fork_session_id;
    interactive.fork_show_all = show_all;

    // Merge fork-scoped flags and overrides with highest precedence.
    merge_interactive_cli_flags(&mut interactive, fork_cli);

    // Propagate any root-level config overrides (e.g. `-c key=value`).
    prepend_config_flags(&mut interactive.config_overrides, root_config_overrides);

    interactive
}

/// Merge flags provided to `aegis resume`/`aegis fork` so they take precedence over any
/// root-level flags. Only overrides fields explicitly set on the subcommand-scoped
/// CLI. Also appends `-c key=value` overrides with highest precedence.
fn merge_interactive_cli_flags(interactive: &mut TuiCli, subcommand_cli: TuiCli) {
    let TuiCli {
        shared,
        approval_policy,
        web_search,
        prompt,
        config_overrides,
        ..
    } = subcommand_cli;
    interactive
        .shared
        .apply_subcommand_overrides(shared.into_inner());
    if let Some(approval) = approval_policy {
        interactive.approval_policy = Some(approval);
    }
    if web_search {
        interactive.web_search = true;
    }
    if let Some(prompt) = prompt {
        // Normalize CRLF/CR to LF so CLI-provided text can't leak `\r` into TUI state.
        interactive.prompt = Some(prompt.replace("\r\n", "\n").replace('\r', "\n"));
    }

    interactive
        .config_overrides
        .raw_overrides
        .extend(config_overrides.raw_overrides);
}

fn print_completion(cmd: CompletionCommand) {
    let mut app = MultitoolCli::command();
    let name = "aegis";
    generate(cmd.shell, &mut app, name, &mut std::io::stdout());
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use codex_protocol::ThreadId;
    use codex_tui::TokenUsage;
    use pretty_assertions::assert_eq;

    fn finalize_resume_from_args(args: &[&str]) -> TuiCli {
        let cli = MultitoolCli::try_parse_from(args).expect("parse");
        let MultitoolCli {
            interactive,
            config_overrides: root_overrides,
            subcommand,
            feature_toggles: _,
            remote: _,
        } = cli;

        let Subcommand::Resume(ResumeCommand {
            session_id,
            last,
            all,
            include_non_interactive,
            remote: _,
            config_overrides: resume_cli,
        }) = subcommand.expect("resume present")
        else {
            unreachable!()
        };

        finalize_resume_interactive(
            interactive,
            root_overrides,
            session_id,
            last,
            all,
            include_non_interactive,
            resume_cli,
        )
    }

    fn finalize_fork_from_args(args: &[&str]) -> TuiCli {
        let cli = MultitoolCli::try_parse_from(args).expect("parse");
        let MultitoolCli {
            interactive,
            config_overrides: root_overrides,
            subcommand,
            feature_toggles: _,
            remote: _,
        } = cli;

        let Subcommand::Fork(ForkCommand {
            session_id,
            last,
            all,
            remote: _,
            config_overrides: fork_cli,
        }) = subcommand.expect("fork present")
        else {
            unreachable!()
        };

        finalize_fork_interactive(interactive, root_overrides, session_id, last, all, fork_cli)
    }

    #[test]
    fn exec_resume_last_accepts_prompt_positional() {
        let cli =
            MultitoolCli::try_parse_from(["aegis", "exec", "--json", "resume", "--last", "2+2"])
                .expect("parse should succeed");

        let Some(Subcommand::Exec(exec)) = cli.subcommand else {
            panic!("expected exec subcommand");
        };
        let Some(codex_exec::Command::Resume(args)) = exec.command else {
            panic!("expected exec resume");
        };

        assert!(args.last);
        assert_eq!(args.session_id, None);
        assert_eq!(args.prompt.as_deref(), Some("2+2"));
    }

    #[test]
    fn exec_resume_accepts_output_last_message_flag_after_subcommand() {
        let cli = MultitoolCli::try_parse_from([
            "aegis",
            "exec",
            "resume",
            "session-123",
            "-o",
            "/tmp/resume-output.md",
            "re-review",
        ])
        .expect("parse should succeed");

        let Some(Subcommand::Exec(exec)) = cli.subcommand else {
            panic!("expected exec subcommand");
        };
        let Some(codex_exec::Command::Resume(args)) = exec.command else {
            panic!("expected exec resume");
        };

        assert_eq!(
            exec.last_message_file,
            Some(std::path::PathBuf::from("/tmp/resume-output.md"))
        );
        assert_eq!(args.session_id.as_deref(), Some("session-123"));
        assert_eq!(args.prompt.as_deref(), Some("re-review"));
    }

    #[test]
    fn dangerous_bypass_conflicts_with_approval_policy() {
        let err = MultitoolCli::try_parse_from([
            "aegis",
            "--dangerously-bypass-approvals-and-sandbox",
            "--ask-for-approval",
            "on-request",
        ])
        .expect_err("conflicting permission flags should be rejected");

        assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
    }

    fn app_server_from_args(args: &[&str]) -> AppServerCommand {
        let cli = MultitoolCli::try_parse_from(args).expect("parse");
        let Subcommand::AppServer(app_server) = cli.subcommand.expect("app-server present") else {
            unreachable!()
        };
        app_server
    }

    fn default_app_server_socket_path() -> AbsolutePathBuf {
        let codex_home = find_codex_home().expect("aegis home");
        codex_app_server::app_server_control_socket_path(&codex_home)
            .expect("default app-server socket path")
    }

    #[test]
    fn debug_prompt_input_parses_prompt_and_images() {
        let cli = MultitoolCli::try_parse_from([
            "aegis",
            "debug",
            "prompt-input",
            "hello",
            "--image",
            "/tmp/a.png,/tmp/b.png",
        ])
        .expect("parse");

        let Some(Subcommand::Debug(DebugCommand {
            subcommand: DebugSubcommand::PromptInput(cmd),
        })) = cli.subcommand
        else {
            panic!("expected debug prompt-input subcommand");
        };

        assert_eq!(cmd.prompt.as_deref(), Some("hello"));
        assert_eq!(
            cmd.images,
            vec![PathBuf::from("/tmp/a.png"), PathBuf::from("/tmp/b.png")]
        );
    }

    #[test]
    fn debug_prompt_layers_parses_prompt_and_images() {
        let cli = MultitoolCli::try_parse_from([
            "aegis",
            "debug",
            "prompt-layers",
            "hello",
            "--image",
            "/tmp/a.png,/tmp/b.png",
        ])
        .expect("parse");

        let Some(Subcommand::Debug(DebugCommand {
            subcommand: DebugSubcommand::PromptLayers(cmd),
        })) = cli.subcommand
        else {
            panic!("expected debug prompt-layers subcommand");
        };

        assert_eq!(cmd.prompt.as_deref(), Some("hello"));
        assert_eq!(
            cmd.images,
            vec![PathBuf::from("/tmp/a.png"), PathBuf::from("/tmp/b.png")]
        );
    }

    #[test]
    fn debug_models_parses_bundled_flag() {
        let cli =
            MultitoolCli::try_parse_from(["aegis", "debug", "models", "--bundled"]).expect("parse");

        let Some(Subcommand::Debug(DebugCommand {
            subcommand: DebugSubcommand::Models(cmd),
        })) = cli.subcommand
        else {
            panic!("expected debug models subcommand");
        };

        assert!(cmd.bundled);
    }

    #[test]
    fn doctor_parses_json_flag() {
        let cli = MultitoolCli::try_parse_from(["aegis", "doctor", "--json"]).expect("parse");

        let Some(Subcommand::Doctor(cmd)) = cli.subcommand else {
            panic!("expected doctor subcommand");
        };

        assert!(cmd.json);
    }

    #[test]
    fn doctor_accepts_root_local_provider_flags() {
        let cli = MultitoolCli::try_parse_from([
            "aegis",
            "--oss",
            "--local-provider",
            "ollama",
            "doctor",
            "--json",
        ])
        .expect("parse");

        let Some(Subcommand::Doctor(cmd)) = cli.subcommand else {
            panic!("expected doctor subcommand");
        };

        assert!(cmd.json);
        assert!(cli.interactive.oss);
        assert_eq!(cli.interactive.oss_provider.as_deref(), Some("ollama"));
    }

    #[test]
    fn responses_subcommand_is_not_registered() {
        let command = MultitoolCli::command();
        assert!(
            command
                .get_subcommands()
                .all(|subcommand| subcommand.get_name() != "responses")
        );
    }

    fn help_from_args(args: &[&str]) -> String {
        let err = MultitoolCli::try_parse_from(args).expect_err("help should short-circuit");
        assert_eq!(err.kind(), clap::error::ErrorKind::DisplayHelp);
        err.to_string()
    }

    #[test]
    fn plugin_marketplace_help_uses_plugin_namespace() {
        let help = help_from_args(&["aegis", "plugin", "marketplace", "--help"]);
        assert!(
            help.contains("Usage: aegis plugin marketplace [OPTIONS] <COMMAND>"),
            "{help}"
        );

        for (subcommand, usage) in [
            ("add", "Usage: aegis plugin marketplace add"),
            ("upgrade", "Usage: aegis plugin marketplace upgrade"),
            ("remove", "Usage: aegis plugin marketplace remove"),
        ] {
            let help = help_from_args(&["aegis", "plugin", "marketplace", subcommand, "--help"]);
            assert!(help.contains(usage), "{help}");
        }
    }

    #[test]
    fn plugin_marketplace_add_parses_under_plugin() {
        let cli =
            MultitoolCli::try_parse_from(["aegis", "plugin", "marketplace", "add", "owner/repo"])
                .expect("parse");

        assert!(matches!(cli.subcommand, Some(Subcommand::Plugin(_))));
    }

    #[test]
    fn plugin_marketplace_upgrade_parses_under_plugin() {
        let cli =
            MultitoolCli::try_parse_from(["aegis", "plugin", "marketplace", "upgrade", "debug"])
                .expect("parse");

        assert!(matches!(cli.subcommand, Some(Subcommand::Plugin(_))));
    }

    #[test]
    fn update_parses_as_update_subcommand() {
        let cli = MultitoolCli::try_parse_from(["aegis", "update"]).expect("parse");
        assert!(matches!(cli.subcommand, Some(Subcommand::Update)));
    }

    #[test]
    fn sandbox_macos_parses_permissions_profile() {
        let cli = MultitoolCli::try_parse_from([
            "aegis",
            "sandbox",
            "macos",
            "--permissions-profile",
            ":workspace",
            "--",
            "echo",
        ])
        .expect("parse");

        let Some(Subcommand::Sandbox(SandboxArgs {
            cmd: SandboxCommand::Macos(command),
        })) = cli.subcommand
        else {
            panic!("expected sandbox macos command");
        };

        assert_eq!(command.permissions_profile.as_deref(), Some(":workspace"));
        assert_eq!(command.command, vec!["echo"]);
    }

    #[test]
    fn sandbox_macos_rejects_explicit_profile_controls_without_profile() {
        let err = MultitoolCli::try_parse_from(["aegis", "sandbox", "macos", "-C", "/tmp"])
            .expect_err("parse should fail");

        assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn plugin_marketplace_remove_parses_under_plugin() {
        let cli =
            MultitoolCli::try_parse_from(["aegis", "plugin", "marketplace", "remove", "debug"])
                .expect("parse");

        assert!(matches!(cli.subcommand, Some(Subcommand::Plugin(_))));
    }

    #[test]
    fn marketplace_no_longer_parses_at_top_level() {
        let add_result =
            MultitoolCli::try_parse_from(["aegis", "marketplace", "add", "owner/repo"]);
        assert!(add_result.is_err());

        let upgrade_result =
            MultitoolCli::try_parse_from(["aegis", "marketplace", "upgrade", "debug"]);
        assert!(upgrade_result.is_err());

        let remove_result =
            MultitoolCli::try_parse_from(["aegis", "marketplace", "remove", "debug"]);
        assert!(remove_result.is_err());
    }

    #[test]
    fn full_auto_no_longer_parses_at_top_level() {
        let result = MultitoolCli::try_parse_from(["aegis", "--full-auto"]);

        assert!(result.is_err());
    }

    #[test]
    fn exec_full_auto_reports_migration_path() {
        let cli = MultitoolCli::try_parse_from(["aegis", "exec", "--full-auto", "summarize"])
            .expect("exec should accept removed flag long enough to report a migration path");
        let Some(Subcommand::Exec(exec)) = cli.subcommand else {
            panic!("expected exec subcommand");
        };

        assert_eq!(
            exec.removed_full_auto_warning(),
            Some("warning: `--full-auto` is deprecated; use `--sandbox workspace-write` instead.")
        );
    }

    #[test]
    fn sandbox_full_auto_no_longer_parses() {
        let result =
            MultitoolCli::try_parse_from(["aegis", "sandbox", "linux", "--full-auto", "--"]);

        assert!(result.is_err());
    }

    fn sample_exit_info(conversation_id: Option<&str>, thread_name: Option<&str>) -> AppExitInfo {
        let token_usage = TokenUsage {
            output_tokens: 2,
            total_tokens: 2,
            ..Default::default()
        };
        AppExitInfo {
            token_usage,
            thread_id: conversation_id
                .map(ThreadId::from_string)
                .map(Result::unwrap),
            thread_name: thread_name.map(str::to_string),
            update_action: None,
            exit_reason: ExitReason::UserRequested,
        }
    }

    #[test]
    fn format_exit_messages_skips_zero_usage() {
        let exit_info = AppExitInfo {
            token_usage: TokenUsage::default(),
            thread_id: None,
            thread_name: None,
            update_action: None,
            exit_reason: ExitReason::UserRequested,
        };
        let lines = format_exit_messages(exit_info, /*color_enabled*/ false);
        assert!(lines.is_empty());
    }

    #[test]
    fn format_exit_messages_includes_resume_hint_without_color() {
        let exit_info = sample_exit_info(
            Some("123e4567-e89b-12d3-a456-426614174000"),
            /*thread_name*/ None,
        );
        let lines = format_exit_messages(exit_info, /*color_enabled*/ false);
        assert_eq!(
            lines,
            vec![
                "Token usage: total=2 input=0 output=2".to_string(),
                "To continue this session, run aegis resume 123e4567-e89b-12d3-a456-426614174000"
                    .to_string(),
            ]
        );
    }

    #[test]
    fn format_exit_messages_applies_color_when_enabled() {
        let exit_info = sample_exit_info(
            Some("123e4567-e89b-12d3-a456-426614174000"),
            /*thread_name*/ None,
        );
        let lines = format_exit_messages(exit_info, /*color_enabled*/ true);
        assert_eq!(lines.len(), 2);
        assert!(lines[1].contains("\u{1b}[36m"));
    }

    #[test]
    fn format_exit_messages_uses_id_even_when_thread_has_name() {
        let exit_info = sample_exit_info(
            Some("123e4567-e89b-12d3-a456-426614174000"),
            Some("my-thread"),
        );
        let lines = format_exit_messages(exit_info, /*color_enabled*/ false);
        assert_eq!(
            lines,
            vec![
                "Token usage: total=2 input=0 output=2".to_string(),
                "To continue this session, run aegis resume 123e4567-e89b-12d3-a456-426614174000"
                    .to_string(),
            ]
        );
    }

    #[test]
    fn resume_model_flag_applies_when_no_root_flags() {
        let interactive =
            finalize_resume_from_args(["aegis", "resume", "-m", "gpt-5.1-test"].as_ref());

        assert_eq!(interactive.model.as_deref(), Some("gpt-5.1-test"));
        assert!(interactive.resume_picker);
        assert!(!interactive.resume_last);
        assert_eq!(interactive.resume_session_id, None);
    }

    #[test]
    fn resume_picker_logic_none_and_not_last() {
        let interactive = finalize_resume_from_args(["aegis", "resume"].as_ref());
        assert!(interactive.resume_picker);
        assert!(!interactive.resume_last);
        assert_eq!(interactive.resume_session_id, None);
        assert!(!interactive.resume_show_all);
    }

    #[test]
    fn resume_picker_logic_last() {
        let interactive = finalize_resume_from_args(["aegis", "resume", "--last"].as_ref());
        assert!(!interactive.resume_picker);
        assert!(interactive.resume_last);
        assert_eq!(interactive.resume_session_id, None);
        assert!(!interactive.resume_show_all);
    }

    #[test]
    fn resume_picker_logic_with_session_id() {
        let interactive = finalize_resume_from_args(["aegis", "resume", "1234"].as_ref());
        assert!(!interactive.resume_picker);
        assert!(!interactive.resume_last);
        assert_eq!(interactive.resume_session_id.as_deref(), Some("1234"));
        assert!(!interactive.resume_show_all);
    }

    #[test]
    fn resume_all_flag_sets_show_all() {
        let interactive = finalize_resume_from_args(["aegis", "resume", "--all"].as_ref());
        assert!(interactive.resume_picker);
        assert!(interactive.resume_show_all);
    }

    #[test]
    fn resume_include_non_interactive_flag_sets_source_filter_override() {
        let interactive =
            finalize_resume_from_args(["aegis", "resume", "--include-non-interactive"].as_ref());

        assert!(interactive.resume_picker);
        assert!(interactive.resume_include_non_interactive);
    }

    #[test]
    fn resume_merges_option_flags() {
        let interactive = finalize_resume_from_args(
            [
                "aegis",
                "resume",
                "sid",
                "--oss",
                "--local-provider",
                "ollama",
                "--search",
                "--sandbox",
                "workspace-write",
                "--ask-for-approval",
                "on-request",
                "-m",
                "gpt-5.1-test",
                "-p",
                "my-profile",
                "-C",
                "/tmp",
                "-i",
                "/tmp/a.png,/tmp/b.png",
            ]
            .as_ref(),
        );

        assert_eq!(interactive.model.as_deref(), Some("gpt-5.1-test"));
        assert!(interactive.oss);
        assert_eq!(interactive.oss_provider.as_deref(), Some("ollama"));
        assert_eq!(interactive.config_profile.as_deref(), Some("my-profile"));
        assert_matches!(
            interactive.sandbox_mode,
            Some(codex_utils_cli::SandboxModeCliArg::WorkspaceWrite)
        );
        assert_matches!(
            interactive.approval_policy,
            Some(codex_utils_cli::ApprovalModeCliArg::OnRequest)
        );
        assert_eq!(
            interactive.cwd.as_deref(),
            Some(std::path::Path::new("/tmp"))
        );
        assert!(interactive.web_search);
        let has_a = interactive
            .images
            .iter()
            .any(|p| p == std::path::Path::new("/tmp/a.png"));
        let has_b = interactive
            .images
            .iter()
            .any(|p| p == std::path::Path::new("/tmp/b.png"));
        assert!(has_a && has_b);
        assert!(!interactive.resume_picker);
        assert!(!interactive.resume_last);
        assert_eq!(interactive.resume_session_id.as_deref(), Some("sid"));
    }

    #[test]
    fn resume_merges_dangerously_bypass_flag() {
        let interactive = finalize_resume_from_args(
            [
                "aegis",
                "resume",
                "--dangerously-bypass-approvals-and-sandbox",
            ]
            .as_ref(),
        );
        assert!(interactive.dangerously_bypass_approvals_and_sandbox);
        assert!(interactive.resume_picker);
        assert!(!interactive.resume_last);
        assert_eq!(interactive.resume_session_id, None);
    }

    #[test]
    fn fork_picker_logic_none_and_not_last() {
        let interactive = finalize_fork_from_args(["aegis", "fork"].as_ref());
        assert!(interactive.fork_picker);
        assert!(!interactive.fork_last);
        assert_eq!(interactive.fork_session_id, None);
        assert!(!interactive.fork_show_all);
    }

    #[test]
    fn fork_picker_logic_last() {
        let interactive = finalize_fork_from_args(["aegis", "fork", "--last"].as_ref());
        assert!(!interactive.fork_picker);
        assert!(interactive.fork_last);
        assert_eq!(interactive.fork_session_id, None);
        assert!(!interactive.fork_show_all);
    }

    #[test]
    fn fork_picker_logic_with_session_id() {
        let interactive = finalize_fork_from_args(["aegis", "fork", "1234"].as_ref());
        assert!(!interactive.fork_picker);
        assert!(!interactive.fork_last);
        assert_eq!(interactive.fork_session_id.as_deref(), Some("1234"));
        assert!(!interactive.fork_show_all);
    }

    #[test]
    fn fork_all_flag_sets_show_all() {
        let interactive = finalize_fork_from_args(["aegis", "fork", "--all"].as_ref());
        assert!(interactive.fork_picker);
        assert!(interactive.fork_show_all);
    }

    #[test]
    fn app_server_analytics_default_disabled_without_flag() {
        let app_server = app_server_from_args(["aegis", "app-server"].as_ref());
        assert!(!app_server.analytics_default_enabled);
        assert_eq!(
            app_server.listen,
            codex_app_server::AppServerTransport::Stdio
        );
    }

    #[test]
    fn app_server_analytics_default_enabled_with_flag() {
        let app_server =
            app_server_from_args(["aegis", "app-server", "--analytics-default-enabled"].as_ref());
        assert!(app_server.analytics_default_enabled);
    }

    #[test]
    fn remote_flag_parses_for_interactive_root() {
        let cli = MultitoolCli::try_parse_from(["aegis", "--remote", "ws://127.0.0.1:4500"])
            .expect("parse");
        assert_eq!(cli.remote.remote.as_deref(), Some("ws://127.0.0.1:4500"));
    }

    #[test]
    fn remote_auth_token_env_flag_parses_for_interactive_root() {
        let cli = MultitoolCli::try_parse_from([
            "aegis",
            "--remote-auth-token-env",
            "CODEX_REMOTE_AUTH_TOKEN",
            "--remote",
            "ws://127.0.0.1:4500",
        ])
        .expect("parse");
        assert_eq!(
            cli.remote.remote_auth_token_env.as_deref(),
            Some("CODEX_REMOTE_AUTH_TOKEN")
        );
    }

    #[test]
    fn remote_flag_parses_for_resume_subcommand() {
        let cli =
            MultitoolCli::try_parse_from(["aegis", "resume", "--remote", "ws://127.0.0.1:4500"])
                .expect("parse");
        let Subcommand::Resume(ResumeCommand { remote, .. }) =
            cli.subcommand.expect("resume present")
        else {
            panic!("expected resume subcommand");
        };
        assert_eq!(remote.remote.as_deref(), Some("ws://127.0.0.1:4500"));
    }

    #[test]
    fn reject_remote_mode_for_non_interactive_subcommands() {
        let err = reject_remote_mode_for_subcommand(
            Some("127.0.0.1:4500"),
            /*remote_auth_token_env*/ None,
            "exec",
        )
        .expect_err("non-interactive subcommands should reject --remote");
        assert!(
            err.to_string()
                .contains("only supported for interactive TUI commands")
        );
    }

    #[test]
    fn reject_remote_auth_token_env_for_non_interactive_subcommands() {
        let err = reject_remote_mode_for_subcommand(
            /*remote*/ None,
            Some("CODEX_REMOTE_AUTH_TOKEN"),
            "exec",
        )
        .expect_err("non-interactive subcommands should reject --remote-auth-token-env");
        assert!(
            err.to_string()
                .contains("only supported for interactive TUI commands")
        );
    }

    #[test]
    fn reject_remote_auth_token_env_for_app_server_generate_internal_json_schema() {
        let subcommand =
            AppServerSubcommand::GenerateInternalJsonSchema(GenerateInternalJsonSchemaCommand {
                out_dir: PathBuf::from("/tmp/out"),
            });
        let err = reject_remote_mode_for_app_server_subcommand(
            /*remote*/ None,
            Some("CODEX_REMOTE_AUTH_TOKEN"),
            Some(&subcommand),
        )
        .expect_err("non-interactive app-server subcommands should reject --remote-auth-token-env");
        assert!(err.to_string().contains("generate-internal-json-schema"));
    }

    #[test]
    fn read_remote_auth_token_from_env_var_reports_missing_values() {
        let err = read_remote_auth_token_from_env_var_with("CODEX_REMOTE_AUTH_TOKEN", |_| {
            Err(std::env::VarError::NotPresent)
        })
        .expect_err("missing env vars should be rejected");
        assert!(err.to_string().contains("is not set"));
    }

    #[test]
    fn read_remote_auth_token_from_env_var_trims_values() {
        let auth_token =
            read_remote_auth_token_from_env_var_with("CODEX_REMOTE_AUTH_TOKEN", |_| {
                Ok("  bearer-token  ".to_string())
            })
            .expect("env var should parse");
        assert_eq!(auth_token, "bearer-token");
    }

    #[test]
    fn read_remote_auth_token_from_env_var_rejects_empty_values() {
        let err = read_remote_auth_token_from_env_var_with("CODEX_REMOTE_AUTH_TOKEN", |_| {
            Ok(" \n\t ".to_string())
        })
        .expect_err("empty env vars should be rejected");
        assert!(err.to_string().contains("is empty"));
    }

    #[test]
    fn app_server_listen_websocket_url_parses() {
        let app_server = app_server_from_args(
            ["aegis", "app-server", "--listen", "ws://127.0.0.1:4500"].as_ref(),
        );
        assert_eq!(
            app_server.listen,
            codex_app_server::AppServerTransport::WebSocket {
                bind_address: "127.0.0.1:4500".parse().expect("valid socket address"),
            }
        );
    }

    #[test]
    fn app_server_listen_stdio_url_parses() {
        let app_server =
            app_server_from_args(["aegis", "app-server", "--listen", "stdio://"].as_ref());
        assert_eq!(
            app_server.listen,
            codex_app_server::AppServerTransport::Stdio
        );
    }

    #[test]
    fn app_server_listen_unix_socket_url_parses() {
        let app_server =
            app_server_from_args(["aegis", "app-server", "--listen", "unix://"].as_ref());
        assert_eq!(
            app_server.listen,
            codex_app_server::AppServerTransport::UnixSocket {
                socket_path: default_app_server_socket_path()
            }
        );
    }

    #[test]
    fn app_server_listen_unix_socket_path_parses() {
        let app_server = app_server_from_args(
            ["aegis", "app-server", "--listen", "unix:///tmp/codex.sock"].as_ref(),
        );
        assert_eq!(
            app_server.listen,
            codex_app_server::AppServerTransport::UnixSocket {
                socket_path: AbsolutePathBuf::from_absolute_path("/tmp/codex.sock")
                    .expect("absolute path should parse")
            }
        );
    }

    #[test]
    fn app_server_listen_off_parses() {
        let app_server = app_server_from_args(["aegis", "app-server", "--listen", "off"].as_ref());
        assert_eq!(app_server.listen, codex_app_server::AppServerTransport::Off);
    }

    #[test]
    fn app_server_listen_invalid_url_fails_to_parse() {
        let parse_result =
            MultitoolCli::try_parse_from(["aegis", "app-server", "--listen", "http://foo"]);
        assert!(parse_result.is_err());
    }

    #[test]
    fn app_server_proxy_subcommand_parses() {
        let app_server = app_server_from_args(["aegis", "app-server", "proxy"].as_ref());
        assert!(matches!(
            app_server.subcommand,
            Some(AppServerSubcommand::Proxy(AppServerProxyCommand {
                socket_path: None
            }))
        ));
    }

    #[test]
    fn app_server_proxy_sock_path_parses() {
        let app_server =
            app_server_from_args(["aegis", "app-server", "proxy", "--sock", "codex.sock"].as_ref());
        let Some(AppServerSubcommand::Proxy(proxy)) = app_server.subcommand else {
            panic!("expected proxy subcommand");
        };
        assert_eq!(
            proxy.socket_path,
            Some(
                AbsolutePathBuf::relative_to_current_dir("codex.sock")
                    .expect("relative path should resolve")
            )
        );
    }

    #[test]
    fn reject_remote_auth_token_env_for_app_server_proxy() {
        let subcommand = AppServerSubcommand::Proxy(AppServerProxyCommand { socket_path: None });
        let err = reject_remote_mode_for_app_server_subcommand(
            /*remote*/ None,
            Some("CODEX_REMOTE_AUTH_TOKEN"),
            Some(&subcommand),
        )
        .expect_err("app-server proxy should reject --remote-auth-token-env");
        assert!(err.to_string().contains("app-server proxy"));
    }

    #[test]
    fn app_server_capability_token_flags_parse() {
        let app_server = app_server_from_args(
            [
                "aegis",
                "app-server",
                "--ws-auth",
                "capability-token",
                "--ws-token-file",
                "/tmp/codex-token",
            ]
            .as_ref(),
        );
        assert_eq!(
            app_server.auth.ws_auth,
            Some(codex_app_server::WebsocketAuthCliMode::CapabilityToken)
        );
        assert_eq!(
            app_server.auth.ws_token_file,
            Some(PathBuf::from("/tmp/codex-token"))
        );
    }

    #[test]
    fn app_server_signed_bearer_flags_parse() {
        let app_server = app_server_from_args(
            [
                "aegis",
                "app-server",
                "--ws-auth",
                "signed-bearer-token",
                "--ws-shared-secret-file",
                "/tmp/codex-secret",
                "--ws-issuer",
                "issuer",
                "--ws-audience",
                "audience",
                "--ws-max-clock-skew-seconds",
                "9",
            ]
            .as_ref(),
        );
        assert_eq!(
            app_server.auth.ws_auth,
            Some(codex_app_server::WebsocketAuthCliMode::SignedBearerToken)
        );
        assert_eq!(
            app_server.auth.ws_shared_secret_file,
            Some(PathBuf::from("/tmp/codex-secret"))
        );
        assert_eq!(app_server.auth.ws_issuer.as_deref(), Some("issuer"));
        assert_eq!(app_server.auth.ws_audience.as_deref(), Some("audience"));
        assert_eq!(app_server.auth.ws_max_clock_skew_seconds, Some(9));
    }

    #[test]
    fn app_server_rejects_removed_insecure_non_loopback_flag() {
        let parse_result = MultitoolCli::try_parse_from([
            "aegis",
            "app-server",
            "--allow-unauthenticated-non-loopback-ws",
        ]);
        assert!(parse_result.is_err());
    }

    #[test]
    fn features_enable_parses_feature_name() {
        let cli = MultitoolCli::try_parse_from(["aegis", "features", "enable", "unified_exec"])
            .expect("parse should succeed");
        let Some(Subcommand::Features(FeaturesCli { sub })) = cli.subcommand else {
            panic!("expected features subcommand");
        };
        let FeaturesSubcommand::Enable(FeatureSetArgs { feature }) = sub else {
            panic!("expected features enable");
        };
        assert_eq!(feature, "unified_exec");
    }

    #[test]
    fn features_disable_parses_feature_name() {
        let cli = MultitoolCli::try_parse_from(["aegis", "features", "disable", "shell_tool"])
            .expect("parse should succeed");
        let Some(Subcommand::Features(FeaturesCli { sub })) = cli.subcommand else {
            panic!("expected features subcommand");
        };
        let FeaturesSubcommand::Disable(FeatureSetArgs { feature }) = sub else {
            panic!("expected features disable");
        };
        assert_eq!(feature, "shell_tool");
    }

    #[test]
    fn issue_train_validate_parses_repo_parent_and_json() {
        let cli = MultitoolCli::try_parse_from([
            "aegis",
            "issue-train",
            "validate",
            "--repo",
            "owner/repo",
            "--parent",
            "7",
            "--json",
        ])
        .expect("parse should succeed");

        let Some(Subcommand::IssueTrain(IssueTrainCommand { subcommand })) = cli.subcommand else {
            panic!("expected issue-train subcommand");
        };
        let IssueTrainSubcommand::Validate(cmd) = subcommand;
        assert_eq!(cmd.repo.as_deref(), Some("owner/repo"));
        assert_eq!(cmd.parent, Some(7));
        assert!(cmd.json);
    }

    #[test]
    fn issue_train_validate_uses_fake_gh_for_defaults() {
        let parent_body = "## Objective\n\nCoordinate work.\n\n## Child Issues\n\n- [ ] #2 Implement validator\n\n## Evidence Required For Closure\n\nReconcile child state.\n";
        let child_body = "## Objective\n\nShip an issue train validator.\n\n## Scope\n\nValidate issue train readiness.\n\n## Acceptance Criteria\n\n- Ready trains pass.\n\n## Falsifiers\n\n- Vague issues pass.\n\n## Dependencies\n\nNone\n";
        let parent_json = serde_json::json!({
            "number": 1,
            "title": "Plan: Method workflow",
            "state": "OPEN",
            "body": parent_body,
            "labels": [{ "name": "aegis-code:plan" }]
        })
        .to_string();
        let child_json = serde_json::json!({
            "number": 2,
            "title": "Task: Implement validator",
            "state": "OPEN",
            "body": child_body,
            "labels": [{ "name": "aegis-code:task" }]
        })
        .to_string();
        let runner = FakeGhRunner::new()
            .with_success(
                &["repo", "view", "--json", "nameWithOwner"],
                r#"{"nameWithOwner":"owner/repo"}"#,
            )
            .with_success(
                &[
                    "issue",
                    "list",
                    "--repo",
                    "owner/repo",
                    "--state",
                    "open",
                    "--label",
                    "aegis-code:plan",
                    "--json",
                    "number",
                    "--limit",
                    "50",
                ],
                r#"[{"number":1}]"#,
            )
            .with_success(
                &[
                    "issue",
                    "view",
                    "1",
                    "--repo",
                    "owner/repo",
                    "--json",
                    "number,title,state,body,labels",
                ],
                &parent_json,
            )
            .with_success(
                &[
                    "issue",
                    "view",
                    "2",
                    "--repo",
                    "owner/repo",
                    "--json",
                    "number,title,state,body,labels",
                ],
                &child_json,
            );
        let cmd = IssueTrainValidateCommand {
            repo: None,
            parent: None,
            json: true,
        };

        let report = validate_issue_train_from_github(&cmd, &runner)
            .expect("fake gh should provide a valid issue train");

        assert!(report.valid, "{report:#?}");
        assert_eq!(report.parent_issue, 1);
        assert_eq!(report.child_count, 1);
    }

    #[test]
    fn issue_train_report_json_contains_expected_fields() {
        let report = IssueTrainReport {
            valid: false,
            parent_issue: 1,
            child_count: 0,
            findings: vec![codex_core::issue_train::IssueTrainFinding {
                severity: FindingSeverity::Error,
                code: "parent_missing_child_refs".to_string(),
                issue_number: Some(1),
                message: "missing children".to_string(),
                remediation: "add child refs".to_string(),
            }],
        };

        let value = serde_json::to_value(&report).expect("serialize report");

        assert_eq!(value["valid"], false);
        assert_eq!(value["parent_issue"], 1);
        assert_eq!(value["child_count"], 0);
        assert_eq!(value["findings"][0]["code"], "parent_missing_child_refs");
    }

    #[test]
    fn pr_readiness_validate_parses_flags() {
        let cli = MultitoolCli::try_parse_from([
            "aegis",
            "pr-readiness",
            "validate",
            "--repo",
            "owner/repo",
            "--pr",
            "7",
            "--method-state",
            "/tmp/method-state.json",
            "--allowed-path",
            "codex-rs/core",
            "--json",
        ])
        .expect("parse should succeed");

        let Some(Subcommand::PrReadiness(PrReadinessCommand { subcommand })) = cli.subcommand
        else {
            panic!("expected pr-readiness subcommand");
        };
        let PrReadinessSubcommand::Validate(cmd) = subcommand;
        assert_eq!(cmd.repo.as_deref(), Some("owner/repo"));
        assert_eq!(cmd.pr, Some(7));
        assert_eq!(cmd.method_state, PathBuf::from("/tmp/method-state.json"));
        assert_eq!(cmd.allowed_paths, vec!["codex-rs/core"]);
        assert!(cmd.json);
    }

    #[test]
    fn pr_readiness_validate_uses_fake_gh_for_defaults() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let method_state_path = tempdir.path().join("method-state.json");
        std::fs::write(&method_state_path, pr_method_state_json("abc123"))
            .expect("write method state");
        let pr_body = "## Summary\n\nReady.\n\n## Allowed Paths\n\n- codex-rs/core\n\n## Issue\n\nFixes #20\n";
        let pr_json = serde_json::json!({
            "number": 7,
            "title": "Add PR readiness",
            "body": pr_body,
            "headRefOid": "abc123",
            "headRefName": "task-20",
            "baseRefName": "master",
            "files": [{ "path": "codex-rs/core/src/pr_readiness.rs" }]
        })
        .to_string();
        let parent_body = "## Objective\n\nCoordinate work.\n\n## Child Issues\n\n- [ ] #20 Implement PR readiness validator\n\n## Evidence Required For Closure\n\nReconcile children.\n";
        let parent_json = serde_json::json!({
            "number": 1,
            "title": "Plan: Aegis Code",
            "state": "OPEN",
            "body": parent_body,
            "labels": [{ "name": "aegis-code:plan" }]
        })
        .to_string();
        let child_json = serde_json::json!({
            "number": 20,
            "title": "Task: Implement PR readiness validator",
            "state": "OPEN",
            "body": task_issue_body(),
            "labels": [{ "name": "aegis-code:task" }]
        })
        .to_string();
        let runner = FakeGhRunner::new()
            .with_success(
                &["repo", "view", "--json", "nameWithOwner"],
                r#"{"nameWithOwner":"owner/repo"}"#,
            )
            .with_success(
                &["pr", "view", "--repo", "owner/repo", "--json", "number"],
                r#"{"number":7}"#,
            )
            .with_success(
                &[
                    "pr",
                    "view",
                    "7",
                    "--repo",
                    "owner/repo",
                    "--json",
                    "number,title,body,headRefOid,headRefName,baseRefName,files",
                ],
                &pr_json,
            )
            .with_success(
                &[
                    "issue",
                    "view",
                    "20",
                    "--repo",
                    "owner/repo",
                    "--json",
                    "number,title,state,body,labels",
                ],
                &child_json,
            )
            .with_success(
                &[
                    "issue",
                    "list",
                    "--repo",
                    "owner/repo",
                    "--state",
                    "open",
                    "--label",
                    "aegis-code:plan",
                    "--json",
                    "number",
                    "--limit",
                    "50",
                ],
                r#"[{"number":1}]"#,
            )
            .with_success(
                &[
                    "issue",
                    "view",
                    "1",
                    "--repo",
                    "owner/repo",
                    "--json",
                    "number,title,state,body,labels",
                ],
                &parent_json,
            );
        let cmd = PrReadinessValidateCommand {
            repo: None,
            pr: None,
            method_state: method_state_path,
            allowed_paths: Vec::new(),
            json: true,
        };

        let report = validate_pr_readiness_from_github(&cmd, &runner)
            .expect("fake gh should provide a valid PR readiness snapshot");

        assert!(report.valid, "{report:#?}");
        assert_eq!(report.pr_number, 7);
        assert_eq!(report.linked_issue_number, Some(20));
        assert_eq!(report.changed_file_count, 1);
    }

    #[test]
    fn pr_readiness_report_json_contains_expected_fields() {
        let report = PrReadinessReport {
            valid: false,
            pr_number: 7,
            linked_issue_number: Some(20),
            changed_file_count: 1,
            findings: vec![codex_core::pr_readiness::PrReadinessFinding {
                severity: FindingSeverity::Error,
                code: "missing_method_state".to_string(),
                subject: Some("pr:7".to_string()),
                message: "missing".to_string(),
                remediation: "provide method state".to_string(),
            }],
        };

        let value = serde_json::to_value(&report).expect("serialize report");

        assert_eq!(value["valid"], false);
        assert_eq!(value["pr_number"], 7);
        assert_eq!(value["linked_issue_number"], 20);
        assert_eq!(value["changed_file_count"], 1);
        assert_eq!(value["findings"][0]["code"], "missing_method_state");
    }

    #[test]
    fn context_pack_promote_parses_evidence_and_actor() {
        let cli = MultitoolCli::try_parse_from([
            "aegis",
            "context-pack",
            "promote",
            "learned:candidate",
            "--evidence",
            "issue:13",
            "--actor",
            "Tester",
            "--reason",
            "reviewed",
        ])
        .expect("parse should succeed");

        let Some(Subcommand::ContextPack(ContextPackCommand { subcommand })) = cli.subcommand
        else {
            panic!("expected context-pack subcommand");
        };
        let ContextPackSubcommand::Promote(cmd) = subcommand else {
            panic!("expected promote subcommand");
        };
        assert_eq!(cmd.selector, "learned:candidate");
        assert_eq!(cmd.evidence, vec!["issue:13"]);
        assert_eq!(cmd.actor.as_deref(), Some("Tester"));
        assert_eq!(cmd.reason.as_deref(), Some("reviewed"));
    }

    #[test]
    fn context_pack_promote_requires_evidence() {
        let parse_result =
            MultitoolCli::try_parse_from(["aegis", "context-pack", "promote", "learned:candidate"]);

        assert!(parse_result.is_err());
    }

    #[test]
    fn feature_toggles_known_features_generate_overrides() {
        let toggles = FeatureToggles {
            enable: vec!["web_search_request".to_string()],
            disable: vec!["unified_exec".to_string()],
        };
        let overrides = toggles.to_overrides().expect("valid features");
        assert_eq!(
            overrides,
            vec![
                "features.web_search_request=true".to_string(),
                "features.unified_exec=false".to_string(),
            ]
        );
    }

    #[test]
    fn feature_toggles_accept_legacy_linux_sandbox_flag() {
        let toggles = FeatureToggles {
            enable: vec!["use_linux_sandbox_bwrap".to_string()],
            disable: Vec::new(),
        };
        let overrides = toggles.to_overrides().expect("valid features");
        assert_eq!(
            overrides,
            vec!["features.use_linux_sandbox_bwrap=true".to_string(),]
        );
    }

    #[test]
    fn feature_toggles_accept_removed_image_detail_original_flag() {
        let toggles = FeatureToggles {
            enable: vec!["image_detail_original".to_string()],
            disable: Vec::new(),
        };
        let overrides = toggles.to_overrides().expect("valid features");
        assert_eq!(
            overrides,
            vec!["features.image_detail_original=true".to_string(),]
        );
    }

    #[test]
    fn feature_toggles_unknown_feature_errors() {
        let toggles = FeatureToggles {
            enable: vec!["does_not_exist".to_string()],
            disable: Vec::new(),
        };
        let err = toggles
            .to_overrides()
            .expect_err("feature should be rejected");
        assert_eq!(err.to_string(), "Unknown feature flag: does_not_exist");
    }

    fn task_issue_body() -> &'static str {
        "## Objective\n\nShip PR readiness.\n\n## Scope\n\nValidate PR readiness.\n\n## Acceptance Criteria\n\n- Valid PRs pass.\n\n## Falsifiers\n\n- Invalid PRs pass.\n\n## Dependencies\n\nNone\n"
    }

    fn pr_method_state_json(commit: &str) -> String {
        serde_json::json!({
            "schema_version": 1,
            "intent": {
                "summary": "Ship PR readiness validator",
                "success_criteria": ["PR readiness is validated"]
            },
            "linked_issue": {
                "provider": "git_hub",
                "repository": "owner/repo",
                "number": 20,
                "title": "Task: Implement PR readiness validator"
            },
            "status": "closed",
            "claims": [],
            "assumptions": [],
            "falsifiers": [{
                "id": "falsifier:missing-linkage",
                "summary": "PR can pass with no linked task issue",
                "status": "disproved",
                "evidence_ids": ["evidence:test"]
            }],
            "evidence_requirements": [{
                "id": "requirement:test",
                "summary": "Tests pass",
                "required": true,
                "commands": ["cargo test -p codex-core pr_readiness"],
                "claim_ids": [],
                "falsifier_ids": ["falsifier:missing-linkage"]
            }],
            "evidence": [{
                "id": "evidence:test",
                "summary": "Tests passed",
                "kind": "test",
                "requirement_ids": ["requirement:test"],
                "claim_ids": [],
                "falsifier_ids": ["falsifier:missing-linkage"],
                "source": "test",
                "captured_at_unix_seconds": 1,
                "receipt": {
                    "schema_version": 1,
                    "command": ["cargo", "test"],
                    "cwd": "/repo",
                    "captured_at_unix_seconds": 1,
                    "git_state": {
                        "status": "captured",
                        "repository": "owner/repo",
                        "branch": "task-20",
                        "commit": commit,
                        "dirty": false
                    },
                    "exit_status": {
                        "exit_code": 0,
                        "timed_out": false
                    },
                    "output_summary": "ok",
                    "artifacts": [],
                    "session": {
                        "session_id": "session",
                        "thread_id": "thread",
                        "provider": "test",
                        "model": "test"
                    },
                    "redaction_status": "not_needed"
                }
            }],
            "gates": [],
            "review_findings": [{
                "id": "finding:review",
                "summary": "Review completed with no blocking findings",
                "severity": "info",
                "status": "addressed",
                "claim_ids": [],
                "evidence_ids": ["evidence:test"],
                "reviewed_at_unix_seconds": 1,
                "reviewer": "tester"
            }],
            "closure": {
                "closed_at_unix_seconds": 2,
                "summary": "Ready",
                "evidence_ids": ["evidence:test"],
                "review_finding_ids": ["finding:review"],
                "closed_by": "tester"
            },
            "resume_context": {
                "repository": "owner/repo",
                "branch": "task-20",
                "commit": commit,
                "schema_version": 1
            },
            "provenance": {
                "created_at_unix_seconds": 1,
                "updated_at_unix_seconds": 2,
                "source": "agent",
                "actor": "tester"
            }
        })
        .to_string()
    }

    struct FakeGhRunner {
        outputs: std::collections::BTreeMap<String, GhOutput>,
        calls: std::cell::RefCell<Vec<Vec<String>>>,
    }

    impl FakeGhRunner {
        fn new() -> Self {
            Self {
                outputs: std::collections::BTreeMap::new(),
                calls: std::cell::RefCell::new(Vec::new()),
            }
        }

        fn with_success(mut self, args: &[&str], stdout: &str) -> Self {
            self.outputs.insert(
                args_key(args.iter().copied()),
                GhOutput {
                    stdout: stdout.to_string(),
                    stderr: String::new(),
                    success: true,
                },
            );
            self
        }
    }

    impl GhRunner for FakeGhRunner {
        fn run(&self, args: &[String]) -> anyhow::Result<GhOutput> {
            self.calls.borrow_mut().push(args.to_vec());
            let key = args_key(args.iter().map(String::as_str));
            self.outputs
                .get(&key)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("unexpected gh args: {args:?}"))
        }
    }

    fn args_key<'a>(args: impl IntoIterator<Item = &'a str>) -> String {
        args.into_iter().collect::<Vec<_>>().join("\u{0}")
    }
}
