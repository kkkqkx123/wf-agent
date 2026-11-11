use super::*;

fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
    Cli::try_parse_from(std::iter::once("wf").chain(args.iter().copied()))
}

#[test]
fn parses_run_subcommand_with_prompt() {
    let cli = parse(&["run", "hello world"]).unwrap();
    let Some(Command::Run {
        prompt,
        agent,
        model,
        approve_prefixes,
        ..
    }) = cli.command
    else {
        panic!("expected run command");
    };
    assert_eq!(prompt.as_deref(), Some("hello world"));
    assert!(agent.is_none());
    assert!(model.is_none());
    assert!(approve_prefixes.is_empty());
}

#[test]
fn parses_run_model_and_repeatable_approve_prefixes() {
    let cli = parse(&[
        "run",
        "hi",
        "--model",
        "mock",
        "--approve-prefix",
        "git",
        "--approve-prefix",
        "cargo ",
    ])
    .unwrap();
    let Some(Command::Run {
        model,
        approve_prefixes,
        ..
    }) = cli.command
    else {
        panic!("expected run command");
    };
    assert_eq!(model.as_deref(), Some("mock"));
    assert_eq!(
        approve_prefixes,
        vec!["git".to_string(), "cargo ".to_string()]
    );
}

#[test]
fn parses_interactive_flags() {
    let cli = parse(&["--tui"]).unwrap();
    assert!(cli.tui);
}

#[test]
fn output_format_flag_parses_all_variants() {
    for (flag, expected) in [
        ("text", OutputFormat::Text),
        ("json", OutputFormat::Json),
        ("jsonl", OutputFormat::JsonLines),
        ("silent", OutputFormat::Silent),
    ] {
        let cli = parse(&["run", "-o", flag]).unwrap();
        assert_eq!(cli.output, expected, "flag {flag}");
        let cli = parse(&["run", "--output", flag]).unwrap();
        assert_eq!(cli.output, expected, "long flag {flag}");
    }
}

#[test]
fn global_flags_are_visible_before_subcommand() {
    let cli = parse(&["--log", "out.log", "--no-color", "run", "x"]).unwrap();
    assert_eq!(
        cli.log.as_deref().map(|p| p.to_string_lossy().to_string()),
        Some("out.log".into())
    );
    assert!(cli.no_color);
}

#[test]
fn rejects_subcommand_with_interactive_flag() {
    let cli = parse(&["--tui", "run", "x"]).unwrap();
    assert!(cli.validate().is_err());
}

#[test]
fn parses_tui_session_options() {
    let cli = parse(&["--tui", "--agent", "ag", "--model", "m"]).unwrap();
    assert!(cli.tui);
    assert_eq!(cli.agent.as_deref(), Some("ag"));
    assert_eq!(cli.model.as_deref(), Some("m"));

    let cli = parse(&["--tui", "--session", "abc"]).unwrap();
    assert_eq!(cli.session.as_deref(), Some("abc"));

    let cli = parse(&["--tui", "--resume"]).unwrap();
    assert!(cli.resume);
}

#[test]
fn rejects_session_with_subcommand() {
    // The top-level options appear before the subcommand position.
    let cli = parse(&["--session", "abc", "run", "x"]).unwrap();
    assert!(cli.validate().is_err());
    let cli = parse(&["--resume", "run", "x"]).unwrap();
    assert!(cli.validate().is_err());
}

#[test]
fn rejects_session_and_resume_together() {
    let cli = parse(&["--tui", "--session", "abc", "--resume"]).unwrap();
    assert!(cli.validate().is_err());
}

#[test]
fn history_file_and_no_history_are_exclusive() {
    let cli = parse(&["--history-file", "/tmp/h", "--no-history"]).unwrap();
    let err = cli.validate().unwrap_err();
    assert!(err.contains("mutually exclusive"), "{err}");

    let cli = parse(&["--history-file", "/tmp/h"]).unwrap();
    assert!(cli.validate().is_ok());

    let cli = parse(&["--no-history"]).unwrap();
    assert!(cli.validate().is_ok());
}

#[test]
fn mini_display_flags_parse() {
    let cli = parse(&[
        "--markdown",
        "plain",
        "--no-reasoning",
        "--no-usage",
        "--quiet-tools",
    ])
    .unwrap();
    assert_eq!(cli.markdown, Some(MarkdownMode::Plain));
    assert!(cli.no_reasoning);
    assert!(cli.no_usage);
    assert!(cli.quiet_tools);
    assert!(cli.validate().is_ok());

    let cli = parse(&["--markdown", "styled"]).unwrap();
    assert_eq!(cli.markdown, Some(MarkdownMode::Styled));

    let cli = parse(&[]).unwrap();
    assert!(cli.markdown.is_none());
    assert!(!cli.no_reasoning);
}

#[test]
fn rejects_interactive_options_with_no_tui() {
    let cli = parse(&["--no-tui", "--resume"]).unwrap();
    assert!(cli.validate().is_err());
    let cli = parse(&["--no-tui", "--session", "abc"]).unwrap();
    assert!(cli.validate().is_err());
}

#[test]
fn run_subcommand_keeps_its_own_agent_model() {
    let cli = parse(&["run", "x", "--agent", "ag", "--model", "m"]).unwrap();
    assert!(cli.validate().is_ok());
    let Some(Command::Run { agent, model, .. }) = cli.command else {
        panic!("expected run command");
    };
    assert_eq!(agent.as_deref(), Some("ag"));
    assert_eq!(model.as_deref(), Some("m"));
    // Top-level interactive options stay untouched.
    assert!(cli.agent.is_none());
    assert!(cli.model.is_none());
}

#[test]
fn session_flags_validate_without_local_storage() {
    // Sessions live server-side; no local storage flag is required.
    let cli = parse(&["--tui", "--session", "abc"]).unwrap();
    assert!(cli.validate().is_ok());
    let cli = parse(&["--tui", "--resume"]).unwrap();
    assert!(cli.validate().is_ok());
}

#[test]
fn timeout_flag_parses_and_validates() {
    let cli = parse(&["--timeout", "5000"]).unwrap();
    assert_eq!(cli.timeout, Some(5000));
    assert!(cli.validate().is_ok());
    let cli = parse(&["--timeout", "0"]).unwrap();
    assert!(cli.validate().is_err());
}

#[test]
fn workflow_flags_parse_and_validate() {
    let cli = parse(&["run", "--workflow", "wf-1"]).unwrap();
    let Some(Command::Run {
        workflow, input, ..
    }) = &cli.command
    else {
        panic!("expected run");
    };
    assert_eq!(workflow.as_deref(), Some("wf-1"));
    assert!(input.is_none());
    assert!(cli.validate().is_ok());

    let cli = parse(&["run", "--workflow", "wf-1", "--input", r#"{"a":1}"#]).unwrap();
    assert!(cli.validate().is_ok());

    let cli = parse(&["run", "--input", r#"{"a":1}"#]).unwrap();
    assert!(cli.validate().is_err());

    let cli = parse(&["run", "prompt", "--workflow", "wf-1"]).unwrap();
    assert!(cli.validate().is_err());

    let cli = parse(&["run", "--workflow", "wf-1", "--input", "bad-json"]).unwrap();
    assert!(cli.validate().is_err());

    let cli = parse(&["run", "--workflow", "wf-1", "--input", "[1,2]"]).unwrap();
    assert!(cli.validate().is_err());
}

#[test]
fn image_flags_parse_and_reject_workflow() {
    let cli = parse(&["run", "hi", "--image", "a.png", "--image", "b.jpg"]).unwrap();
    let Some(Command::Run { image, .. }) = &cli.command else {
        panic!("expected run");
    };
    assert_eq!(image.len(), 2);
    assert!(cli.validate().is_ok());

    let cli = parse(&[
        "run",
        "hi",
        "--image-url",
        "https://example.com/shot.png",
        "--image-detail",
        "high",
    ])
    .unwrap();
    assert!(cli.validate().is_ok());

    let cli = parse(&["run", "hi", "--image-detail", "nope"]).unwrap();
    assert!(cli.validate().is_err());

    let cli = parse(&["run", "hi", "--image-url", "ftp://x/y.png"]).unwrap();
    assert!(cli.validate().is_err());

    let cli = parse(&["run", "--workflow", "wf-1", "--image", "a.png"]).unwrap();
    let err = cli.validate().unwrap_err();
    assert!(err.contains("--image"), "{err}");

    let cli = parse(&[
        "run",
        "--workflow",
        "wf-1",
        "--image-url",
        "https://example.com/shot.png",
    ])
    .unwrap();
    let err = cli.validate().unwrap_err();
    assert!(err.contains("--image"), "{err}");

    let cli = parse(&["run", "hi", "--attach", "a.txt"]).unwrap();
    let Some(Command::Run { attach, .. }) = &cli.command else {
        panic!("expected run");
    };
    assert_eq!(attach.len(), 1);
    assert!(cli.validate().is_ok());

    let cli = parse(&["run", "--workflow", "wf-1", "--attach", "a.txt"]).unwrap();
    let err = cli.validate().unwrap_err();
    assert!(err.contains("--attach"), "{err}");
}

#[test]
fn interactive_run_flags_parse() {
    let cli = parse(&["run", "hi", "--interactive"]).unwrap();
    let Some(Command::Run {
        interactive,
        approval_timeout,
        assume_yes,
        ..
    }) = &cli.command
    else {
        panic!("expected run");
    };
    assert!(*interactive);
    assert_eq!(*approval_timeout, None);
    assert!(!assume_yes);
    assert!(cli.validate().is_ok());

    let cli = parse(&["run", "hi", "--interactive", "--approval-timeout", "30"]).unwrap();
    assert!(cli.validate().is_ok());

    let cli = parse(&["run", "hi", "-y"]).unwrap();
    let Some(Command::Run { assume_yes, .. }) = &cli.command else {
        panic!("expected run");
    };
    assert!(*assume_yes);
    assert!(cli.validate().is_ok());
}

#[test]
fn interactive_and_assume_yes_are_exclusive() {
    let cli = parse(&["run", "hi", "--interactive", "--assume-yes"]).unwrap();
    let err = cli.validate().unwrap_err();
    assert!(err.contains("mutually exclusive"), "{err}");
}

#[test]
fn approval_timeout_needs_interactive_and_positive() {
    let cli = parse(&["run", "hi", "--approval-timeout", "30"]).unwrap();
    let err = cli.validate().unwrap_err();
    assert!(err.contains("--interactive"), "{err}");

    let cli = parse(&["run", "hi", "--interactive", "--approval-timeout", "0"]).unwrap();
    let err = cli.validate().unwrap_err();
    assert!(err.contains("greater than 0"), "{err}");
}

#[test]
fn directory_flag_and_positional_parse() {
    let cli = parse(&["--directory", "/tmp"]).unwrap();
    assert_eq!(
        cli.directory
            .as_deref()
            .map(|p| p.to_string_lossy().to_string()),
        Some("/tmp".into())
    );
    assert!(cli.directory_arg.is_none());
    assert!(cli.validate().is_ok());

    let cli = parse(&["/tmp"]).unwrap();
    assert!(cli.directory.is_none());
    assert!(cli.directory_arg.is_some());
    assert!(cli.validate().is_ok());

    let cli = parse(&["--directory", "/a", "/b"]).unwrap();
    let err = cli.validate().unwrap_err();
    assert!(err.contains("mutually exclusive"), "{err}");

    let mut cli = parse(&["run", "x"]).unwrap();
    cli.directory_arg = Some(std::path::PathBuf::from("/tmp"));
    let err = cli.validate().unwrap_err();
    assert!(err.contains("--directory with subcommands"), "{err}");

    let cli = parse(&["run", "--directory", "/tmp", "x"]).unwrap();
    assert!(cli.validate().is_ok());
}

#[test]
fn worktree_flag_parses_and_rejects_workflow() {
    let cli = parse(&["run", "hi", "--worktree"]).unwrap();
    let Some(Command::Run { worktree, .. }) = &cli.command else {
        panic!("expected run");
    };
    assert_eq!(worktree.as_deref(), Some(""));
    assert!(cli.validate().is_ok());

    let cli = parse(&["run", "hi", "--worktree", "feat"]).unwrap();
    assert!(cli.validate().is_ok());

    let cli = parse(&["run", "hi", "--worktree", "a/b"]).unwrap();
    let err = cli.validate().unwrap_err();
    assert!(err.contains("plain directory name"), "{err}");

    let cli = parse(&["run", "--workflow", "wf-1", "--worktree"]).unwrap();
    let err = cli.validate().unwrap_err();
    assert!(err.contains("--worktree"), "{err}");
}

#[test]
fn directories_command_parses() {
    let cli = parse(&["directories"]).unwrap();
    assert!(matches!(cli.command, Some(Command::Directories)));
    assert!(cli.validate().is_ok());
}

#[test]
fn config_export_import_parse() {
    let cli = parse(&["config", "export", "/tmp/backup.toml"]).unwrap();
    let Some(Command::Config { sub }) = &cli.command else {
        panic!("expected config");
    };
    assert!(matches!(sub, ConfigSub::Export { .. }));
    assert!(cli.validate().is_ok());

    let cli = parse(&["config", "import", "/tmp/backup.toml"]).unwrap();
    let Some(Command::Config { sub }) = &cli.command else {
        panic!("expected config");
    };
    assert!(matches!(sub, ConfigSub::Import { .. }));
    assert!(cli.validate().is_ok());
}
