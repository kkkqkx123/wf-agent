use super::*;
use crate::event_sink::ShellEventSink;
use std::sync::Mutex as StdMutex;

/// Poll the sink until `predicate` holds or the deadline elapses. Events
/// are delivered on a background dispatch thread, so tests must wait for
/// them instead of reading synchronously.
fn wait_for_events(sink: &MemSink, predicate: impl Fn(&[String]) -> bool) -> Vec<String> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    loop {
        let events = sink.events.lock().unwrap().clone();
        if predicate(&events) || std::time::Instant::now() >= deadline {
            return events;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[test]
fn test_spawn_validation() {
    let store = BackgroundShellStore::new(None);
    assert!(store.spawn("", None).is_err());
}

#[test]
fn test_spawn_denied_command_rejected() {
    let config = crate::config::ShellToolConfig {
        denied_commands: Some(vec!["danger".into()]),
        ..Default::default()
    };
    let store = BackgroundShellStore::from_config(&config);
    let err = store.spawn("danger --all", None).unwrap_err();
    assert!(
        err.to_string().contains("rejected by shell policy"),
        "error: {}",
        err
    );
    // No empty idle session is left behind by the rejected spawn.
    assert_eq!(store.session_count(), 0);
}

#[tokio::test]
async fn test_execute_in_session_denied_command_rejected() {
    let config = crate::config::ShellToolConfig {
        denied_commands: Some(vec!["danger".into()]),
        ..Default::default()
    };
    let store = Arc::new(BackgroundShellStore::from_config(&config));
    let created = store
        .get_or_create(&SessionCreateOptions::default(), Some("t1"))
        .unwrap();
    let sid = &created.session_id;
    let err = store
        .execute_in_session(sid, "danger --all", None)
        .unwrap_err();
    assert!(
        err.to_string().contains("rejected by shell policy"),
        "error: {}",
        err
    );
    assert_eq!(
        store.get(sid).unwrap().status(),
        SessionStatus::Idle,
        "session stays idle after a rejected command"
    );
    let _ = store.kill(sid);
}

#[tokio::test]
async fn test_background_command_dispatches_completion_without_query() {
    let sink = Arc::new(MemSink::default());
    let mut store = BackgroundShellStore::new(None);
    store.output_event_enabled = true;
    store.event_sink = Some(EventDispatcher::new(sink.clone()));

    // Spawn a short background command; never query the session.
    let id = store.spawn("echo background-done", None).unwrap();

    // The completion event must arrive on its own (push-based), without
    // any shell_output / status query.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    let completed = loop {
        let events = sink.events.lock().unwrap().clone();
        if let Some(e) = events
            .iter()
            .find(|e| e.starts_with(&format!("completed:{}:", id)))
        {
            break e.clone();
        }
        assert!(
            std::time::Instant::now() < deadline,
            "completion event never dispatched: {:?}",
            events
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    assert!(completed.contains("echo background-done"), "{}", completed);

    // The session was finalized without any external query.
    let session = store.get(&id).unwrap();
    assert_eq!(session.status(), SessionStatus::Idle);
    assert_eq!(session.last_exit_code(), Some(0));
    assert!(
        session.pid().is_some(),
        "pid stays available after the monitor reaped the child"
    );
    let _ = store.kill(&id);
}

#[test]
fn test_store_monitor_thread_exits_on_drop() {
    let store = BackgroundShellStore::new(None);
    let id = store.spawn("echo monitor-exit", None).unwrap();
    // The monitor thread is lazily started on the first spawn.
    assert!(
        store.monitor.lock().unwrap().is_some(),
        "monitor thread lazily started on first spawn"
    );

    // Wait for the monitor to finalize the command (reaps the child).
    let session = store.get(&id).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    while std::time::Instant::now() < deadline && session.status() != SessionStatus::Idle {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(session.status(), SessionStatus::Idle);

    // Dropping the store joins the monitor thread; if the thread did not
    // exit, the join (and hence the drop) would hang past the bound.
    let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
    std::thread::spawn(move || {
        drop(store);
        let _ = done_tx.send(());
    });
    done_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("store drop hung: monitor thread never exited");
}

#[tokio::test]
async fn test_completion_eof_triggered_despite_large_poll_interval() {
    // The completion event must be dispatched on the output-reader EOF
    // wakeup, not on the next poll tick: with a 60s poll interval, only
    // the EOF wake can finalize the command within the test deadline.
    let sink = Arc::new(MemSink::default());
    let mut store = BackgroundShellStore::new(None);
    store.monitor_poll_interval_ms = 60_000;
    store.output_event_enabled = true;
    store.event_sink = Some(EventDispatcher::new(sink.clone()));

    let id = store.spawn("echo eof-wake", None).unwrap();

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    loop {
        let events = sink.events.lock().unwrap().clone();
        if events
            .iter()
            .any(|e| e.starts_with(&format!("completed:{}:", id)))
        {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "completion never dispatched (EOF wake missing?): {:?}",
            events
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let _ = store.kill(&id);
}

#[tokio::test]
async fn test_monitor_finalizes_concurrent_background_commands() {
    let sink = Arc::new(MemSink::default());
    let mut store = BackgroundShellStore::new(None);
    store.output_event_enabled = true;
    store.event_sink = Some(EventDispatcher::new(sink.clone()));

    let mut ids = Vec::new();
    for i in 0..5 {
        let id = store.spawn(&format!("echo bg-{}", i), None).unwrap();
        ids.push(id);
    }

    // Every command completes on its own (push-based); a single monitor
    // thread must finalize all of them.
    let events = wait_for_events(&sink, |events| {
        ids.iter().all(|id| {
            events
                .iter()
                .any(|e| e.starts_with(&format!("completed:{}:", id)))
        })
    });

    for id in &ids {
        let count = events
            .iter()
            .filter(|e| e.starts_with(&format!("completed:{}:", id)))
            .count();
        assert_eq!(
            count, 1,
            "completion for {} dispatched {} times: {:?}",
            id, count, events
        );
        let session = store.get(id).unwrap();
        assert_eq!(
            session.status(),
            SessionStatus::Idle,
            "session {} finalized by the monitor",
            id
        );
        assert_eq!(session.last_exit_code(), Some(0));
        let _ = store.kill_with(id, true);
    }
}

#[test]
fn test_kill_missing_session() {
    let store = BackgroundShellStore::new(None);
    assert!(!store.kill("nope").unwrap());
}

#[test]
fn test_normalize_cwd_path() {
    assert_eq!(normalize_cwd_path("/tmp/x"), "/tmp/x");
    assert_eq!(normalize_cwd_path("/tmp/x/"), "/tmp/x");
    assert_eq!(normalize_cwd_path(""), "");
    assert_eq!(normalize_cwd_path("///"), "");
}

#[test]
fn test_get_or_create_reuse_priorities() {
    let store = BackgroundShellStore::new(None);
    let opts_a = SessionCreateOptions {
        cwd: Some("/tmp/a".into()),
        ..Default::default()
    };

    let first = store.get_or_create(&opts_a, Some("t1")).unwrap();
    assert!(!first.reused);

    // Same task + cwd: reuse (priority 1).
    let second = store.get_or_create(&opts_a, Some("t1")).unwrap();
    assert!(second.reused);
    assert_eq!(second.session_id, first.session_id);
    assert_eq!(second.status, SessionStatus::Idle);

    // Same cwd, different task: reuse (priority 2), task id updated.
    let third = store.get_or_create(&opts_a, Some("t2")).unwrap();
    assert!(third.reused);
    assert_eq!(third.session_id, first.session_id);
    assert_eq!(third.task_id.as_deref(), Some("t2"));

    // Different cwd: new session.
    let opts_b = SessionCreateOptions {
        cwd: Some("/tmp/b".into()),
        ..Default::default()
    };
    let other = store.get_or_create(&opts_b, Some("t1")).unwrap();
    assert!(!other.reused);
    assert_ne!(other.session_id, first.session_id);

    // No task id: falls through to priority 2 (cwd only).
    let no_task = store.get_or_create(&opts_a, None).unwrap();
    assert!(no_task.reused);
    assert_eq!(no_task.session_id, first.session_id);
}

#[test]
fn test_get_or_create_trailing_slash_normalization() {
    let store = BackgroundShellStore::new(None);
    let with_slash = store
        .get_or_create(
            &SessionCreateOptions {
                cwd: Some("/tmp/x/".into()),
                ..Default::default()
            },
            Some("t1"),
        )
        .unwrap();
    let without_slash = store
        .get_or_create(
            &SessionCreateOptions {
                cwd: Some("/tmp/x".into()),
                ..Default::default()
            },
            Some("t1"),
        )
        .unwrap();
    assert!(without_slash.reused);
    assert_eq!(without_slash.session_id, with_slash.session_id);
}

#[test]
fn test_get_or_create_reuse_disabled() {
    let mut store = BackgroundShellStore::new(None);
    store.session_reuse_enabled = false;
    let opts = SessionCreateOptions::default();
    let first = store.get_or_create(&opts, Some("t1")).unwrap();
    let second = store.get_or_create(&opts, Some("t1")).unwrap();
    assert!(!first.reused);
    assert!(!second.reused);
    assert_ne!(first.session_id, second.session_id);
}

#[test]
fn test_get_or_create_max_sessions_per_task() {
    let mut store = BackgroundShellStore::new(None);
    store.max_sessions_per_task = Some(2);
    for _ in 0..2 {
        store
            .get_or_create(&SessionCreateOptions::default(), Some("t1"))
            .unwrap();
    }
    let err = store
        .get_or_create(&SessionCreateOptions::default(), Some("t1"))
        .unwrap_err();
    assert!(err.to_string().contains("Maximum sessions"));
}

#[test]
fn test_release_sessions_for_task() {
    let store = BackgroundShellStore::new(None);
    let opts = SessionCreateOptions {
        cwd: Some("/tmp/r".into()),
        ..Default::default()
    };
    let s1 = store.get_or_create(&opts, Some("t1")).unwrap();
    store
        .get_or_create(
            &SessionCreateOptions {
                cwd: Some("/tmp/r2".into()),
                ..Default::default()
            },
            Some("t1"),
        )
        .unwrap();
    store
        .get_or_create(
            &SessionCreateOptions {
                cwd: Some("/tmp/r3".into()),
                ..Default::default()
            },
            Some("t2"),
        )
        .unwrap();

    // Release (not terminate): task bindings cleared, sessions retained.
    assert_eq!(store.release_sessions_for_task("t1", false), 2);
    assert_eq!(store.session_count(), 3);
    let reused = store.get_or_create(&opts, Some("t3")).unwrap();
    assert!(reused.reused);
    assert_eq!(reused.session_id, s1.session_id);
    assert_eq!(reused.task_id.as_deref(), Some("t3"));

    // Terminate: sessions removed.
    assert_eq!(store.release_sessions_for_task("t3", true), 1);
    assert!(store.get(&s1.session_id).is_none());
    assert_eq!(store.session_count(), 2);
}

#[test]
fn test_sweep_idle_sessions() {
    let store = BackgroundShellStore::new(None);
    let created = store
        .get_or_create(&SessionCreateOptions::default(), Some("t1"))
        .unwrap();
    // last_active_at is far in the past for the sweep to match.
    store
        .get(&created.session_id)
        .unwrap()
        .last_active_at
        .lock()
        .unwrap()
        .clone_from(&(wf_common::time::now() - 60_000));
    assert_eq!(store.sweep_idle_sessions(30_000), 1);
    assert!(store.get(&created.session_id).is_none());
}

#[test]
fn test_session_mode_default_pipe() {
    let store = BackgroundShellStore::new(None);
    let id = store.spawn("echo hi", None).unwrap();
    let session = store.get(&id).unwrap();
    assert_eq!(session.mode_str(), "pipe");
    assert!(session.pid().is_some());
    let _ = store.kill(&id);
}

#[tokio::test]
async fn test_execute_in_session_accumulates_output() {
    let store = Arc::new(BackgroundShellStore::new(None));
    let created = store
        .get_or_create(&SessionCreateOptions::default(), Some("t1"))
        .unwrap();
    let sid = &created.session_id;

    let first = store
        .execute_in_session(sid, "echo a", Some(10_000))
        .unwrap();
    assert_eq!(first["success"], serde_json::json!(true));
    assert!(first["output"].as_str().unwrap().contains("a"));
    assert_eq!(
        store.get(sid).unwrap().status(),
        SessionStatus::Idle,
        "idle -> busy -> idle after a command"
    );

    let second = store
        .execute_in_session(sid, "echo b", Some(10_000))
        .unwrap();
    assert_eq!(second["success"], serde_json::json!(true));
    assert!(second["output"].as_str().unwrap().contains("b"));

    // Session-level output accumulates across commands.
    let session = store.get(sid).unwrap();
    let all = session.output.lock().unwrap().snapshot();
    assert!(all.contains("a"), "accumulated: {}", all);
    assert!(all.contains("b"), "accumulated: {}", all);

    // The incremental cursor still works across commands.
    session.read_new_output();
    assert_eq!(session.read_new_output(), "");

    let _ = store.kill(sid);
}

#[tokio::test]
async fn test_execute_in_session_busy_rejected() {
    let store = Arc::new(BackgroundShellStore::new(None));
    let created = store
        .get_or_create(&SessionCreateOptions::default(), Some("t1"))
        .unwrap();
    let sid = created.session_id.clone();

    let busy_store = store.clone();
    let busy_sid = sid.clone();
    let handle = std::thread::spawn(move || {
        busy_store
            .execute_in_session(&busy_sid, "sleep 3", Some(10_000))
            .unwrap()
    });

    // Wait until the session becomes busy.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        if store.get(&sid).unwrap().status() == SessionStatus::Busy {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(store.get(&sid).unwrap().status(), SessionStatus::Busy);

    let err = store.execute_in_session(&sid, "echo hi", None).unwrap_err();
    assert!(err.to_string().contains("busy"), "error: {}", err);

    handle.join().unwrap();
    assert_eq!(store.get(&sid).unwrap().status(), SessionStatus::Idle);
    let _ = store.kill(&sid);
}

#[tokio::test]
async fn test_execute_in_session_timeout_terminates() {
    let store = Arc::new(BackgroundShellStore::new(None));
    let created = store
        .get_or_create(&SessionCreateOptions::default(), Some("t1"))
        .unwrap();
    let sid = &created.session_id;
    let result = store
        .execute_in_session(sid, "sleep 30", Some(500))
        .unwrap();
    assert_eq!(result["timed_out"], serde_json::json!(true));
    assert_eq!(result["success"], serde_json::json!(false));
    assert_eq!(
        store.get(sid).unwrap().status(),
        SessionStatus::Idle,
        "session idle after timeout termination"
    );
    let _ = store.kill(sid);
}

#[tokio::test]
async fn test_stateless_vs_stateful_output_consistent() {
    // The stateless runner and the stateful session entry share the same
    // spawn configuration; the same command must produce the same output
    // on both paths.
    let output =
        crate::runner::run_command("printf 'alpha\\nbeta\\n'", None, 10_000, None, None, None)
            .await
            .unwrap();
    assert!(output.status.success());
    let stateless = String::from_utf8_lossy(&output.stdout).to_string();

    let store = Arc::new(BackgroundShellStore::new(None));
    let created = store
        .get_or_create(&SessionCreateOptions::default(), Some("cmp"))
        .unwrap();
    let sid = &created.session_id;
    let result = store
        .execute_in_session(sid, "printf 'alpha\\nbeta\\n'", Some(10_000))
        .unwrap();
    let stateful = result["output"].as_str().unwrap().to_string();

    assert_eq!(stateless, stateful, "outputs diverged between entries");
    let _ = store.kill(sid);
}

#[derive(Default)]
struct MemSink {
    events: StdMutex<Vec<String>>,
}

impl ShellEventSink for MemSink {
    fn on_session_created(&self, session_id: &str, reused: bool, task_id: Option<&str>) {
        self.events.lock().unwrap().push(format!(
            "created:{}:{}:{}",
            session_id,
            reused,
            task_id.unwrap_or("")
        ));
    }

    fn on_command_started(&self, session_id: &str, task_id: Option<&str>, command: &str) {
        self.events.lock().unwrap().push(format!(
            "started:{}:{}:{}",
            session_id,
            task_id.unwrap_or(""),
            command
        ));
    }

    fn on_output(&self, session_id: &str, task_id: Option<&str>, line: &str) {
        self.events.lock().unwrap().push(format!(
            "output:{}:{}:{}",
            session_id,
            task_id.unwrap_or(""),
            line
        ));
    }

    fn on_command_completed(
        &self,
        session_id: &str,
        task_id: Option<&str>,
        command: &str,
        exit_code: Option<i32>,
        success: bool,
    ) {
        self.events.lock().unwrap().push(format!(
            "completed:{}:{}:{}:{:?}:{}",
            session_id,
            task_id.unwrap_or(""),
            command,
            exit_code,
            success
        ));
    }

    fn on_session_terminated(&self, session_id: &str, task_id: Option<&str>) {
        self.events.lock().unwrap().push(format!(
            "terminated:{}:{}",
            session_id,
            task_id.unwrap_or("")
        ));
    }
}

#[tokio::test]
async fn test_output_events_dispatched() {
    let sink = Arc::new(MemSink::default());
    let mut store = BackgroundShellStore::new(None);
    store.output_event_enabled = true;
    store.event_sink = Some(EventDispatcher::new(sink.clone()));

    let created = store
        .get_or_create(&SessionCreateOptions::default(), Some("t1"))
        .unwrap();
    let sid = created.session_id.clone();
    let result = store
        .execute_in_session(&sid, "printf 'one\\ntwo\\n'", Some(10_000))
        .unwrap();
    assert_eq!(result["success"], serde_json::json!(true));

    let _ = store.kill_with(&sid, true);
    // The terminated event is queued asynchronously after kill; wait for
    // it (the execute_in_session path already flushed the rest).
    let events = wait_for_events(&sink, |events| {
        events
            .iter()
            .any(|e| e.starts_with(&format!("terminated:{}:t1", sid)))
    });
    assert!(
        events
            .iter()
            .any(|e| e.starts_with(&format!("created:{}:false:t1", sid))),
        "events: {:?}",
        events
    );
    assert!(
        events
            .iter()
            .any(|e| e == &format!("started:{}:t1:printf 'one\\ntwo\\n'", sid)),
        "events: {:?}",
        events
    );
    assert!(
        events
            .iter()
            .any(|e| e == &format!("output:{}:t1:one", sid)),
        "events: {:?}",
        events
    );
    assert!(
        events
            .iter()
            .any(|e| e == &format!("output:{}:t1:two", sid)),
        "events: {:?}",
        events
    );
    assert!(
        events
            .iter()
            .any(|e| e.starts_with(&format!("completed:{}:t1:", sid))),
        "events: {:?}",
        events
    );
    assert!(
        events
            .iter()
            .any(|e| e.starts_with(&format!("terminated:{}:t1", sid))),
        "events: {:?}",
        events
    );
}

#[tokio::test]
async fn test_completion_event_ordered_after_output_events() {
    // The completion event must be delivered after every output event of
    // the same command (drain-gated finalization), so a push consumer
    // never sees the command "complete" before its trailing output.
    let sink = Arc::new(MemSink::default());
    let mut store = BackgroundShellStore::new(None);
    store.output_event_enabled = true;
    store.event_sink = Some(EventDispatcher::new(sink.clone()));

    let id = store
        .spawn("for i in $(seq 1 200); do echo line-$i; done", None)
        .unwrap();

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    let events = loop {
        let events = sink.events.lock().unwrap().clone();
        if events
            .iter()
            .any(|e| e.starts_with(&format!("completed:{}:", id)))
        {
            break events;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "completion never arrived: {:?}",
            events
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    };

    let mut last_output = None;
    let mut completed = None;
    for (i, e) in events.iter().enumerate() {
        if e.starts_with(&format!("output:{}:", id)) {
            last_output = Some(i);
        }
        if e.starts_with(&format!("completed:{}:", id)) {
            completed = Some(i);
        }
    }
    let last_output = last_output.expect("output events were delivered");
    let completed = completed.expect("completed event was delivered");
    assert!(
        completed > last_output,
        "completed at {} must follow last output at {}: {:?}",
        completed,
        last_output,
        events
    );
    let _ = store.kill(&id);
}

#[test]
fn test_blocked_sink_does_not_backpressure_output_reading() {
    // A sink whose on_output blocks forever must not prevent the reader
    // threads from draining the process output into the session buffer
    // (backpressure would leave the reader stuck and output incomplete).
    let (_never_tx, never_rx) = std::sync::mpsc::channel::<()>();
    let sink = Arc::new(BlockingSink {
        gate: never_rx.into(),
    });
    let mut store = BackgroundShellStore::new(None);
    store.output_event_enabled = true;
    store.event_sink = Some(EventDispatcher::new(sink));

    let id = store
        .spawn("printf 'blocked-a\\nblocked-b\\n'", None)
        .unwrap();

    // The monitor thread finalizes the session independently of the
    // blocked sink (idle is set before the async dispatch flush).
    let session = store.get(&id).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    while std::time::Instant::now() < deadline && session.status() != SessionStatus::Idle {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(session.status(), SessionStatus::Idle);

    // The full output was captured even though the sink never consumed a
    // single line.
    let output = session.output.lock().unwrap().snapshot();
    assert!(output.contains("blocked-a"), "output: {}", output);
    assert!(output.contains("blocked-b"), "output: {}", output);
    let _ = store.kill(&id);
}

/// Sink whose `on_output` blocks forever, used to prove the dispatch
/// channel decouples readers from sink work.
struct BlockingSink {
    gate: StdMutex<std::sync::mpsc::Receiver<()>>,
}

impl ShellEventSink for BlockingSink {
    fn on_output(&self, _session_id: &str, _task_id: Option<&str>, _line: &str) {
        let gate = self.gate.lock().unwrap();
        let _ = gate.recv();
    }
}

#[tokio::test]
async fn test_events_disabled_by_default() {
    let sink = Arc::new(MemSink::default());
    let mut store = BackgroundShellStore::new(None);
    store.event_sink = Some(EventDispatcher::new(sink.clone()));

    let created = store
        .get_or_create(&SessionCreateOptions::default(), Some("t1"))
        .unwrap();
    store
        .execute_in_session(&created.session_id, "echo hi", Some(10_000))
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(200));
    assert!(
        sink.events.lock().unwrap().is_empty(),
        "no events without output_event_enabled"
    );
    let _ = store.kill(&created.session_id);
}
