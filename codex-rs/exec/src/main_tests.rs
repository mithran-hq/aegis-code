use super::*;
use pretty_assertions::assert_eq;

#[test]
fn top_cli_parses_resume_prompt_after_config_flag() {
    const PROMPT: &str = "echo resume-with-global-flags-after-subcommand";
    let cli = TopCli::parse_from([
        "codex-exec",
        "resume",
        "--last",
        "--json",
        "--model",
        "gpt-5.2-codex",
        "--config",
        "reasoning_level=xhigh",
        "--dangerously-bypass-approvals-and-sandbox",
        "--skip-git-repo-check",
        PROMPT,
    ]);

    let Some(codex_exec::Command::Resume(args)) = cli.inner.command else {
        panic!("expected resume command");
    };
    let effective_prompt = args.prompt.clone().or_else(|| {
        if args.last {
            args.session_id.clone()
        } else {
            None
        }
    });
    assert_eq!(effective_prompt.as_deref(), Some(PROMPT));
    assert_eq!(cli.config_overrides.raw_overrides.len(), 1);
    assert_eq!(
        cli.config_overrides.raw_overrides[0],
        "reasoning_level=xhigh"
    );
}

#[test]
fn top_cli_parses_method_state_flags_for_exec() {
    let cli = TopCli::parse_from([
        "codex-exec",
        "--method-state",
        "/tmp/method-state-in.json",
        "--method-state-output",
        "/tmp/method-state-out.json",
        "run the task",
    ]);

    assert_eq!(
        cli.inner.method_state.as_deref(),
        Some(std::path::Path::new("/tmp/method-state-in.json"))
    );
    assert_eq!(
        cli.inner.method_state_output.as_deref(),
        Some(std::path::Path::new("/tmp/method-state-out.json"))
    );
}
