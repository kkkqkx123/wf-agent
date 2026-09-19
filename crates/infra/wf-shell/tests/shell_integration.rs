use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use wf_shell::command_safety::{
    contains_dangerous_substitution, get_command_decision, CommandDecision,
};
use wf_shell::config::ShellToolConfig;
use wf_shell::engine::{BackgroundShellStore, SessionCreateOptions, SpawnOptions};
use wf_shell::event_sink::ShellEventSink;
use wf_shell::lifecycle::{SessionLifecycleEvent, SessionLifecycleKind, SessionLifecycleSink};
use wf_shell::shell_detector::ShellType;

fn poll_until(timeout: Duration, mut cond: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if cond() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    cond()
}

fn session_output(store: &BackgroundShellStore, id: &str) -> String {
    store
        .get(id)
        .map(|s| {
            s.snapshot()
                .get("output")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string()
        })
        .unwrap_or_default()
}

fn wait_for_output(store: &BackgroundShellStore, id: &str, needle: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut last = String::new();
    while Instant::now() < deadline {
        last = session_output(store, id);
        if last.contains(needle) {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    last
}

#[tokio::test]
async fn stateless_runner_respects_cwd_and_reports_exit_code() {
    let dir = std::env::temp_dir().join("wf-shell-it-cwd");
    std::fs::create_dir_all(&dir).unwrap();
    let cwd = dir.to_string_lossy().to_string();

    let out = wf_shell::runner::run_command("pwd", Some(&cwd), 10_000, None, None, None)
        .await
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.trim().ends_with("wf-shell-it-cwd"),
        "pwd output: {stdout}"
    );

    let out = wf_shell::runner::run_command("exit 3", None, 10_000, None, None, None)
        .await
        .unwrap();
    assert_eq!(out.status.code(), Some(3));

    let out = wf_shell::runner::run_command("echo err-line >&2", None, 10_000, None, None, None)
        .await
        .unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("err-line"));

    let err = wf_shell::runner::run_command("sleep 10", None, 300, None, None, None)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("timed out"), "error: {err}");
}

#[tokio::test]
async fn stateless_runner_stdin_roundtrip_and_shell_override() {
    let out = wf_shell::runner::run_command("cat", None, 10_000, None, Some("stdin-probe"), None)
        .await
        .unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).contains("stdin-probe"));

    let out = wf_shell::runner::run_command(
        "echo override-ok",
        None,
        10_000,
        Some(ShellType::Sh),
        None,
        None,
    )
    .await
    .unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).contains("override-ok"));
}

#[test]
fn blocking_spawn_inherits_parent_env_and_overlays_session_env() {
    std::env::set_var("WF_SHELL_IT_PARENT", "parent-value");
    let output = wf_shell::spawn::run_shell_blocking(
        None,
        "echo $WF_SHELL_IT_PARENT",
        None,
        None,
        None,
        Duration::from_secs(10),
        None,
    )
    .unwrap();
    assert!(String::from_utf8_lossy(&output.stdout).contains("parent-value"));

    let mut overlay = HashMap::new();
    overlay.insert("WF_SHELL_IT_PARENT".to_string(), "overlay-wins".to_string());
    let output = wf_shell::spawn::run_shell_blocking(
        None,
        "echo $WF_SHELL_IT_PARENT",
        None,
        Some(&overlay),
        None,
        Duration::from_secs(10),
        None,
    )
    .unwrap();
    assert!(String::from_utf8_lossy(&output.stdout).contains("overlay-wins"));
    std::env::remove_var("WF_SHELL_IT_PARENT");
}

#[test]
fn store_default_env_flows_into_session_commands_and_path_can_be_replaced() {
    let mut default_env = HashMap::new();
    default_env.insert("WF_STORE_DEFAULT".to_string(), "default-ok".to_string());
    let config = ShellToolConfig {
        default_env,
        ..Default::default()
    };
    let store = BackgroundShellStore::from_config(&config);
    let created = store
        .get_or_create(&SessionCreateOptions::default(), Some("env-task"))
        .unwrap();
    let first = store
        .execute_in_session(&created.session_id, "echo $WF_STORE_DEFAULT", Some(10_000))
        .unwrap();
    assert!(first["output"].as_str().unwrap().contains("default-ok"));

    let created2 = store
        .get_or_create(
            &SessionCreateOptions {
                env: HashMap::from([("WF_STORE_DEFAULT".to_string(), "session-wins".to_string())]),
                ..Default::default()
            },
            Some("env-task-2"),
        )
        .unwrap();
    let second = store
        .execute_in_session(&created2.session_id, "echo $WF_STORE_DEFAULT", Some(10_000))
        .unwrap();
    assert!(second["output"].as_str().unwrap().contains("session-wins"));

    let created3 = store
        .get_or_create(
            &SessionCreateOptions {
                env: HashMap::from([("PATH".to_string(), "/nonexistent-path".to_string())]),
                ..Default::default()
            },
            Some("env-task-3"),
        )
        .unwrap();
    let third = store
        .execute_in_session(&created3.session_id, "echo $PATH", Some(10_000))
        .unwrap();
    assert!(
        third["output"]
            .as_str()
            .unwrap()
            .contains("/nonexistent-path"),
        "PATH overlay must replace inherited PATH"
    );
    store.clear();
}

#[test]
fn store_cwd_resolution_prefers_session_cwd_over_default() {
    let dir = std::env::temp_dir().join("wf-shell-it-default-cwd");
    std::fs::create_dir_all(&dir).unwrap();
    let store = BackgroundShellStore::new(Some(dir.clone()));
    let created = store
        .get_or_create(&SessionCreateOptions::default(), None)
        .unwrap();
    assert_eq!(created.cwd, Some(dir.clone()));

    let other = std::env::temp_dir().join("wf-shell-it-session-cwd");
    std::fs::create_dir_all(&other).unwrap();
    let created2 = store
        .get_or_create(
            &SessionCreateOptions {
                cwd: Some(other.to_string_lossy().to_string()),
                ..Default::default()
            },
            None,
        )
        .unwrap();
    assert_eq!(created2.cwd, Some(other.clone()));

    let result = store
        .execute_in_session(&created2.session_id, "pwd", Some(10_000))
        .unwrap();
    assert!(
        result["output"]
            .as_str()
            .unwrap()
            .contains(&other.to_string_lossy().to_string()),
        "output: {}",
        result["output"]
    );
    store.clear();
}

#[test]
fn execute_in_session_reports_nonzero_exit_and_captures_stderr() {
    let store = BackgroundShellStore::new(None);
    let created = store
        .get_or_create(&SessionCreateOptions::default(), Some("exit-task"))
        .unwrap();
    let result = store
        .execute_in_session(
            &created.session_id,
            "echo out-line; echo err-line >&2; exit 3",
            Some(10_000),
        )
        .unwrap();
    assert_eq!(result["success"], serde_json::json!(false));
    assert_eq!(result["exit_code"], serde_json::json!(3));
    let output = result["output"].as_str().unwrap();
    assert!(output.contains("out-line"), "output: {output}");
    assert!(output.contains("err-line"), "output: {output}");
    let session = store.get(&created.session_id).unwrap();
    assert_eq!(session.last_exit_code(), Some(3));
    assert_eq!(session.status_str(), "idle");
    let _ = store.kill(&created.session_id);
}

#[test]
fn execute_in_session_timeout_terminates_command_and_goes_idle() {
    let store = BackgroundShellStore::new(None);
    let created = store
        .get_or_create(&SessionCreateOptions::default(), Some("timeout-task"))
        .unwrap();
    let start = Instant::now();
    let result = store
        .execute_in_session(&created.session_id, "sleep 30", Some(1000))
        .unwrap();
    assert_eq!(result["timed_out"], serde_json::json!(true));
    assert_eq!(result["success"], serde_json::json!(false));
    assert!(
        start.elapsed() < Duration::from_secs(10),
        "timeout path hung: {:?}",
        start.elapsed()
    );
    assert_eq!(store.get(&created.session_id).unwrap().status_str(), "idle");
    let _ = store.kill(&created.session_id);
}

#[test]
fn send_input_pipe_roundtrip_and_idle_or_missing_errors() {
    let store = BackgroundShellStore::new(None);
    let id = store
        .spawn_with_options(SpawnOptions {
            command: "read x; echo \"got:$x\"".to_string(),
            ..Default::default()
        })
        .unwrap();
    assert!(poll_until(Duration::from_secs(5), || {
        store.get(&id).is_some_and(|s| s.status_str() == "busy")
    }));
    store.send_input(&id, "hello-pipe", true).unwrap();
    let output = wait_for_output(&store, &id, "got:hello-pipe");
    assert!(output.contains("got:hello-pipe"), "output: {output}");
    assert!(poll_until(Duration::from_secs(8), || {
        store.get(&id).is_some_and(|s| s.status_str() == "idle")
    }));
    assert_eq!(store.get(&id).unwrap().last_exit_code(), Some(0));

    let err = store.send_input(&id, "late", true).unwrap_err();
    assert!(
        err.to_string().contains("idle") || err.to_string().contains("exited"),
        "error: {err}"
    );
    let err = store.send_input("missing-session", "hi", true).unwrap_err();
    assert!(err.to_string().contains("missing-session"), "error: {err}");
    let _ = store.kill(&id);
}

#[test]
fn resize_pipe_session_errors_while_pty_resize_succeeds() {
    let store = BackgroundShellStore::new(None);
    let pipe_id = store.spawn("sleep 5", None).unwrap();
    let err = store.resize(&pipe_id, 10, 20).unwrap_err();
    assert!(err.to_string().contains("PTY"), "error: {err}");
    let _ = store.kill(&pipe_id);

    let pty_store = BackgroundShellStore::new(None);
    let pty_id = pty_store
        .spawn_with_options(SpawnOptions {
            command: "sleep 5".to_string(),
            interactive: true,
            pty_size: (12, 34),
            ..Default::default()
        })
        .unwrap();
    let session = pty_store.get(&pty_id).unwrap();
    assert_eq!(session.mode_str(), "pty");
    pty_store.resize(&pty_id, 20, 45).unwrap();
    let _ = pty_store.kill(&pty_id);

    let disabled = BackgroundShellStore::from_config(&ShellToolConfig {
        pty_enabled: false,
        ..Default::default()
    });
    let fallback_id = disabled
        .spawn_with_options(SpawnOptions {
            command: "echo hi".to_string(),
            interactive: true,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(disabled.get(&fallback_id).unwrap().mode_str(), "pipe");
    let _ = disabled.kill(&fallback_id);
}

#[test]
fn pty_interactive_prompt_roundtrip_has_no_carriage_return() {
    let store = BackgroundShellStore::new(None);
    let id = store
        .spawn_with_options(SpawnOptions {
            command: "printf 'name: '; read n; echo \"hi $n\"".to_string(),
            interactive: true,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(store.get(&id).unwrap().mode_str(), "pty");
    let prompt = wait_for_output(&store, &id, "name:");
    assert!(prompt.contains("name:"), "prompt: {prompt:?}");
    store.send_input(&id, "world", true).unwrap();
    let output = wait_for_output(&store, &id, "hi world");
    assert!(output.contains("hi world"), "output: {output:?}");
    assert!(!output.contains('\r'), "CR must be normalized: {output:?}");
    let _ = store.kill(&id);
}

#[test]
fn output_window_pages_and_snapshot_reports_status() {
    let store = BackgroundShellStore::new(None);
    let created = store
        .get_or_create(&SessionCreateOptions::default(), Some("window-task"))
        .unwrap();
    store
        .execute_in_session(&created.session_id, "printf '0123456789'", Some(10_000))
        .unwrap();
    let session = store.get(&created.session_id).unwrap();

    let first = session.output_window(0, 4);
    assert_eq!(first["output"], serde_json::json!("0123"));
    assert_eq!(first["window_end"], serde_json::json!(4));
    assert_eq!(first["status"], serde_json::json!("idle"));
    assert_eq!(first["exit_code"], serde_json::json!(0));
    let start = first["window_end"].as_u64().unwrap() as usize;
    let second = session.output_window(start, 100);
    assert_eq!(second["output"], serde_json::json!("456789"));

    let snapshot = session.snapshot();
    assert_eq!(snapshot["status"], serde_json::json!("idle"));
    assert_eq!(snapshot["exit_code"], serde_json::json!(0));
    assert_eq!(snapshot["mode"], serde_json::json!("pipe"));
    assert!(snapshot["output"].as_str().unwrap().contains("0123456789"));
    assert!(session.pid().is_some());

    session.read_new_output();
    assert_eq!(session.read_new_output(), "");
    assert_eq!(session.peek_new_output(), "");
    let _ = store.kill(&created.session_id);
}

#[derive(Default)]
struct RecordingLifecycle {
    events: Mutex<Vec<SessionLifecycleEvent>>,
}

impl SessionLifecycleSink for RecordingLifecycle {
    fn on_lifecycle(&self, event: &SessionLifecycleEvent) {
        self.events.lock().unwrap().push(event.clone());
    }
}

impl RecordingLifecycle {
    fn snapshot(&self) -> Vec<SessionLifecycleEvent> {
        self.events.lock().unwrap().clone()
    }
}

#[test]
fn lifecycle_sink_observes_start_complete_and_terminate() {
    let sink: Arc<RecordingLifecycle> = Arc::new(RecordingLifecycle::default());
    let config = ShellToolConfig {
        lifecycle_sink: Some(sink.clone()),
        ..Default::default()
    };
    let store = BackgroundShellStore::from_config(&config);
    let created = store
        .get_or_create(&SessionCreateOptions::default(), Some("life-task"))
        .unwrap();
    assert!(!created.reused);
    store
        .execute_in_session(&created.session_id, "echo life-ok", Some(10_000))
        .unwrap();
    store.kill(&created.session_id).unwrap();

    let events = sink.snapshot();
    let started = events
        .iter()
        .find(|e| {
            e.session_id == created.session_id
                && matches!(e.kind, SessionLifecycleKind::Started { reused: false })
        })
        .expect("started event");
    assert_eq!(started.task_id.as_deref(), Some("life-task"));
    let completed = events
        .iter()
        .find(|e| {
            e.session_id == created.session_id
                && matches!(
                    &e.kind,
                    SessionLifecycleKind::CommandCompleted {
                        command,
                        exit_code: Some(0),
                        success: true
                    } if command == "echo life-ok"
                )
        })
        .expect("completed event");
    assert_eq!(completed.task_id.as_deref(), Some("life-task"));
    assert!(
        events
            .iter()
            .any(|e| e.session_id == created.session_id
                && e.kind == SessionLifecycleKind::Terminated),
        "events: {events:?}"
    );
}

#[test]
fn lifecycle_sink_registered_late_still_observes_session_end() {
    let mut store = BackgroundShellStore::new(None);
    let created = store
        .get_or_create(&SessionCreateOptions::default(), Some("late-task"))
        .unwrap();
    let sink: Arc<RecordingLifecycle> = Arc::new(RecordingLifecycle::default());
    store.set_lifecycle_sink(sink.clone());
    store
        .execute_in_session(&created.session_id, "echo late-ok", Some(10_000))
        .unwrap();
    let events = sink.snapshot();
    assert!(
        events.iter().any(|e| matches!(
            &e.kind,
            SessionLifecycleKind::CommandCompleted { command, .. } if command == "echo late-ok"
        )),
        "events: {events:?}"
    );
}

#[test]
fn lifecycle_reused_flag_set_on_session_reuse() {
    let sink: Arc<RecordingLifecycle> = Arc::new(RecordingLifecycle::default());
    let config = ShellToolConfig {
        lifecycle_sink: Some(sink.clone()),
        ..Default::default()
    };
    let store = BackgroundShellStore::from_config(&config);
    let opts = SessionCreateOptions {
        cwd: Some("/tmp/wf-shell-it-reuse".to_string()),
        ..Default::default()
    };
    let first = store.get_or_create(&opts, Some("reuse-life")).unwrap();
    let second = store.get_or_create(&opts, Some("reuse-life")).unwrap();
    assert!(second.reused);
    assert_eq!(first.session_id, second.session_id);
    let events = sink.snapshot();
    assert!(
        events
            .iter()
            .any(|e| matches!(e.kind, SessionLifecycleKind::Started { reused: true })),
        "events: {events:?}"
    );
    let _ = store.kill(&first.session_id);
}

#[derive(Default)]
struct MemEventSink {
    events: Mutex<Vec<String>>,
}

impl ShellEventSink for MemEventSink {
    fn on_output(&self, session_id: &str, _task_id: Option<&str>, line: &str) {
        self.events
            .lock()
            .unwrap()
            .push(format!("{session_id}:{line}"));
    }
    fn on_command_completed(
        &self,
        session_id: &str,
        _task_id: Option<&str>,
        _command: &str,
        _exit_code: Option<i32>,
        _success: bool,
    ) {
        self.events
            .lock()
            .unwrap()
            .push(format!("{session_id}:completed"));
    }
}

#[test]
fn event_sink_wired_through_config_delivers_output_and_completion() {
    let sink: Arc<MemEventSink> = Arc::new(MemEventSink::default());
    let config = ShellToolConfig {
        output_event_enabled: true,
        event_sink: Some(sink.clone()),
        ..Default::default()
    };
    let store = BackgroundShellStore::from_config(&config);
    let created = store
        .get_or_create(&SessionCreateOptions::default(), Some("evt-task"))
        .unwrap();
    store
        .execute_in_session(&created.session_id, "printf 'one\ntwo\n'", Some(10_000))
        .unwrap();
    assert!(poll_until(Duration::from_secs(8), || {
        let events = sink.events.lock().unwrap();
        events.iter().any(|e| e.ends_with(":one"))
            && events.iter().any(|e| e.ends_with(":two"))
            && events.iter().any(|e| e.ends_with(":completed"))
    }));
    let _ = store.kill(&created.session_id);
}

#[test]
fn store_task_binding_release_and_clear_behave_end_to_end() {
    let store = BackgroundShellStore::new(None);
    let a = store
        .get_or_create(
            &SessionCreateOptions {
                cwd: Some("/tmp/wf-shell-it-task-a".to_string()),
                ..Default::default()
            },
            Some("bulk-task"),
        )
        .unwrap();
    let b = store
        .get_or_create(
            &SessionCreateOptions {
                cwd: Some("/tmp/wf-shell-it-task-b".to_string()),
                ..Default::default()
            },
            Some("bulk-task"),
        )
        .unwrap();
    assert_eq!(store.session_count(), 2);
    assert_eq!(store.sessions_for_task("bulk-task").len(), 2);
    assert!(store.sessions_for_task("missing").is_empty());

    assert_eq!(store.release_sessions_for_task("bulk-task", false), 2);
    assert_eq!(store.session_count(), 2);
    assert!(store.sessions_for_task("bulk-task").is_empty());
    let reused = store
        .get_or_create(
            &SessionCreateOptions {
                cwd: Some("/tmp/wf-shell-it-task-a".to_string()),
                ..Default::default()
            },
            Some("other-task"),
        )
        .unwrap();
    assert!(reused.reused);
    assert_eq!(reused.session_id, a.session_id);

    assert_eq!(store.release_sessions_for_task("other-task", true), 1);
    assert!(store.get(&a.session_id).is_none());

    assert!(!store.kill("missing-session").unwrap());
    assert!(store.kill_with(&b.session_id, false).unwrap());
    store.clear();
    assert_eq!(store.session_count(), 0);
}

#[test]
fn sweep_idle_sessions_removes_only_unbound_idle_sessions() {
    let store = BackgroundShellStore::new(None);
    let unbound = store
        .get_or_create(&SessionCreateOptions::default(), None)
        .unwrap();
    assert_eq!(store.sweep_idle_sessions(0), 1);
    assert!(store.get(&unbound.session_id).is_none());

    let bound = store
        .get_or_create(&SessionCreateOptions::default(), Some("sweep-task"))
        .unwrap();
    assert_eq!(store.sweep_idle_sessions(0), 0);
    assert!(store.get(&bound.session_id).is_some());
    assert_eq!(store.release_sessions_for_task("sweep-task", false), 1);
    assert_eq!(store.sweep_idle_sessions(0), 1);
    assert!(store.get(&bound.session_id).is_none());
}

#[test]
fn idle_timeout_config_lazily_sweeps_on_get_or_create() {
    let config = ShellToolConfig {
        session_idle_timeout_ms: Some(0),
        ..Default::default()
    };
    let store = BackgroundShellStore::from_config(&config);
    let first = store
        .get_or_create(&SessionCreateOptions::default(), None)
        .unwrap();
    let second = store
        .get_or_create(&SessionCreateOptions::default(), None)
        .unwrap();
    assert_ne!(
        first.session_id, second.session_id,
        "idle session must have been swept before the second lookup"
    );
    assert_eq!(store.session_count(), 1);
    let _ = store.kill(&second.session_id);
}

#[test]
fn max_sessions_per_task_limit_is_enforced() {
    let config = ShellToolConfig {
        max_sessions_per_task: Some(1),
        ..Default::default()
    };
    let store = BackgroundShellStore::from_config(&config);
    store
        .get_or_create(&SessionCreateOptions::default(), Some("capped"))
        .unwrap();
    let err = store
        .get_or_create(&SessionCreateOptions::default(), Some("capped"))
        .unwrap_err();
    assert!(err.to_string().contains("Maximum sessions"), "error: {err}");
}

#[test]
fn policy_denied_command_rejected_without_session_leak() {
    let config = ShellToolConfig {
        denied_commands: Some(vec!["danger-it".to_string()]),
        ..Default::default()
    };
    let store = BackgroundShellStore::from_config(&config);
    let err = store.spawn("danger-it --all", None).unwrap_err();
    assert!(err.to_string().contains("rejected by shell policy"));
    assert_eq!(store.session_count(), 0);

    let created = store
        .get_or_create(&SessionCreateOptions::default(), Some("policy-task"))
        .unwrap();
    let err = store
        .execute_in_session(&created.session_id, "danger-it --all", None)
        .unwrap_err();
    assert!(err.to_string().contains("rejected by shell policy"));
    assert_eq!(store.get(&created.session_id).unwrap().status_str(), "idle");
    let _ = store.kill(&created.session_id);
}

#[test]
fn policy_ask_user_commands_still_execute_and_chain_helpers_agree() {
    assert!(contains_dangerous_substitution("echo ${USER@Q}"));
    assert!(!contains_dangerous_substitution("echo hello"));

    let allowed = vec!["git".to_string()];
    let denied = vec!["rm".to_string()];
    assert_eq!(
        get_command_decision("sudo rm -rf /", &allowed, Some(&denied)),
        CommandDecision::AutoDeny
    );
    assert_ne!(
        get_command_decision("git status", &allowed, None),
        CommandDecision::AutoDeny
    );

    let store = BackgroundShellStore::new(None);
    let created = store
        .get_or_create(&SessionCreateOptions::default(), Some("ask-task"))
        .unwrap();
    let result = store
        .execute_in_session(&created.session_id, "echo ask-proceeds", Some(10_000))
        .unwrap();
    assert_eq!(result["success"], serde_json::json!(true));
    let _ = store.kill(&created.session_id);
}

#[test]
fn shell_detector_resolution_and_config_defaults_hold() {
    let detector = wf_shell::shell_detector::ShellDetector::new();
    let (program, args) =
        wf_shell::shell_detector::resolve_shell_command(&detector, None, "echo hi");
    assert!(!program.is_empty());
    assert_eq!(args.len(), 2);
    assert_eq!(ShellType::from_name("bash"), Some(ShellType::Bash));
    assert_eq!(ShellType::from_name("unknown-shell"), None);

    let config = ShellToolConfig::default();
    assert!(config.pty_enabled);
    assert!(config.session_reuse_enabled);
    assert!(!config.output_event_enabled);
    assert!(config.allowed_commands.contains(&"git".to_string()));
}
