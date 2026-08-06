//! Event sink abstraction for shell session events.
//!
//! A [`ShellEventSink`] receives session lifecycle and per-line output events
//! (aligned with the TS terminal service `output:received` events). The trait
//! is defined here in `wf-shell`; upper crates (e.g. `wf-runtime`) provide an
//! implementation that bridges into the `wf_core::EventBus`.
//!
//! Dispatch is asynchronous: the engine never calls a sink directly from a
//! reader or engine thread. [`EventDispatcher`] queues events and a single
//! dedicated thread forwards them to the sink, so a slow or blocking sink
//! cannot backpressure the output pipeline. The channel itself stays
//! unbounded (a producer never blocks), but queued **output** events are
//! capped: once the backlog reaches [`MAX_QUEUED_OUTPUT_EVENTS`] further
//! output lines are dropped and counted, so a wedged sink cannot grow the
//! queue without bound. A drop sentinel is synthesized on the next delivered
//! output so a push consumer can fall back to the pull-side
//! [`crate::engine::OutputBuffer`]. Lifecycle and [`Flush`](ShellEvent::Flush)
//! events are never dropped. The dispatcher is created once per
//! [`crate::engine::BackgroundShellStore`] when a sink is configured, keeping
//! the extra thread count bounded.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Cap on queued `Output` events per dispatcher. Lifecycle/`Flush` events may
/// transiently exceed it; the bound is approximate by event count, not bytes.
const MAX_QUEUED_OUTPUT_EVENTS: usize = 4096;

/// Receives shell session lifecycle and output events. All methods default to
/// no-ops; the session store only dispatches events when a sink is registered
/// (via [`crate::config::ShellToolConfig`]) and output events are enabled.
pub trait ShellEventSink: Send + Sync {
    /// A session was created, or an existing idle session was reused.
    fn on_session_created(&self, _session_id: &str, _reused: bool, _task_id: Option<&str>) {}

    /// A command started running in a session.
    fn on_command_started(&self, _session_id: &str, _task_id: Option<&str>, _command: &str) {}

    /// A complete output line was received from the session. Empty lines are
    /// skipped; CRLF is normalized on the PTY path, raw bytes are preserved
    /// on the pipe path (same as the output buffer).
    fn on_output(&self, _session_id: &str, _task_id: Option<&str>, _line: &str) {}

    /// A command finished (exited or was terminated).
    fn on_command_completed(
        &self,
        _session_id: &str,
        _task_id: Option<&str>,
        _command: &str,
        _exit_code: Option<i32>,
        _success: bool,
    ) {
    }

    /// A session was terminated (killed and removed from the store).
    fn on_session_terminated(&self, _session_id: &str, _task_id: Option<&str>) {}
}

/// A queued shell session event forwarded to the sink by
/// [`EventDispatcher`]. Events carry owned payloads so they can cross the
/// channel boundary; ordering is preserved per send.
#[derive(Debug)]
pub(crate) enum ShellEvent {
    SessionCreated {
        session_id: String,
        reused: bool,
        task_id: Option<String>,
    },
    CommandStarted {
        session_id: String,
        task_id: Option<String>,
        command: String,
    },
    Output {
        session_id: String,
        task_id: Option<String>,
        line: String,
    },
    CommandCompleted {
        session_id: String,
        task_id: Option<String>,
        command: String,
        exit_code: Option<i32>,
        success: bool,
    },
    SessionTerminated {
        session_id: String,
        task_id: Option<String>,
    },
    /// Barrier marker: acknowledges on `ack` once every event queued before it
    /// has been delivered, letting a caller wait for the queue to drain.
    Flush { ack: std::sync::mpsc::Sender<()> },
}

/// Routes shell events to a [`ShellEventSink`] asynchronously.
///
/// One dedicated dispatch thread consumes the queue and calls the sink, so the
/// engine (in particular the per-command output reader threads) never blocks
/// on sink work. Events are delivered in queue order. A
/// [`Flush`](ShellEvent::Flush) marker lets a caller wait until every event
/// queued before the call has been delivered.
pub(crate) struct EventDispatcher {
    tx: tokio::sync::mpsc::UnboundedSender<ShellEvent>,
    /// Number of events currently queued (incremented on enqueue, decremented
    /// on consume). Approximate: transiently overshoots by the events being
    /// sent concurrently.
    queued: Arc<AtomicUsize>,
    /// Output lines dropped because the queue reached the cap. Store-level
    /// global; a sentinel attributes them to the next delivered output.
    dropped: Arc<AtomicUsize>,
    /// Cap on queued output events (default [`MAX_QUEUED_OUTPUT_EVENTS`];
    /// injectable in tests).
    max_queued_output_events: usize,
    _thread: std::thread::JoinHandle<()>,
}

impl EventDispatcher {
    /// Spawn the dispatch thread and return a shared handle. Dropping every
    /// handle (and hence every clone of the sender) makes the thread exit.
    pub(crate) fn new(sink: Arc<dyn ShellEventSink>) -> Arc<Self> {
        Self::spawn(sink, MAX_QUEUED_OUTPUT_EVENTS)
    }

    /// Spawn the dispatch thread with a test-injectable output cap.
    #[cfg(test)]
    pub(crate) fn with_output_capacity(
        sink: Arc<dyn ShellEventSink>,
        max_queued_output_events: usize,
    ) -> Arc<Self> {
        Self::spawn(sink, max_queued_output_events)
    }

    fn spawn(sink: Arc<dyn ShellEventSink>, max_queued_output_events: usize) -> Arc<Self> {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<ShellEvent>();
        let queued = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicUsize::new(0));
        let thread_queued = Arc::clone(&queued);
        let thread_dropped = Arc::clone(&dropped);
        let thread = std::thread::Builder::new()
            .name("shell-event-dispatch".into())
            .spawn(move || {
                while let Some(event) = rx.blocking_recv() {
                    thread_queued.fetch_sub(1, Ordering::SeqCst);
                    Self::deliver(sink.as_ref(), event, &thread_dropped);
                }
            })
            .expect("failed to spawn shell event dispatch thread");
        Arc::new(Self {
            tx,
            queued,
            dropped,
            max_queued_output_events,
            _thread: thread,
        })
    }

    /// Queue an event. Non-blocking; a closed queue (dispatcher dropped) is
    /// silently ignored so producers never observe dispatch-side failures.
    /// `Output` events are dropped (and counted) once the queued backlog
    /// reaches the cap, so a wedged sink cannot grow the queue without bound
    /// while the reader threads stay unblocked. Lifecycle and `Flush` events
    /// always go through.
    pub(crate) fn send(&self, event: ShellEvent) {
        let is_output = matches!(event, ShellEvent::Output { .. });
        if is_output && self.queued.load(Ordering::SeqCst) >= self.max_queued_output_events {
            self.dropped.fetch_add(1, Ordering::SeqCst);
            return;
        }
        let _ = self.enqueue(event);
    }

    /// Queue an event and bump the shared in-queue counter.
    fn enqueue(&self, event: ShellEvent) -> bool {
        self.queued.fetch_add(1, Ordering::SeqCst);
        self.tx.send(event).is_ok()
    }

    /// Wait until every event queued before this call has been delivered to
    /// the sink, bounded by `timeout` so a wedged dispatch thread never blocks
    /// a caller forever. The flush marker is never dropped, so the barrier is
    /// reliable even when the queue is full.
    pub(crate) fn flush(&self, timeout: Duration) {
        let (ack, done) = std::sync::mpsc::channel::<()>();
        if !self.enqueue(ShellEvent::Flush { ack }) {
            return;
        }
        let _ = done.recv_timeout(timeout);
    }

    /// Deliver an event, synthesizing a drop sentinel before an `Output` when
    /// lines were dropped since the last delivered output. The sentinel is
    /// attributed to the current event's session and keeps the push stream
    /// ordered: it appears exactly where the dropped lines would have been.
    fn deliver(sink: &dyn ShellEventSink, event: ShellEvent, dropped: &AtomicUsize) {
        if let ShellEvent::Output {
            session_id,
            task_id,
            ..
        } = &event
        {
            let dropped_count = dropped.swap(0, Ordering::SeqCst);
            if dropped_count > 0 {
                sink.on_output(
                    session_id,
                    task_id.as_deref(),
                    &format!("[{} output lines dropped]", dropped_count),
                );
            }
        }
        Self::deliver_event(sink, event);
    }

    fn deliver_event(sink: &dyn ShellEventSink, event: ShellEvent) {
        match event {
            ShellEvent::SessionCreated {
                session_id,
                reused,
                task_id,
            } => sink.on_session_created(&session_id, reused, task_id.as_deref()),
            ShellEvent::CommandStarted {
                session_id,
                task_id,
                command,
            } => sink.on_command_started(&session_id, task_id.as_deref(), &command),
            ShellEvent::Output {
                session_id,
                task_id,
                line,
            } => sink.on_output(&session_id, task_id.as_deref(), &line),
            ShellEvent::CommandCompleted {
                session_id,
                task_id,
                command,
                exit_code,
                success,
            } => sink.on_command_completed(
                &session_id,
                task_id.as_deref(),
                &command,
                exit_code,
                success,
            ),
            ShellEvent::SessionTerminated {
                session_id,
                task_id,
            } => {
                sink.on_session_terminated(&session_id, task_id.as_deref());
            }
            ShellEvent::Flush { ack } => {
                let _ = ack.send(());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use std::sync::Mutex as StdMutex;

    /// Sink that records every event and blocks `on_output` until released
    /// once (first output call) so the queue can fill while the dispatch
    /// thread is wedged. `gate_open` flips to true after the first block so
    /// subsequent outputs are delivered without blocking.
    struct GatedSink {
        events: StdMutex<Vec<String>>,
        entered: std::sync::mpsc::SyncSender<()>,
        release: StdMutex<std::sync::mpsc::Receiver<()>>,
        gate_open: AtomicBool,
    }

    impl GatedSink {
        fn new() -> (
            Arc<Self>,
            std::sync::mpsc::Receiver<()>,
            std::sync::mpsc::Sender<()>,
        ) {
            let (entered_tx, entered_rx) = std::sync::mpsc::sync_channel::<()>(32);
            let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
            let sink = Arc::new(Self {
                events: StdMutex::new(Vec::new()),
                entered: entered_tx,
                release: StdMutex::new(release_rx),
                gate_open: AtomicBool::new(false),
            });
            (sink, entered_rx, release_tx)
        }

        fn open(&self) {
            self.gate_open.store(true, Ordering::SeqCst);
        }

        fn recorded(&self) -> Vec<String> {
            self.events.lock().unwrap().clone()
        }
    }

    impl ShellEventSink for GatedSink {
        fn on_output(&self, session_id: &str, _task_id: Option<&str>, line: &str) {
            let _ = self.entered.send(());
            self.events
                .lock()
                .unwrap()
                .push(format!("{}|{}", session_id, line));
            if !self.gate_open.load(Ordering::SeqCst) {
                let release = self.release.lock().unwrap();
                let _ = release.recv();
            }
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
                .push(format!("{}|completed", session_id));
        }

        fn on_session_created(&self, session_id: &str, _reused: bool, _task_id: Option<&str>) {
            self.events
                .lock()
                .unwrap()
                .push(format!("{}|created", session_id));
        }

        fn on_session_terminated(&self, session_id: &str, _task_id: Option<&str>) {
            self.events
                .lock()
                .unwrap()
                .push(format!("{}|terminated", session_id));
        }
    }

    /// Overflow the output queue while the dispatch thread is wedged in the
    /// sink, then release it. Asserts: a sentinel line appears before the next
    /// delivered output, the dropped lines are absent, lifecycle events are
    /// delivered, and the completion event is still ordered after every output.
    #[test]
    fn test_output_queue_overflow_synthesizes_sentinel_preserving_order() {
        let (sink, entered_rx, release_tx) = GatedSink::new();
        let dispatcher = EventDispatcher::with_output_capacity(sink.clone(), 2);

        dispatcher.send(ShellEvent::Output {
            session_id: "s1".into(),
            task_id: None,
            line: "1".into(),
        });
        // The dispatch thread is now blocked inside on_output("1").
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();

        // With capacity 2 the next two outputs fit, the rest are dropped.
        for i in 2..=10 {
            dispatcher.send(ShellEvent::Output {
                session_id: "s1".into(),
                task_id: None,
                line: i.to_string(),
            });
        }
        // Lifecycle events must survive a full queue.
        dispatcher.send(ShellEvent::SessionCreated {
            session_id: "s1".into(),
            reused: true,
            task_id: None,
        });
        dispatcher.send(ShellEvent::CommandCompleted {
            session_id: "s1".into(),
            task_id: None,
            command: "cmd".into(),
            exit_code: Some(0),
            success: true,
        });

        // Release the blocked sink; the queued events drain.
        sink.open();
        release_tx.send(()).unwrap();
        dispatcher.flush(Duration::from_secs(5));

        let events = sink.recorded();
        assert_eq!(events[0], "s1|1", "events: {:?}", events);
        // Sentinel before the next delivered output: 2..=10 minus the two that
        // fit (2,3) leaves 7 dropped lines.
        let sentinel = "s1|[7 output lines dropped]".to_string();
        let sentinel_idx = events
            .iter()
            .position(|e| *e == sentinel)
            .expect("sentinel");
        let out2 = events.iter().position(|e| e == "s1|2").expect("output 2");
        let out3 = events.iter().position(|e| e == "s1|3").expect("output 3");
        assert!(
            sentinel_idx < out2 && out2 < out3,
            "sentinel must precede later outputs: {:?}",
            events
        );
        assert!(
            !events.iter().any(|e| e == "s1|4"),
            "dropped lines absent: {:?}",
            events
        );
        assert!(
            events.iter().any(|e| e == "s1|created"),
            "lifecycle delivered despite full queue: {:?}",
            events
        );
        let completed = events
            .iter()
            .position(|e| e == "s1|completed")
            .expect("completed");
        assert!(
            completed > out3,
            "completed must follow every output: {:?}",
            events
        );
    }

    /// A `Flush` barrier must not be dropped when the queue is full; the
    /// barrier returns promptly once the wedge is released.
    #[test]
    fn test_flush_not_dropped_when_queue_full() {
        let (sink, entered_rx, release_tx) = GatedSink::new();
        let dispatcher = EventDispatcher::with_output_capacity(sink.clone(), 2);

        dispatcher.send(ShellEvent::Output {
            session_id: "s1".into(),
            task_id: None,
            line: "a".into(),
        });
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        for i in 2..=10 {
            dispatcher.send(ShellEvent::Output {
                session_id: "s1".into(),
                task_id: None,
                line: i.to_string(),
            });
        }

        let dispatcher_thread = dispatcher.clone();
        let flush_handle = std::thread::spawn(move || {
            dispatcher_thread.flush(Duration::from_secs(8));
        });
        // Give the flush marker time to enqueue behind the full queue.
        std::thread::sleep(Duration::from_millis(100));

        sink.open();
        release_tx.send(()).unwrap();
        let start = std::time::Instant::now();
        flush_handle.join().unwrap();
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "flush blocked far longer than a draining queue requires"
        );
    }
}
