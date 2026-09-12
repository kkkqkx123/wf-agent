//! Event-driven trigger listener.
//!
//! Subscribes to the runtime EventBus and reacts to events by matching them
//! against registered trigger templates.
//! Matching and scheduling are this module's only responsibility; the
//! business logic triggered by a match lives behind the
//! [`TriggerActionRunner`] trait (implemented by wf-runtime, e.g. the
//! context-compression chain runner).
//!
//! wf-workflow stays decoupled from wf-resource and from concrete trigger
//! business: templates are provided through the [`TriggerTemplateRegistry`]
//! trait, sub-workflow execution through the [`SubworkflowRunner`] trait and
//! the action itself through [`TriggerActionRunner`], all implemented by
//! wf-runtime during assembly.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use dashmap::DashMap;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};
use wf_common::gate::ConcurrencyGate;
use wf_core::error::EventError;
use wf_core::EventBus;
use wf_types::events::{BaseEvent, EventType};
use wf_types::trigger::{TriggerCondition, TriggerRuntimeLimits, TriggerTemplate};

use crate::error::WorkflowResult;

/// Lookup source for trigger templates. Implemented by wf-runtime over the
/// wf-resource registrar, where the predefined templates are registered.
pub trait TriggerTemplateRegistry: Send + Sync {
    /// All enabled trigger templates to match events against.
    fn templates(&self) -> Vec<TriggerTemplate>;
}

/// Executes a triggered sub-workflow and returns its final output.
/// Implemented by wf-runtime over the workflow coordinator. Used by
/// concrete [`TriggerActionRunner`] implementations (e.g. the compression
/// chain runner) that need to run a workflow from a trigger event.
#[async_trait]
pub trait SubworkflowRunner: Send + Sync {
    async fn run(
        &self,
        workflow_id: &str,
        input: serde_json::Value,
    ) -> WorkflowResult<serde_json::Value>;
}

/// Executes the action of a matched trigger template for one event.
///
/// Implemented by wf-runtime: `SubworkflowActionRunner` handles the
/// user-template sub-workflow action; other runners can be assembled for
/// other event/action combinations. The context-compression chain itself no
/// longer runs through the listener — the engine dispatches the
/// `CONTEXT_COMPRESSION_REQUESTED` signal to hook receivers registered on
/// the shared hook registry (see `wf-runtime`'s `CompressionService`).
#[async_trait]
pub trait TriggerActionRunner: Send + Sync {
    /// Run the action of `template` for `event`. Best-effort: the listener
    /// logs failures, never propagates them to the emitting execution.
    async fn run(&self, template: &TriggerTemplate, event: &BaseEvent) -> WorkflowResult<()>;
}

/// Write-back target of compressed message arrays: the workflow engine's
/// [`ExecutionContextRegistry`], mapping a workflow execution id to the
/// versioned write-back handle over its variable map. Agent conversations
/// are not registered here: the agent engine consumes the completed event
/// itself (session self-consumption).
pub use crate::execution_context::{ExecutionContextRegistry, WriteBackError};

/// A matched trigger template and its source event, ready for dispatch.
struct TriggerMatch {
    template: TriggerTemplate,
    event: BaseEvent,
    key: String,
}

/// Listens for events and executes matching trigger templates.
///
/// Matching and dispatch are separated: the event loop receives events and
/// spawns matching tasks, which send [`TriggerMatch`] values through an
/// internal channel. A dedicated dispatch loop consumes the channel and
/// executes each matched action, keeping the event loop responsive even
/// under heavy template matching load.
///
/// Matching is best-effort: failures are logged, never propagated to the
/// emitting execution. Events are handled concurrently: each matching
/// (execution, template) pair runs in its own task, so the in-flight guard
/// and `max_triggers` budget are meaningful even for events that arrive
/// back-to-back.
///
/// When several templates match one event, winners resolve per competition
/// scope: a unique scope contributes its single subscriber, a best-win scope
/// contributes its highest explicit priority. Scopes that fail validation
/// (several unique subscribers, missing or duplicate best-win priorities,
/// mixed dispatch modes) contribute no winner and are dropped loudly.
/// At most one scope can match one event instance, so more than one scope
/// winner for the same event also means validation was bypassed and is
/// dropped loudly. Idempotency (e.g. repeated compression requests for the
/// same array version) is the responsibility of the [`TriggerActionRunner`].
#[derive(Clone)]
pub struct TriggerEventListener {
    bus: Arc<EventBus>,
    registry: Arc<dyn TriggerTemplateRegistry>,
    runner: Arc<dyn TriggerActionRunner>,
    /// `execution_id:trigger_name` pairs with a run in flight.
    in_flight: DashMap<String, ()>,
    /// Per-execution fire counts keyed by `execution_id:template_name`
    /// (only consulted when `max_triggers > 0`), so concurrent executions
    /// never consume each other's budget.
    trigger_counts: Arc<std::sync::Mutex<HashMap<String, u32>>>,
    /// Event types with at least one registered template; the listener
    /// subscribes a typed channel per type. Empty when no template declares
    /// a parseable type (general-channel fallback).
    interested_types: Vec<EventType>,
    /// Optional concurrency gate bounding concurrent trigger-action
    /// execution. `None` keeps the previous unbounded behavior.
    concurrency_gate: Option<Arc<ConcurrencyGate>>,
    /// Runtime limits (concurrency + dispatch circuit breaker). Default is
    /// unbounded with no burst cap, preserving the previous behavior.
    /// Over-limit winners are dropped with a warning, never queued.
    runtime_limits: TriggerRuntimeLimits,
    shutdown: CancellationToken,
}

/// Event types that at least one registered template can match, deduplicated
/// in registration order. A template whose `event_type` does not parse into a
/// known [`EventType`] forces the general-channel fallback (empty result) so
/// misconfigured templates keep their previous delivery behavior.
fn collect_interested_types(registry: &Arc<dyn TriggerTemplateRegistry>) -> Vec<EventType> {
    let mut seen = HashSet::new();
    let mut types = Vec::new();
    for template in registry.templates() {
        let Some(condition) = &template.condition else {
            continue;
        };
        if !seen.insert(condition.event_type.clone()) {
            continue;
        }
        match condition.event_type.parse::<EventType>() {
            Ok(event_type) => types.push(event_type),
            Err(e) => {
                warn!(
                    "Trigger template event_type '{}' is not a known event type: {}; falling back to the general event channel",
                    condition.event_type, e
                );
                return Vec::new();
            }
        }
    }
    types
}

impl TriggerEventListener {
    pub fn new(
        bus: Arc<EventBus>,
        registry: Arc<dyn TriggerTemplateRegistry>,
        runner: Arc<dyn TriggerActionRunner>,
        shutdown: CancellationToken,
    ) -> Self {
        let interested_types = collect_interested_types(&registry);
        Self {
            bus,
            registry,
            runner,
            in_flight: DashMap::new(),
            trigger_counts: Arc::new(std::sync::Mutex::new(HashMap::new())),
            interested_types,
            concurrency_gate: None,
            runtime_limits: TriggerRuntimeLimits::default(),
            shutdown,
        }
    }

    /// Bound concurrent trigger-action execution with a shared gate. When
    /// `None` (default), actions spawn unbounded as before.
    pub fn with_concurrency_gate(mut self, gate: Arc<ConcurrencyGate>) -> Self {
        self.concurrency_gate = Some(gate);
        self
    }

    /// Apply shared runtime limits. `max_concurrent_actions` installs a
    /// concurrency gate when none was set explicitly; the dispatch burst
    /// cap is enforced per event (warn-and-drop, never queued). Absent
    /// values keep the previous unbounded behavior.
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
        let (event_tx, mut event_rx) = mpsc::unbounded_channel::<BaseEvent>();
        if self.interested_types.is_empty() {
            let tx = event_tx.clone();
            let mut subscription = self.bus.subscribe();
            tokio::spawn(async move {
                loop {
                    match subscription.recv().await {
                        Ok(event) => {
                            if tx.send(event).is_err() {
                                break;
                            }
                        }
                        Err(EventError::Lagged(_)) => {
                            warn!("TriggerEventListener lagged behind the event bus");
                            continue;
                        }
                        Err(_) => break,
                    }
                }
            });
        } else {
            for event_type in &self.interested_types {
                let tx = event_tx.clone();
                let event_type = event_type.clone();
                let mut subscription = self.bus.subscribe_typed(event_type.clone());
                tokio::spawn(async move {
                    loop {
                        match subscription.recv().await {
                            Ok(event) => {
                                if tx.send(event).is_err() {
                                    break;
                                }
                            }
                            Err(EventError::Lagged(_)) => {
                                match event_type.category() {
                                    wf_types::events::EventCategory::Observable => {
                                        debug!(
                                            "TriggerEventListener lagged behind observable event type {}",
                                            event_type.as_str()
                                        );
                                    }
                                    _ => {
                                        warn!(
                                            "TriggerEventListener lagged behind request/mutated event type {}",
                                            event_type.as_str()
                                        );
                                    }
                                }
                                continue;
                            }
                            Err(_) => break,
                        }
                    }
                });
            }
        }

        // Main event loop: receive events, spawn matching in background tasks.
        loop {
            tokio::select! {
                _ = self.shutdown.cancelled() => {
                    debug!("TriggerEventListener shutdown requested");
                    break;
                }
                event = event_rx.recv() => match event {
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

    /// Match an event against all registered templates and return the single
    /// winner. Matches resolve per competition scope: a valid unique scope
    /// contributes its single subscriber, a valid best-win scope contributes
    /// its highest explicit priority. Invalid scopes contribute nothing.
    /// At most one scope winner is returned; several scope winners for one
    /// event mean validation was bypassed and yield no winner.
    fn select_templates(&self, event: &BaseEvent) -> Vec<TriggerMatch> {
        let Some(execution_id) = event.execution_id.as_ref() else {
            return Vec::new();
        };

        let mut matched: Vec<(usize, TriggerTemplate)> = Vec::new();
        for (index, template) in self.registry.templates().iter().enumerate() {
            if !template.enabled.unwrap_or(true) {
                continue;
            }
            let Some(condition) = &template.condition else {
                continue;
            };
            // Defense in depth: templates that slipped past validation must
            // not drive functional actions off the internal compression
            // signal or its audit copy (owned synchronously by the builtin
            // compression service).
            if condition.targets_compression_signal() {
                warn!(
                    "Trigger '{}' targets the internal compression signal; skipping (register a hook handler instead)",
                    template.name
                );
                continue;
            }
            if !self.matches(event, condition) {
                continue;
            }
            if template.action.is_none() {
                continue;
            }
            matched.push((index, template.clone()));
        }

        let mut winners: Vec<(usize, TriggerTemplate)> = Vec::new();
        if !matched.is_empty() {
            let snapshots: Vec<TriggerTemplate> = matched.iter().map(|(_, t)| t.clone()).collect();
            for group in wf_types::trigger::scope_groups(&snapshots) {
                let members: Vec<(usize, TriggerTemplate)> =
                    group.iter().map(|&i| matched[i].clone()).collect();
                let refs: Vec<&TriggerTemplate> = members.iter().map(|(_, t)| t).collect();
                match wf_types::trigger::resolve_scope_mode(&refs) {
                    Err((unique, best_win)) => {
                        let unique_names: Vec<&str> =
                            unique.iter().map(|t| t.name.as_str()).collect();
                        let best_win_names: Vec<&str> =
                            best_win.iter().map(|t| t.name.as_str()).collect();
                        warn!(
                            "Trigger scope for event {} mixes unique declarations [{}] with best-win declarations [{}]; validation was bypassed, dropping the scope",
                            event.r#type.as_str(),
                            unique_names.join(", "),
                            best_win_names.join(", "),
                        );
                    }
                    Ok(wf_types::trigger::ResolvedScopeMode::Unique) => {
                        if members.len() > 1 {
                            let names: Vec<&str> =
                                members.iter().map(|(_, t)| t.name.as_str()).collect();
                            warn!(
                                "Several trigger templates [{}] matched {} under unique dispatch; validation was bypassed, dropping the scope",
                                names.join(", "),
                                event.r#type.as_str(),
                            );
                        } else if let Some(winner) = members.into_iter().next() {
                            winners.push(winner);
                        }
                    }
                    Ok(wf_types::trigger::ResolvedScopeMode::BestWin) => {
                        // Best-win scopes require an explicit distinct
                        // priority per subscriber. Anything else means
                        // validation was bypassed: drop the scope without
                        // falling back to implicit ordering.
                        let mut missing: Vec<&str> = Vec::new();
                        for (_, template) in &members {
                            if template.priority.is_none() {
                                missing.push(template.name.as_str());
                            }
                        }
                        if !missing.is_empty() {
                            warn!(
                                "Trigger templates [{}] matched {} under best-win dispatch without explicit priorities; validation was bypassed, dropping the scope",
                                missing.join(", "),
                                event.r#type.as_str(),
                            );
                            continue;
                        }
                        let mut by_priority: std::collections::HashMap<i32, Vec<&str>> =
                            std::collections::HashMap::new();
                        for (_, template) in &members {
                            let priority = template.priority.unwrap_or_default();
                            by_priority
                                .entry(priority)
                                .or_default()
                                .push(template.name.as_str());
                        }
                        if let Some(duplicated) =
                            by_priority.iter().find(|(_, names)| names.len() > 1)
                        {
                            warn!(
                                "Trigger templates [{}] share priority {} on event {}; validation was bypassed, dropping the scope",
                                duplicated.1.join(", "),
                                duplicated.0,
                                event.r#type.as_str(),
                            );
                            continue;
                        }
                        let winner = members
                            .into_iter()
                            .max_by_key(|(_, template)| template.priority.unwrap_or_default());
                        if let Some(winner) = winner {
                            winners.push(winner);
                        }
                    }
                }
            }
        }

        if winners.len() > 1 {
            if self.runtime_limits.dispatch_burst_limit.is_some() {
                let mut winners = winners;
                let dropped = self.runtime_limits.apply_burst_cap(&mut winners);
                let names: Vec<&str> = winners.iter().map(|(_, t)| t.name.as_str()).collect();
                warn!(
                    "Several trigger scopes [{}] matched {} for one event; dispatch burst cap dropped {} winner(s), executing [{}]",
                    names.join(", "),
                    event.r#type.as_str(),
                    dropped,
                    names.join(", "),
                );
                return self.finalize_winners(winners, event, execution_id);
            }
            let names: Vec<&str> = winners.iter().map(|(_, t)| t.name.as_str()).collect();
            warn!(
                "Several trigger scopes [{}] matched {} for one event; validation was bypassed, dropping every winner",
                names.join(", "),
                event.r#type.as_str(),
            );
            return Vec::new();
        }

        // Debug-log only events that have templates configured for their type.
        if winners.is_empty() {
            let type_configured = self.registry.templates().iter().any(|t| {
                t.condition
                    .as_ref()
                    .is_some_and(|c| c.event_type == event.r#type.as_str())
            });
            if type_configured {
                debug!(
                    "No trigger template matched event {} for execution {}",
                    event.r#type.as_str(),
                    execution_id
                );
            }
            return Vec::new();
        }

        winners
            .into_iter()
            .map(|(_, template)| {
                if template.allow_multi_effect == Some(true) {
                    warn!(
                        "Trigger '{}' declares multi-effect execution with an explicit order, but the runtime still executes the single winner; ordered multi-execution is validated, not yet executed",
                        template.name,
                    );
                }
                let key = format!("{}:{}", execution_id, template.name);
                TriggerMatch {
                    template,
                    event: event.clone(),
                    key,
                }
            })
            .collect()
    }

    /// Map burst-capped winners to dispatchable matches (shared with the
    /// default path; multi-effect declarations stay single-execution).
    fn finalize_winners(
        &self,
        winners: Vec<(usize, TriggerTemplate)>,
        event: &BaseEvent,
        execution_id: &str,
    ) -> Vec<TriggerMatch> {
        winners
            .into_iter()
            .map(|(_, template)| {
                if template.allow_multi_effect == Some(true) {
                    warn!(
                        "Trigger '{}' declares multi-effect execution with an explicit order, but the runtime still executes the single winner; ordered multi-execution is validated, not yet executed",
                        template.name,
                    );
                }
                let key = format!("{}:{}", execution_id, template.name);
                TriggerMatch {
                    template,
                    event: event.clone(),
                    key,
                }
            })
            .collect()
    }

    /// Execute a matched trigger action: check in-flight guard and
    /// per-execution max_triggers budget, then spawn the action runner.
    async fn execute_action(&self, matched: TriggerMatch) {
        let TriggerMatch {
            template,
            event,
            key,
        } = matched;

        let Some(execution_id) = event.execution_id.clone() else {
            return;
        };

        // Atomic re-entrancy guard: a present entry means a run for this
        // (execution, trigger) pair is already in flight.
        if self.in_flight.insert(key.clone(), ()).is_some() {
            debug!(
                "Trigger '{}' already running for execution {}, skipping",
                template.name, execution_id
            );
            return;
        }
        if let Some(max) = template.max_triggers {
            if max > 0 {
                let budget_key = format!("{}:{}", execution_id, template.name);
                let mut counts = wf_common::lock::lock_ok(self.trigger_counts.lock());
                let count = counts.entry(budget_key).or_insert(0);
                if *count >= max {
                    debug!(
                        "Trigger '{}' reached max_triggers ({}) for execution {}, skipping",
                        template.name, max, execution_id
                    );
                    self.in_flight.remove(&key);
                    return;
                }
                *count += 1;
            }
        }

        let listener = self.clone();
        let shutdown = self.shutdown.clone();
        let template = template.clone();
        let event = event.clone();
        let gate = self.concurrency_gate.clone();
        tokio::spawn(async move {
            // Concurrency gate: wait for a permit before running (queued
            // triggers hold their in-flight slot). No gate keeps the
            // previous unbounded behavior.
            let _permit = match gate {
                Some(gate) => match gate.acquire_wait().await {
                    Ok(permit) => Some(permit),
                    Err(e) => {
                        warn!(
                            "Trigger '{}' rejected by concurrency gate: {}",
                            template.name, e
                        );
                        listener.in_flight.remove(&key);
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
            listener.in_flight.remove(&key);
        });
    }

    /// Whether a condition carries metadata routing (metadata map, existence
    /// list, or execution prefix). More specific templates win ties.
    fn matches(&self, event: &BaseEvent, condition: &TriggerCondition) -> bool {
        if event.r#type.as_str() != condition.event_type {
            return false;
        }

        // Secondary discriminator: `event_name` must equal the event's own
        // event name when configured.
        if let Some(expected) = &condition.event_name {
            if event.event_name.as_deref() != Some(expected.as_str()) {
                return false;
            }
        }

        if let Some(prefix) = &condition.execution_prefix {
            let hit = event
                .execution_id
                .as_deref()
                .is_some_and(|id| id.starts_with(prefix))
                || event
                    .agent_loop_id
                    .as_deref()
                    .is_some_and(|id| id.starts_with(prefix));
            if !hit {
                return false;
            }
        }

        let Some(event_metadata) = &event.metadata else {
            return condition.metadata.is_none()
                && condition.metadata_exists.is_none()
                && condition.condition.is_none();
        };

        if let Some(required) = &condition.metadata_exists {
            if !required.iter().all(|key| event_metadata.contains_key(key)) {
                return false;
            }
        }

        if let Some(condition_metadata) = &condition.metadata {
            if !condition_metadata
                .iter()
                .all(|(key, expected)| match event_metadata.get(key) {
                    Some(actual) => value_matches(actual, expected),
                    None => false,
                })
            {
                return false;
            }
        }

        // Expression condition: evaluated against the event fields plus its
        // metadata; an evaluation error
        // is a non-match, never a failure of the listener loop.
        if let Some(expression) = &condition.condition {
            return Self::evaluate_condition(expression, event);
        }
        true
    }

    /// Evaluate a trigger condition expression against an event.
    ///
    /// The evaluation context mirrors the event: `type` / `event_name` /
    /// `timestamp` / `execution_id` / `agent_loop_id` plus every metadata key
    /// (so `eq(status, "completed")` works without a `metadata.` prefix).
    fn evaluate_condition(expression: &str, event: &BaseEvent) -> bool {
        let mut context: HashMap<String, serde_json::Value> = HashMap::new();
        context.insert(
            "type".to_string(),
            serde_json::Value::String(event.r#type.as_str().to_string()),
        );
        context.insert(
            "event_name".to_string(),
            event
                .event_name
                .as_ref()
                .cloned()
                .map(serde_json::Value::String)
                .unwrap_or(serde_json::Value::Null),
        );
        context.insert("timestamp".to_string(), serde_json::json!(event.timestamp));
        for (key, value) in [
            ("workflow_id", event.workflow_id.as_ref()),
            ("execution_id", event.execution_id.as_ref()),
            ("agent_loop_id", event.agent_loop_id.as_ref()),
        ] {
            context.insert(
                key.to_string(),
                value
                    .map(|v| serde_json::Value::String(v.clone()))
                    .unwrap_or(serde_json::Value::Null),
            );
        }
        if let Some(metadata) = &event.metadata {
            for (key, value) in metadata {
                context.insert(key.clone(), value.clone());
            }
        }
        match wf_core::condition::ConditionEvaluator::evaluate(expression, &context) {
            Ok(result) => result,
            Err(e) => {
                debug!(
                    "Trigger condition expression '{}' evaluation failed: {}",
                    expression, e
                );
                false
            }
        }
    }
}

/// Compare an actual metadata value against an expected one.
///
/// Exact equality for non-string expected values. String expected values
/// support three conventions (backward compatible):
/// - `">=N"`, `"<=N"`, `">N"`, `"<N"`: numeric comparison against the event
///   value (JSON numbers);
/// - `"^prefix"`: the event string value starts with `prefix`;
/// - anything else: exact string equality.
///
/// Array actual values (notably the `HOOK_TRIGGERED` audit event's
/// `hook_type` list) match when any element matches the expected value, so a
/// trigger can subscribe to one hook type with a plain string condition.
fn value_matches(actual: &serde_json::Value, expected: &serde_json::Value) -> bool {
    if let Some(items) = actual.as_array() {
        return items.iter().any(|item| value_matches(item, expected));
    }
    let Some(s) = expected.as_str() else {
        return actual == expected;
    };
    if let Some(rest) = s.strip_prefix(">=") {
        return parse_number(rest).is_some_and(|n| actual.as_f64().is_some_and(|a| a >= n));
    }
    if let Some(rest) = s.strip_prefix("<=") {
        return parse_number(rest).is_some_and(|n| actual.as_f64().is_some_and(|a| a <= n));
    }
    if let Some(rest) = s.strip_prefix('>') {
        return parse_number(rest).is_some_and(|n| actual.as_f64().is_some_and(|a| a > n));
    }
    if let Some(rest) = s.strip_prefix('<') {
        return parse_number(rest).is_some_and(|n| actual.as_f64().is_some_and(|a| a < n));
    }
    if let Some(prefix) = s.strip_prefix('^') {
        return actual.as_str().is_some_and(|a| a.starts_with(prefix));
    }
    actual == expected
}

fn parse_number(s: &str) -> Option<f64> {
    s.parse::<f64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    use crate::error::WorkflowError;
    use wf_types::events::EventType;

    struct RecordingRunner {
        calls: Arc<AtomicU32>,
        abort_on_event_type: Option<String>,
    }

    #[async_trait]
    impl TriggerActionRunner for RecordingRunner {
        async fn run(&self, _template: &TriggerTemplate, event: &BaseEvent) -> WorkflowResult<()> {
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

    fn event_template(name: &str, event_type: &str, max_triggers: u32) -> TriggerTemplate {
        TriggerTemplate {
            name: name.to_string(),
            description: None,
            condition: Some(TriggerCondition {
                event_type: event_type.to_string(),
                event_name: None,
                condition: None,
                metadata: None,
                metadata_exists: None,
                execution_prefix: None,
            }),
            action: Some(
                serde_json::from_value(serde_json::json!({
                    "action_type": "execute_triggered_subworkflow",
                    "triggered_workflow_id": "summary_flow",
                }))
                .unwrap(),
            ),
            enabled: Some(true),
            max_triggers: Some(max_triggers),
            priority: None,
            dispatch_mode: None,
            allow_multi_effect: None,
            effect_order: None,
            metadata: None,
            created_at: 0,
            updated_at: 0,
            create_checkpoint: None,
            checkpoint_description_template: None,
        }
    }

    struct StaticRegistry(Vec<TriggerTemplate>);

    impl TriggerTemplateRegistry for StaticRegistry {
        fn templates(&self) -> Vec<TriggerTemplate> {
            self.0.clone()
        }
    }

    fn base_event(event_type: EventType, execution_id: &str) -> BaseEvent {
        BaseEvent {
            id: wf_types::Id::new(),
            r#type: event_type,
            timestamp: wf_common::now(),
            workflow_id: None,
            execution_id: Some(execution_id.to_string()),
            agent_loop_id: None,

            event_name: None,
            metadata: None,
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

    #[tokio::test]
    async fn matching_event_runs_runner_once() {
        let bus = Arc::new(EventBus::new(64));
        let registry: Arc<dyn TriggerTemplateRegistry> =
            Arc::new(StaticRegistry(vec![event_template(
                "t1",
                "NODE_COMPLETED",
                0,
            )]));
        let calls = Arc::new(AtomicU32::new(0));
        let runner: Arc<dyn TriggerActionRunner> = Arc::new(RecordingRunner {
            calls: calls.clone(),
            abort_on_event_type: None,
        });
        start_listener(&bus, registry, runner);
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
        let calls = Arc::new(AtomicU32::new(0));
        let runner: Arc<dyn TriggerActionRunner> = Arc::new(RecordingRunner {
            calls: calls.clone(),
            abort_on_event_type: None,
        });
        start_listener(&bus, registry, runner);
        wait_for_listener(&bus, 1).await;

        bus.publish(base_event(EventType::TokenLimitExceeded, "exec-2"))
            .unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(calls.load(Ordering::SeqCst), 0, "no match -> no run");
    }

    #[tokio::test]
    async fn template_without_action_skips_runner() {
        let bus = Arc::new(EventBus::new(64));
        let mut template = event_template("t1", "NODE_COMPLETED", 0);
        template.action = None;
        let registry: Arc<dyn TriggerTemplateRegistry> = Arc::new(StaticRegistry(vec![template]));
        let calls = Arc::new(AtomicU32::new(0));
        let runner: Arc<dyn TriggerActionRunner> = Arc::new(RecordingRunner {
            calls: calls.clone(),
            abort_on_event_type: None,
        });
        start_listener(&bus, registry, runner);
        wait_for_listener(&bus, 1).await;

        bus.publish(base_event(EventType::NodeCompleted, "exec-3"))
            .unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(calls.load(Ordering::SeqCst), 0, "no action -> no run");
    }

    #[tokio::test]
    async fn duplicate_event_is_skipped_while_in_flight() {
        let bus = Arc::new(EventBus::new(64));
        let registry: Arc<dyn TriggerTemplateRegistry> =
            Arc::new(StaticRegistry(vec![event_template(
                "t1",
                "NODE_COMPLETED",
                0,
            )]));
        let calls = Arc::new(AtomicU32::new(0));
        let runner: Arc<dyn TriggerActionRunner> = Arc::new(RecordingRunner {
            calls: calls.clone(),
            abort_on_event_type: None,
        });
        start_listener(&bus, registry, runner);
        wait_for_listener(&bus, 1).await;

        // Two identical events for the same execution: the in-flight claim is
        // taken synchronously during dispatch, so the second must be skipped
        // even though the first finishes quickly.
        bus.publish(base_event(EventType::NodeCompleted, "exec-4"))
            .unwrap();
        bus.publish(base_event(EventType::NodeCompleted, "exec-4"))
            .unwrap();

        // Wait for the first run to finish, then give a would-be second run
        // (which must not happen) room to surface.
        wait_until(|| calls.load(Ordering::SeqCst) == 1).await;
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "re-entrancy must be prevented"
        );
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
        let calls = Arc::new(AtomicU32::new(0));
        let runner: Arc<dyn TriggerActionRunner> = Arc::new(RecordingRunner {
            calls: calls.clone(),
            abort_on_event_type: None,
        });
        start_listener(&bus, registry, runner);
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
        let calls = Arc::new(AtomicU32::new(0));
        let runner: Arc<dyn TriggerActionRunner> = Arc::new(RecordingRunner {
            calls: calls.clone(),
            abort_on_event_type: None,
        });
        start_listener(&bus, registry, runner);
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

    #[test]
    fn best_win_highest_explicit_priority_wins() {
        use wf_types::trigger::TriggerDispatchMode;
        let mut low = event_template("low", "NODE_COMPLETED", 0);
        low.priority = Some(1);
        low.dispatch_mode = Some(TriggerDispatchMode::BestWin);
        let mut high = event_template("high", "NODE_COMPLETED", 0);
        high.priority = Some(5);
        high.dispatch_mode = Some(TriggerDispatchMode::BestWin);
        let templates = vec![low, high];
        let listener = TriggerEventListener::new(
            Arc::new(EventBus::new(4)),
            Arc::new(StaticRegistry(templates)),
            Arc::new(RecordingRunner {
                calls: Arc::new(AtomicU32::new(0)),
                abort_on_event_type: None,
            }),
            CancellationToken::new(),
        );
        let event = base_event(EventType::NodeCompleted, "e1");
        let winners = listener.select_templates(&event);
        assert_eq!(winners.len(), 1);
        assert_eq!(winners[0].template.name, "high");
    }

    #[test]
    fn best_win_without_explicit_priority_is_dropped() {
        use wf_types::trigger::TriggerDispatchMode;
        let mut low = event_template("low", "NODE_COMPLETED", 0);
        low.priority = None;
        low.dispatch_mode = Some(TriggerDispatchMode::BestWin);
        let mut high = event_template("high", "NODE_COMPLETED", 0);
        high.priority = Some(5);
        high.dispatch_mode = Some(TriggerDispatchMode::BestWin);
        let listener = TriggerEventListener::new(
            Arc::new(EventBus::new(4)),
            Arc::new(StaticRegistry(vec![low, high])),
            Arc::new(RecordingRunner {
                calls: Arc::new(AtomicU32::new(0)),
                abort_on_event_type: None,
            }),
            CancellationToken::new(),
        );
        let event = base_event(EventType::NodeCompleted, "e1");
        assert!(listener.select_templates(&event).is_empty());
    }

    #[test]
    fn best_win_with_duplicate_priority_is_dropped() {
        use wf_types::trigger::TriggerDispatchMode;
        let mut low = event_template("low", "NODE_COMPLETED", 0);
        low.priority = Some(5);
        low.dispatch_mode = Some(TriggerDispatchMode::BestWin);
        let mut high = event_template("high", "NODE_COMPLETED", 0);
        high.priority = Some(5);
        high.dispatch_mode = Some(TriggerDispatchMode::BestWin);
        let listener = TriggerEventListener::new(
            Arc::new(EventBus::new(4)),
            Arc::new(StaticRegistry(vec![low, high])),
            Arc::new(RecordingRunner {
                calls: Arc::new(AtomicU32::new(0)),
                abort_on_event_type: None,
            }),
            CancellationToken::new(),
        );
        let event = base_event(EventType::NodeCompleted, "e1");
        assert!(listener.select_templates(&event).is_empty());
    }

    #[test]
    fn unique_scope_with_several_matches_is_dropped() {
        let templates = vec![
            event_template("a", "NODE_COMPLETED", 0),
            event_template("b", "NODE_COMPLETED", 0),
        ];
        let listener = TriggerEventListener::new(
            Arc::new(EventBus::new(4)),
            Arc::new(StaticRegistry(templates)),
            Arc::new(RecordingRunner {
                calls: Arc::new(AtomicU32::new(0)),
                abort_on_event_type: None,
            }),
            CancellationToken::new(),
        );
        let event = base_event(EventType::NodeCompleted, "e1");
        assert!(listener.select_templates(&event).is_empty());
    }

    #[test]
    fn disjoint_prefix_scopes_each_keep_a_winner() {
        let mut a = event_template("a", "NODE_COMPLETED", 0);
        a.condition.as_mut().expect("condition").execution_prefix = Some("exec-a-".to_string());
        let mut b = event_template("b", "NODE_COMPLETED", 0);
        b.condition.as_mut().expect("condition").execution_prefix = Some("exec-b-".to_string());
        let listener = TriggerEventListener::new(
            Arc::new(EventBus::new(4)),
            Arc::new(StaticRegistry(vec![a, b])),
            Arc::new(RecordingRunner {
                calls: Arc::new(AtomicU32::new(0)),
                abort_on_event_type: None,
            }),
            CancellationToken::new(),
        );
        let winners = listener.select_templates(&base_event(EventType::NodeCompleted, "exec-a-1"));
        assert_eq!(winners.len(), 1);
        assert_eq!(winners[0].template.name, "a");
    }

    #[test]
    fn event_type_serialization_matches_template_condition() {
        let event = base_event(EventType::ContextCompressionRequested, "x");
        let condition = TriggerCondition {
            event_type: "CONTEXT_COMPRESSION_REQUESTED".to_string(),
            event_name: None,
            condition: None,
            metadata: None,
            metadata_exists: None,
            execution_prefix: None,
        };
        let listener = TriggerEventListener::new(
            Arc::new(EventBus::new(4)),
            Arc::new(StaticRegistry(Vec::new())),
            Arc::new(RecordingRunner {
                calls: Arc::new(AtomicU32::new(0)),
                abort_on_event_type: None,
            }),
            CancellationToken::new(),
        );
        assert!(listener.matches(&event, &condition));
        assert!(!listener.matches(
            &BaseEvent {
                r#type: EventType::TokenLimitExceeded,
                ..event
            },
            &condition
        ));
    }

    #[test]
    fn event_name_secondary_discriminator_matches() {
        let listener = TriggerEventListener::new(
            Arc::new(EventBus::new(4)),
            Arc::new(StaticRegistry(Vec::new())),
            Arc::new(RecordingRunner {
                calls: Arc::new(AtomicU32::new(0)),
                abort_on_event_type: None,
            }),
            CancellationToken::new(),
        );
        let mut condition = TriggerCondition {
            event_type: "NODE_CUSTOM_EVENT".to_string(),
            event_name: Some("on_issue_created".to_string()),
            condition: None,
            metadata: None,
            metadata_exists: None,
            execution_prefix: None,
        };
        let event = BaseEvent {
            event_name: Some("on_issue_created".to_string()),
            ..base_event(EventType::NodeCustomEvent, "e1")
        };
        assert!(listener.matches(&event, &condition));
        // Wrong name does not match.
        let other = BaseEvent {
            event_name: Some("on_issue_updated".to_string()),
            ..base_event(EventType::NodeCustomEvent, "e1")
        };
        assert!(!listener.matches(&other, &condition));
        // Condition without event_name matches any event name.
        condition.event_name = None;
        assert!(listener.matches(&other, &condition));
    }

    #[test]
    fn condition_expression_matches_against_event_fields_and_metadata() {
        let listener = TriggerEventListener::new(
            Arc::new(EventBus::new(4)),
            Arc::new(StaticRegistry(Vec::new())),
            Arc::new(RecordingRunner {
                calls: Arc::new(AtomicU32::new(0)),
                abort_on_event_type: None,
            }),
            CancellationToken::new(),
        );
        let mut event = base_event(EventType::NodeCustomEvent, "e1");
        event.metadata = Some(std::collections::HashMap::from([(
            "status".to_string(),
            serde_json::json!("completed"),
        )]));
        let condition = TriggerCondition {
            event_type: "NODE_CUSTOM_EVENT".to_string(),
            event_name: None,
            condition: Some(r#"eq(status, "completed")"#.to_string()),
            metadata: None,
            metadata_exists: None,
            execution_prefix: None,
        };
        assert!(
            listener.matches(&event, &condition),
            "expression over metadata must match"
        );
        let failing = TriggerCondition {
            condition: Some(r#"eq(status, "failed")"#.to_string()),
            ..condition.clone()
        };
        assert!(!listener.matches(&event, &failing));
        // Evaluation error (unknown function / malformed) is a non-match.
        let malformed = TriggerCondition {
            condition: Some("bogus((".to_string()),
            ..condition
        };
        assert!(!listener.matches(&event, &malformed));
    }

    #[test]
    fn condition_expression_requires_metadata_presence() {
        let listener = TriggerEventListener::new(
            Arc::new(EventBus::new(4)),
            Arc::new(StaticRegistry(Vec::new())),
            Arc::new(RecordingRunner {
                calls: Arc::new(AtomicU32::new(0)),
                abort_on_event_type: None,
            }),
            CancellationToken::new(),
        );
        // No metadata on the event: the expression condition fails the match
        // (unlike the old behavior where a bare metadata-less event matched
        // any metadata-free condition).
        let condition = TriggerCondition {
            event_type: "NODE_CUSTOM_EVENT".to_string(),
            event_name: None,
            condition: Some(r#"eq(status, "completed")"#.to_string()),
            metadata: None,
            metadata_exists: None,
            execution_prefix: None,
        };
        let event = base_event(EventType::NodeCustomEvent, "e1");
        assert!(!listener.matches(&event, &condition));
    }

    #[test]
    fn hook_type_list_matches_plain_string_condition() {
        let listener = TriggerEventListener::new(
            Arc::new(EventBus::new(4)),
            Arc::new(StaticRegistry(Vec::new())),
            Arc::new(RecordingRunner {
                calls: Arc::new(AtomicU32::new(0)),
                abort_on_event_type: None,
            }),
            CancellationToken::new(),
        );
        let mut event = base_event(EventType::HookTriggered, "e1");
        event.metadata = Some(std::collections::HashMap::from([(
            "hook_type".to_string(),
            serde_json::json!(["BEFORE_TOOL_CALL"]),
        )]));
        let condition = TriggerCondition {
            event_type: "HOOK_TRIGGERED".to_string(),
            event_name: None,
            condition: None,
            metadata: Some(std::collections::HashMap::from([(
                "hook_type".to_string(),
                serde_json::json!("BEFORE_TOOL_CALL"),
            )])),
            metadata_exists: None,
            execution_prefix: None,
        };
        assert!(listener.matches(&event, &condition));
        let other = TriggerCondition {
            metadata: Some(std::collections::HashMap::from([(
                "hook_type".to_string(),
                serde_json::json!("AFTER_TOOL_CALL"),
            )])),
            ..condition
        };
        assert!(!listener.matches(&event, &other));
    }

    #[test]
    fn array_actual_matches_prefix_convention_per_element() {
        assert!(value_matches(
            &serde_json::json!(["agent-a", "agent-b"]),
            &serde_json::json!("^agent-")
        ));
        assert!(!value_matches(
            &serde_json::json!(["other"]),
            &serde_json::json!("^agent-")
        ));
    }

    #[test]
    fn compression_signal_templates_never_selected() {
        let direct = event_template(
            "on-compression",
            wf_types::hook::CONTEXT_COMPRESSION_SIGNAL,
            0,
        );
        let mut audit_copy = event_template("on-compression-audit", "HOOK_TRIGGERED", 0);
        audit_copy.condition.as_mut().expect("condition").metadata =
            Some(std::collections::HashMap::from([(
                "hook_type".to_string(),
                serde_json::json!(wf_types::hook::CONTEXT_COMPRESSION_SIGNAL),
            )]));
        let listener = TriggerEventListener::new(
            Arc::new(EventBus::new(4)),
            Arc::new(StaticRegistry(vec![direct, audit_copy])),
            Arc::new(RecordingRunner {
                calls: Arc::new(AtomicU32::new(0)),
                abort_on_event_type: None,
            }),
            CancellationToken::new(),
        );
        assert!(listener
            .select_templates(&base_event(EventType::ContextCompressionRequested, "e1"))
            .is_empty());
        let mut audit = base_event(EventType::HookTriggered, "e1");
        audit.metadata = Some(std::collections::HashMap::from([(
            "hook_type".to_string(),
            serde_json::json!([wf_types::hook::CONTEXT_COMPRESSION_SIGNAL]),
        )]));
        assert!(listener.select_templates(&audit).is_empty());
    }
}
