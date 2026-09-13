//! Event-driven trigger listener: the orchestration loop.
//!
//! Subscribes to the runtime EventBus and reacts to events by matching them
//! against registered trigger templates. This module only wires the stages
//! together and owns the lifecycle (event loop, dispatch loop, action
//! spawn); each stage has its own module:
//!
//! - [`crate::trigger::matcher`]: which templates are candidates for one event;
//! - [`crate::trigger::arbiter`]: which candidates win their competition scope;
//! - [`crate::trigger::governor`]: whether a winner may fire right now
//!   (re-entrancy guard and `max_triggers` budget);
//! - [`crate::trigger::subscription`]: the event-bus fan-in feeding the loop.
//!
//! The business logic behind a match lives in the ports of
//! [`crate::trigger::ports`] ([`TriggerTemplateRegistry`],
//! [`TriggerActionRunner`] and [`crate::trigger::ports::SubworkflowRunner`]),
//! all implemented by wf-runtime during assembly, so wf-workflow stays
//! decoupled from wf-resource and from concrete trigger business.

use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};
use wf_common::gate::ConcurrencyGate;
use wf_core::EventBus;
use wf_types::events::{BaseEvent, EventType};
use wf_types::trigger::{TriggerRuntimeLimits, TriggerTemplate};

use crate::trigger::arbiter::arbitrate;
use crate::trigger::governor::{FirePermit, TriggerGovernor};
use crate::trigger::matcher::{candidates, is_cold_start, match_key};
use crate::trigger::ports::{TriggerActionRunner, TriggerTemplateRegistry};
use crate::trigger::subscription::{subscribed_types, EventFanIn};

/// A matched trigger template and its source event, ready for dispatch.
struct TriggerMatch {
    template: TriggerTemplate,
    event: BaseEvent,
    key: String,
}

/// Listens for events and executes matching trigger templates.
///
/// Matching and dispatch are separated: the event loop receives events and
/// spawns matching tasks, which send `TriggerMatch` values through an
/// internal channel. A dedicated dispatch loop consumes the channel and
/// executes each matched action, keeping the event loop responsive even
/// under heavy template matching load.
///
/// Matching is best-effort: failures are logged, never propagated to the
/// emitting execution. Events are handled concurrently: each matching
/// (execution, template) pair runs in its own task, so the in-flight guard
/// and `max_triggers` budget are meaningful even for events that arrive
/// back-to-back. Idempotency beyond those two limits (e.g. repeated
/// compression requests for the same array version) is the responsibility of
/// the [`TriggerActionRunner`].
#[derive(Clone)]
pub struct TriggerEventListener {
    bus: Arc<EventBus>,
    registry: Arc<dyn TriggerTemplateRegistry>,
    runner: Arc<dyn TriggerActionRunner>,
    /// Re-entrancy guard and firing budget; shared by every clone so a claim
    /// taken during dispatch is released by the action that consumed it.
    governor: Arc<TriggerGovernor>,
    /// Event types with at least one registered template; the listener
    /// subscribes a typed channel per type. Empty when no template declares
    /// a parseable type (general-channel fallback).
    interested_types: Vec<EventType>,
    /// Optional concurrency gate bounding concurrent trigger-action
    /// execution. `None` keeps the unbounded behavior.
    concurrency_gate: Option<Arc<ConcurrencyGate>>,
    /// Runtime limits (concurrency + dispatch circuit breaker). Default is
    /// unbounded with no burst cap. Over-limit winners are dropped with a
    /// warning, never queued.
    runtime_limits: TriggerRuntimeLimits,
    shutdown: CancellationToken,
}

impl TriggerEventListener {
    pub fn new(
        bus: Arc<EventBus>,
        registry: Arc<dyn TriggerTemplateRegistry>,
        runner: Arc<dyn TriggerActionRunner>,
        shutdown: CancellationToken,
    ) -> Self {
        let interested_types = subscribed_types(&registry.templates());
        Self {
            bus,
            registry,
            runner,
            governor: Arc::new(TriggerGovernor::new()),
            interested_types,
            concurrency_gate: None,
            runtime_limits: TriggerRuntimeLimits::default(),
            shutdown,
        }
    }

    /// Bound concurrent trigger-action execution with a shared gate. When
    /// `None` (default), actions spawn unbounded.
    pub fn with_concurrency_gate(mut self, gate: Arc<ConcurrencyGate>) -> Self {
        self.concurrency_gate = Some(gate);
        self
    }

    /// Apply shared runtime limits. `max_concurrent_actions` installs a
    /// concurrency gate when none was set explicitly; the dispatch burst
    /// cap is enforced per event (warn-and-drop, never queued). Absent
    /// values keep the unbounded behavior.
    pub fn with_runtime_limits(mut self, limits: TriggerRuntimeLimits) -> Self {
        if let Some(max) = limits.max_concurrent_actions {
            if self.concurrency_gate.is_none() && max > 0 {
                self.concurrency_gate = Some(Arc::new(ConcurrencyGate::new(max as usize)));
            }
        }
        self.runtime_limits = limits;
        self
    }

    /// Run the listener loop until shutdown is requested.
    ///
    /// Spawns a background dispatch loop that consumes matched templates from
    /// an internal channel. The main event loop stays responsive by offloading
    /// template matching to spawned tasks.
    pub async fn run(&self) {
        let (match_tx, mut match_rx) = mpsc::unbounded_channel::<Vec<TriggerMatch>>();

        // Background dispatch loop: consumes matched results and executes
        // actions. Runs in a separate task so the event loop is never blocked
        // by action execution.
        let listener = self.clone();
        let shutdown = self.shutdown.clone();
        let dispatch_handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = shutdown.cancelled() => {
                        debug!("TriggerEventListener dispatch loop shutdown");
                        break;
                    }
                    matched = match_rx.recv() => match matched {
                        Some(batch) => {
                            for one in batch {
                                listener.execute_action(one).await;
                            }
                        }
                        None => break,
                    },
                }
            }
        });

        // Fan-in event source: one forwarder per registered event type
        // (typed channels), or the general channel when no template declares
        // a parseable type. The main loop only receives events that at least
        // one template can match, avoiding irrelevant-channel load.
        let mut fan_in = EventFanIn::new(&self.bus, &self.interested_types);

        // Main event loop: receive events, spawn matching in background tasks.
        loop {
            tokio::select! {
                _ = self.shutdown.cancelled() => {
                    debug!("TriggerEventListener shutdown requested");
                    break;
                }
                event = fan_in.recv() => match event {
                    Some(event) => {
                        let tx = match_tx.clone();
                        let listener = self.clone();
                        tokio::spawn(async move {
                            let batch = listener.select_templates(&event);
                            if !batch.is_empty() {
                                let _ = tx.send(batch);
                            }
                        });
                    }
                    None => break,
                },
            }
        }

        // Drop the sender to close the channel, then wait for the dispatch
        // loop to finish processing in-flight matches.
        drop(match_tx);
        let _ = dispatch_handle.await;
    }

    /// Run the pipeline for one event: candidate matching, scope
    /// arbitration, then mapping the winners to dispatchable matches.
    fn select_templates(&self, event: &BaseEvent) -> Vec<TriggerMatch> {
        let templates = self.registry.templates();
        let winners = arbitrate(&candidates(&templates, event), event, &self.runtime_limits);
        if winners.is_empty() {
            self.log_no_match(&templates, event);
            return Vec::new();
        }
        winners
            .into_iter()
            .map(|template| self.dispatchable(template, event))
            .collect()
    }

    /// Turn one winner into its dispatch match, flagging declarations the
    /// runtime does not execute yet (bypass path: load-time validation
    /// rejects the multi-effect opt-in, so reaching here means validation
    /// was bypassed).
    fn dispatchable(&self, template: TriggerTemplate, event: &BaseEvent) -> TriggerMatch {
        if template.allow_multi_effect == Some(true) {
            warn!(
                "Trigger '{}' declares multi-effect execution, which is not implemented; executing the single winner (validation was bypassed)",
                template.name,
            );
        }
        let key = match_key(event, &template.name);
        TriggerMatch {
            template,
            event: event.clone(),
            key,
        }
    }

    /// Debug-log only events that have templates configured for their type,
    /// so an unrelated high-volume event never adds noise.
    fn log_no_match(&self, templates: &[TriggerTemplate], event: &BaseEvent) {
        let type_configured = templates.iter().any(|t| {
            t.condition
                .as_ref()
                .is_some_and(|c| c.event_type == event.r#type.as_str())
        });
        if !type_configured {
            return;
        }
        match event.execution_id.as_ref() {
            Some(execution_id) => debug!(
                "No trigger template matched event {} for execution {}",
                event.r#type.as_str(),
                execution_id
            ),
            None => debug!(
                "No trigger template matched execution-less event {}",
                event.r#type.as_str(),
            ),
        }
    }

    /// Execute a matched trigger action: claim a fire permit from the
    /// governor, then spawn the action runner. The permit key is the
    /// matcher's dispatch key, so the budget is per-execution
    /// (`execution_id:template`) for execution-scoped events and per-fire
    /// (`fire_id`, falling back to `template:timestamp`) for execution-less
    /// creation events.
    async fn execute_action(&self, matched: TriggerMatch) {
        let TriggerMatch {
            template,
            event,
            key,
        } = matched;

        // Defense in depth: execution-less events must only drive cold-start
        // actions (candidate matching already filters, but templates can be
        // registered around validation).
        if event.execution_id.is_none() && !is_cold_start(&template) {
            return;
        }
        let scope_label = event
            .execution_id
            .clone()
            .unwrap_or_else(|| match_key(&event, &template.name));

        match self.governor.request(&key, template.max_triggers) {
            FirePermit::AlreadyRunning => {
                debug!(
                    "Trigger '{}' already running for {}, skipping",
                    template.name, scope_label
                );
                return;
            }
            FirePermit::BudgetExhausted => {
                debug!(
                    "Trigger '{}' reached max_triggers ({}) for {}, skipping",
                    template.name,
                    template.max_triggers.unwrap_or_default(),
                    scope_label
                );
                return;
            }
            FirePermit::Granted => {}
        }

        let listener = self.clone();
        let shutdown = self.shutdown.clone();
        let gate = self.concurrency_gate.clone();
        tokio::spawn(async move {
            // Concurrency gate: wait for a permit before running (queued
            // triggers hold their in-flight slot). No gate keeps the action
            // unbounded.
            let _permit = match gate {
                Some(gate) => match gate.acquire_wait().await {
                    Ok(permit) => Some(permit),
                    Err(e) => {
                        warn!(
                            "Trigger '{}' rejected by concurrency gate: {}",
                            template.name, e
                        );
                        listener.governor.release(&key);
                        return;
                    }
                },
                None => None,
            };
            let run = listener.runner.run(&template, &event);
            tokio::select! {
                outcome = run => {
                    if let Err(e) = outcome {
                        warn!("Trigger '{}' failed: {}", template.name, e);
                    }
                }
                _ = shutdown.cancelled() => {
                    debug!("Trigger '{}' aborted at shutdown", template.name);
                }
            }
            listener.governor.release(&key);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    use async_trait::async_trait;
    use tokio::sync::Semaphore;
    use wf_types::trigger::TriggerDispatchMode;

    use crate::error::{WorkflowError, WorkflowResult};
    use crate::trigger::test_fixtures::*;

    /// Counts action runs, optionally failing for one event type and
    /// optionally blocking each run until a permit is handed out.
    struct RecordingRunner {
        started: Arc<AtomicU32>,
        calls: Arc<AtomicU32>,
        abort_on_event_type: Option<String>,
        hold: Option<Arc<Semaphore>>,
    }

    impl RecordingRunner {
        fn new() -> Self {
            Self {
                started: Arc::new(AtomicU32::new(0)),
                calls: Arc::new(AtomicU32::new(0)),
                abort_on_event_type: None,
                hold: None,
            }
        }

        /// Every run blocks until a permit is added to the returned
        /// semaphore, so tests control when actions finish.
        fn held(hold: Arc<Semaphore>) -> Self {
            Self {
                hold: Some(hold),
                ..Self::new()
            }
        }
    }

    #[async_trait]
    impl TriggerActionRunner for RecordingRunner {
        async fn run(&self, _template: &TriggerTemplate, event: &BaseEvent) -> WorkflowResult<()> {
            self.started.fetch_add(1, Ordering::SeqCst);
            if let Some(hold) = &self.hold {
                let Ok(permit) = hold.acquire().await else {
                    return Ok(());
                };
                drop(permit);
            }
            if self
                .abort_on_event_type
                .as_deref()
                .is_some_and(|t| t == event.r#type.as_str())
            {
                return Err(WorkflowError::TriggerError("runner failure".to_string()));
            }
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    struct StaticRegistry(Vec<TriggerTemplate>);

    impl TriggerTemplateRegistry for StaticRegistry {
        fn templates(&self) -> Vec<TriggerTemplate> {
            self.0.clone()
        }
    }

    fn start_listener(
        bus: &Arc<EventBus>,
        registry: Arc<dyn TriggerTemplateRegistry>,
        runner: Arc<dyn TriggerActionRunner>,
    ) -> Arc<TriggerEventListener> {
        let listener = Arc::new(TriggerEventListener::new(
            bus.clone(),
            registry,
            runner,
            CancellationToken::new(),
        ));
        tokio::spawn({
            let listener = listener.clone();
            async move { listener.run().await }
        });
        listener
    }

    /// The listener's subscription is created when its spawned task first
    /// polls; publishing before that loses the event. Wait until the bus
    /// sees the expected number of receivers before publishing. Bounded: a
    /// wrong expectation must fail loudly instead of spinning forever.
    ///
    /// The listener subscribes typed channels when templates declare
    /// parseable event types, so the total (general + typed) count is
    /// observed: `receiver_count` only covers the general channel and
    /// stays zero for typed-only subscriptions.
    async fn wait_for_listener(bus: &EventBus, expected_receivers: usize) {
        for _ in 0..200 {
            if bus.total_receiver_count() >= expected_receivers {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!(
            "expected {} receivers within 2s, got {}",
            expected_receivers,
            bus.total_receiver_count()
        );
    }

    /// Poll a condition until it holds (2s budget) so async assertions do
    /// not depend on fixed sleeps.
    async fn wait_until(cond: impl Fn() -> bool) {
        for _ in 0..200 {
            if cond() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    fn listener_for(templates: Vec<TriggerTemplate>) -> TriggerEventListener {
        TriggerEventListener::new(
            Arc::new(EventBus::new(4)),
            Arc::new(StaticRegistry(templates)),
            Arc::new(RecordingRunner::new()),
            CancellationToken::new(),
        )
    }

    fn match_names(matches: &[TriggerMatch]) -> Vec<&str> {
        matches.iter().map(|m| m.template.name.as_str()).collect()
    }

    #[tokio::test]
    async fn matching_event_runs_runner_once() {
        let bus = Arc::new(EventBus::new(64));
        let registry: Arc<dyn TriggerTemplateRegistry> =
            Arc::new(StaticRegistry(vec![event_template(
                "t1",
                "NODE_COMPLETED",
                0,
            )]));
        let runner = Arc::new(RecordingRunner::new());
        let calls = runner.calls.clone();
        start_listener(&bus, registry, runner.clone());
        wait_for_listener(&bus, 1).await;

        bus.publish(base_event(EventType::NodeCompleted, "exec-1"))
            .unwrap();
        wait_until(|| calls.load(Ordering::SeqCst) == 1).await;
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn event_without_matching_template_skips_runner() {
        let bus = Arc::new(EventBus::new(64));
        let registry: Arc<dyn TriggerTemplateRegistry> =
            Arc::new(StaticRegistry(vec![event_template(
                "t1",
                "NODE_COMPLETED",
                0,
            )]));
        let runner = Arc::new(RecordingRunner::new());
        let calls = runner.calls.clone();
        start_listener(&bus, registry, runner.clone());
        wait_for_listener(&bus, 1).await;

        bus.publish(base_event(EventType::TokenLimitExceeded, "exec-2"))
            .unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(calls.load(Ordering::SeqCst), 0, "no match -> no run");
    }

    #[tokio::test]
    async fn failing_runner_is_logged_and_never_stops_the_listener() {
        let bus = Arc::new(EventBus::new(64));
        let registry: Arc<dyn TriggerTemplateRegistry> =
            Arc::new(StaticRegistry(vec![event_template(
                "t1",
                "NODE_COMPLETED",
                0,
            )]));
        let runner = Arc::new(RecordingRunner {
            abort_on_event_type: Some("NODE_COMPLETED".to_string()),
            ..RecordingRunner::new()
        });
        let started = runner.started.clone();
        let calls = runner.calls.clone();
        start_listener(&bus, registry, runner.clone());
        wait_for_listener(&bus, 1).await;

        bus.publish(base_event(EventType::NodeCompleted, "exec-f"))
            .unwrap();
        wait_until(|| started.load(Ordering::SeqCst) == 1).await;
        // The failure is swallowed by the listener, and the next execution is
        // still served: the failing pair released its permit.
        bus.publish(base_event(EventType::NodeCompleted, "exec-g"))
            .unwrap();
        wait_until(|| started.load(Ordering::SeqCst) == 2).await;
        assert_eq!(
            calls.load(Ordering::SeqCst),
            0,
            "failures never count as runs"
        );
    }

    #[tokio::test]
    async fn duplicate_event_is_skipped_while_in_flight_and_releases_afterwards() {
        let bus = Arc::new(EventBus::new(64));
        let registry: Arc<dyn TriggerTemplateRegistry> =
            Arc::new(StaticRegistry(vec![event_template(
                "t1",
                "NODE_COMPLETED",
                0,
            )]));
        // The action blocks until the test hands out a permit, so the second
        // duplicate is guaranteed to arrive while the first run is in flight.
        let hold = Arc::new(Semaphore::new(0));
        let runner = Arc::new(RecordingRunner::held(hold.clone()));
        let started = runner.started.clone();
        let calls = runner.calls.clone();
        start_listener(&bus, registry, runner.clone());
        wait_for_listener(&bus, 1).await;

        bus.publish(base_event(EventType::NodeCompleted, "exec-4"))
            .unwrap();
        wait_until(|| started.load(Ordering::SeqCst) == 1).await;
        bus.publish(base_event(EventType::NodeCompleted, "exec-4"))
            .unwrap();
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert_eq!(
            started.load(Ordering::SeqCst),
            1,
            "re-entrancy must be prevented while a run is in flight"
        );

        // Let the first run finish: the same (execution, trigger) pair must be
        // claimable again afterwards.
        hold.add_permits(1);
        wait_until(|| calls.load(Ordering::SeqCst) == 1).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
        bus.publish(base_event(EventType::NodeCompleted, "exec-4"))
            .unwrap();
        hold.add_permits(1);
        wait_until(|| calls.load(Ordering::SeqCst) == 2).await;
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn max_triggers_limits_per_template() {
        let bus = Arc::new(EventBus::new(64));
        let registry: Arc<dyn TriggerTemplateRegistry> =
            Arc::new(StaticRegistry(vec![event_template(
                "t1",
                "NODE_COMPLETED",
                1,
            )]));
        let runner = Arc::new(RecordingRunner::new());
        let calls = runner.calls.clone();
        start_listener(&bus, registry, runner.clone());
        wait_for_listener(&bus, 1).await;

        // Sequential (non-concurrent) events: the second event for the same
        // execution must be dropped by the per-execution max_triggers=1
        // budget.
        bus.publish(base_event(EventType::NodeCompleted, "exec-5"))
            .unwrap();
        wait_until(|| calls.load(Ordering::SeqCst) == 1).await;
        bus.publish(base_event(EventType::NodeCompleted, "exec-5"))
            .unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;

        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "max_triggers must cap runs"
        );
    }

    #[tokio::test]
    async fn max_triggers_budget_is_per_execution() {
        let bus = Arc::new(EventBus::new(64));
        let registry: Arc<dyn TriggerTemplateRegistry> =
            Arc::new(StaticRegistry(vec![event_template(
                "t1",
                "NODE_COMPLETED",
                1,
            )]));
        let runner = Arc::new(RecordingRunner::new());
        let calls = runner.calls.clone();
        start_listener(&bus, registry, runner.clone());
        wait_for_listener(&bus, 1).await;

        bus.publish(base_event(EventType::NodeCompleted, "exec-a"))
            .unwrap();
        wait_until(|| calls.load(Ordering::SeqCst) == 1).await;
        bus.publish(base_event(EventType::NodeCompleted, "exec-b"))
            .unwrap();
        wait_until(|| calls.load(Ordering::SeqCst) == 2).await;

        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "independent executions must not share the budget"
        );
    }

    #[tokio::test]
    async fn execution_less_creation_event_runs_runner() {
        let bus = Arc::new(EventBus::new(64));
        let registry: Arc<dyn TriggerTemplateRegistry> =
            Arc::new(StaticRegistry(vec![creation_template(
                "fresh", "nightly", 0,
            )]));
        let runner = Arc::new(RecordingRunner::new());
        let calls = runner.calls.clone();
        start_listener(&bus, registry, runner.clone());
        wait_for_listener(&bus, 1).await;

        bus.publish(execution_less_event("nightly", "nightly:7"))
            .unwrap();
        wait_until(|| calls.load(Ordering::SeqCst) == 1).await;
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn creation_budget_is_per_fire_id() {
        let bus = Arc::new(EventBus::new(64));
        let registry: Arc<dyn TriggerTemplateRegistry> =
            Arc::new(StaticRegistry(vec![creation_template(
                "fresh", "nightly", 1,
            )]));
        let runner = Arc::new(RecordingRunner::new());
        let calls = runner.calls.clone();
        start_listener(&bus, registry, runner.clone());
        wait_for_listener(&bus, 1).await;

        // Same fire replayed: the per-fire max_triggers=1 budget drops the
        // second delivery (the sleep lets the first run release its in-flight
        // slot, so only the budget can be responsible).
        bus.publish(execution_less_event("nightly", "nightly:8"))
            .unwrap();
        wait_until(|| calls.load(Ordering::SeqCst) == 1).await;
        tokio::time::sleep(Duration::from_millis(150)).await;
        bus.publish(execution_less_event("nightly", "nightly:8"))
            .unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "same fire_id must share the budget"
        );
        // A new fire gets a fresh budget.
        bus.publish(execution_less_event("nightly", "nightly:9"))
            .unwrap();
        wait_until(|| calls.load(Ordering::SeqCst) == 2).await;
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn execution_less_event_selects_only_the_cold_start_winner() {
        // The execution-scoped template never becomes a candidate for an
        // execution-less event, so the surviving cold-start template wins its
        // scope alone and carries the per-fire dispatch key.
        let listener = listener_for(vec![
            scoped_custom_template("scoped", "nightly"),
            creation_template("fresh", "nightly", 0),
        ]);
        let winners = listener.select_templates(&execution_less_event("nightly", "nightly:1"));
        assert_eq!(match_names(&winners), vec!["fresh"]);
        assert_eq!(winners[0].key, "nightly:1:fresh");

        // Two execution-scoped subscribers in the same unique scope collide:
        // the scope is dropped loudly instead of picking one.
        let listener = listener_for(vec![
            scoped_custom_template("scoped", "nightly"),
            scoped_custom_template("other", "nightly"),
        ]);
        assert!(listener
            .select_templates(&base_event(EventType::NodeCustomEvent, "exec-1"))
            .is_empty());
    }

    #[test]
    fn disjoint_prefix_scopes_each_keep_a_winner() {
        let mut a = event_template("a", "NODE_COMPLETED", 0);
        a.condition.as_mut().expect("condition").execution_prefix = Some("exec-a-".to_string());
        let mut b = event_template("b", "NODE_COMPLETED", 0);
        b.condition.as_mut().expect("condition").execution_prefix = Some("exec-b-".to_string());
        let listener = listener_for(vec![a, b]);
        let winners = listener.select_templates(&base_event(EventType::NodeCompleted, "exec-a-1"));
        assert_eq!(winners.len(), 1);
        assert_eq!(winners[0].template.name, "a");
    }

    #[test]
    fn best_win_scope_selects_highest_explicit_priority() {
        let mut low = event_template("low", "NODE_COMPLETED", 0);
        low.priority = Some(1);
        low.dispatch_mode = Some(TriggerDispatchMode::BestWin);
        let mut high = event_template("high", "NODE_COMPLETED", 0);
        high.priority = Some(5);
        high.dispatch_mode = Some(TriggerDispatchMode::BestWin);
        let listener = listener_for(vec![low, high]);
        let winners = listener.select_templates(&base_event(EventType::NodeCompleted, "e1"));
        assert_eq!(winners.len(), 1);
        assert_eq!(winners[0].template.name, "high");
    }
}
