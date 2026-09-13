//! Cron scheduler producer for trigger templates.
//!
//! The scheduler owns time: it ticks `ScheduleSpec` templates registered in
//! the resource registries and publishes `NODE_CUSTOM_EVENT`s the listener
//! matches through `TriggerSource::translate_schedule_to_condition`. The
//! scheduler shares the listener shutdown token, so it lives and dies with
//! the trigger subsystem (`assemble_trigger_subsystem`).
//!
//! Execution routing splits the two trigger semantics:
//!
//! - execution-scoped ticks resolve their target through the
//!   [`TimerBindingRegistry`] (`schedule_name -> execution_id`), populated at
//!   runtime by whoever owns the timed execution; ticks without a binding are
//!   skipped (a static execution id in a template would be stale the moment
//!   the execution ends, and registration time never knows runtime ids);
//! - creation ticks publish events without an `execution_id`; only
//!   execution-creating actions (`TriggerAction::is_execution_creating`)
//!   match those, and the `fire_id` metadata keys their idempotency and
//!   quota.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::DateTime;
use dashmap::DashMap;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};
use wf_core::registry::Registry;
use wf_core::EventBus;
use wf_resource::registry::ResourceRegistries;
use wf_types::events::{BaseEvent, EventType};
use wf_types::trigger::{
    CronSchedule, ScheduleMisfirePolicy, ScheduleSpec, ScheduleTarget, FIRE_ID_METADATA_KEY,
    PRODUCER_SOURCE_METADATA_KEY, SCHEDULE_SPEC_METADATA_KEY,
};

/// Metadata key carrying the scheduler-provided run input for creation
/// targets (mirrored by the webhook gateway so runners share one contract).
pub const TRIGGER_INPUT_METADATA_KEY: &str = "trigger_input";

/// How often the scheduler re-reads templates even when no tick is near
/// (picks up newly registered schedules without a restart).
const TEMPLATE_REFRESH_INTERVAL: Duration = Duration::from_secs(60);
/// Upper bound on catch-up fires for one template under `FireAll`.
const MAX_CATCH_UP_FIRES: u32 = 10;

/// Runtime mount table of execution-scoped timers: `schedule_name` bound to
/// the live execution the tick should name. Whoever owns a timed execution
/// binds at start and unbinds at end; the scheduler joins the schedule
/// definition with this table at fire time.
#[derive(Debug, Default)]
pub struct TimerBindingRegistry {
    bindings: DashMap<String, String>,
}

impl TimerBindingRegistry {
    pub fn new() -> Self {
        Self {
            bindings: DashMap::new(),
        }
    }

    /// Bind a schedule to a live execution (replaces any previous binding).
    pub fn bind(&self, schedule_name: &str, execution_id: &str) {
        self.bindings
            .insert(schedule_name.to_string(), execution_id.to_string());
    }

    /// Remove the binding (idempotent; call when the execution ends).
    pub fn unbind(&self, schedule_name: &str) {
        self.bindings.remove(schedule_name);
    }

    /// Bound execution of a schedule, if any.
    pub fn bound_execution(&self, schedule_name: &str) -> Option<String> {
        self.bindings.get(schedule_name).map(|id| id.clone())
    }
}

/// Durable per-schedule fire cursor (`last_fire` unix seconds). The default
/// is process-local memory; a shared-storage implementation keeps multi-copy
/// runtimes from double-firing and survives restarts without replaying old
/// ticks.
#[async_trait]
pub trait ScheduleStateStore: Send + Sync {
    async fn last_fire(&self, schedule_name: &str) -> Option<i64>;
    async fn set_last_fire(&self, schedule_name: &str, timestamp: i64);
}

/// Process-local schedule cursors.
#[derive(Debug, Default)]
pub struct MemoryScheduleStateStore {
    cursors: tokio::sync::Mutex<HashMap<String, i64>>,
}

impl MemoryScheduleStateStore {
    pub fn new() -> Self {
        Self {
            cursors: tokio::sync::Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl ScheduleStateStore for MemoryScheduleStateStore {
    async fn last_fire(&self, schedule_name: &str) -> Option<i64> {
        self.cursors.lock().await.get(schedule_name).copied()
    }

    async fn set_last_fire(&self, schedule_name: &str, timestamp: i64) {
        self.cursors
            .lock()
            .await
            .insert(schedule_name.to_string(), timestamp);
    }
}

/// Dependencies of the scheduler background task.
pub struct SchedulerDeps {
    pub event_bus: Arc<EventBus>,
    pub registries: Arc<ResourceRegistries>,
    pub bindings: Arc<TimerBindingRegistry>,
    pub state_store: Arc<dyn ScheduleStateStore>,
    pub shutdown: CancellationToken,
}

struct ScheduleEntry {
    name: String,
    spec: ScheduleSpec,
    parsed: CronSchedule,
}

/// Snapshot every enabled schedule template with a valid spec.
fn snapshot_schedules(registries: &Arc<ResourceRegistries>) -> Vec<ScheduleEntry> {
    let mut entries = Vec::new();
    for key in registries.trigger_templates.list() {
        let Some(template) = registries.trigger_templates.get(&key) else {
            continue;
        };
        if !template.enabled.unwrap_or(true) {
            continue;
        }
        let Some(metadata) = &template.metadata else {
            continue;
        };
        let Some(spec_value) = metadata.get(SCHEDULE_SPEC_METADATA_KEY) else {
            continue;
        };
        let spec: ScheduleSpec = match serde_json::from_value(spec_value.clone()) {
            Ok(spec) => spec,
            Err(e) => {
                warn!(
                    "Schedule '{}' has an unreadable schedule_spec, skipping: {}",
                    template.name, e
                );
                continue;
            }
        };
        if !spec.enabled {
            continue;
        }
        match spec.validate(&template.name) {
            Ok(parsed) => entries.push(ScheduleEntry {
                name: template.name.clone(),
                spec,
                parsed,
            }),
            Err(e) => warn!(
                "Schedule '{}' failed validation at tick time: {}",
                template.name, e
            ),
        }
    }
    entries
}

/// Publish one tick as a `NODE_CUSTOM_EVENT`. Execution-scoped ticks name the
/// bound execution; creation ticks carry no `execution_id` and stamp
/// `fire_id` plus the target input for the creation runners.
fn publish_tick(
    bus: &Arc<EventBus>,
    entry: &ScheduleEntry,
    fire_at: i64,
    execution_id: Option<String>,
) {
    let fire_id = format!("{}:{}", entry.name, fire_at);
    let mut metadata: HashMap<String, serde_json::Value> = HashMap::from([
        (
            PRODUCER_SOURCE_METADATA_KEY.to_string(),
            serde_json::json!("schedule"),
        ),
        ("fired_at".to_string(), serde_json::json!(fire_at)),
        ("cron".to_string(), serde_json::json!(entry.spec.cron)),
        (FIRE_ID_METADATA_KEY.to_string(), serde_json::json!(fire_id)),
    ]);
    if let ScheduleTarget::Create { input, .. } = &entry.spec.target {
        if let Some(input) = input {
            metadata.insert(TRIGGER_INPUT_METADATA_KEY.to_string(), input.clone());
        }
    }
    let event = BaseEvent {
        id: wf_common::generate_id(),
        r#type: EventType::NodeCustomEvent,
        timestamp: wf_common::now(),
        workflow_id: None,
        execution_id: execution_id.map(wf_types::Id::from),
        agent_loop_id: None,
        event_name: Some(entry.name.clone()),
        metadata: Some(metadata),
    };
    if let Err(e) = bus.publish(event) {
        warn!("Scheduler failed to publish tick '{}': {}", entry.name, e);
    } else {
        debug!("Scheduler fired '{}' ({})", entry.name, fire_id);
    }
}

/// Fire every due tick of one schedule entry, applying the misfire policy.
/// Returns the cursor to persist.
async fn fire_due_entry(
    deps: &SchedulerDeps,
    entry: &ScheduleEntry,
    now: i64,
    last: Option<i64>,
) -> i64 {
    let after = |ts: i64| {
        DateTime::from_timestamp(ts, 0)
            .unwrap_or_else(|| DateTime::from_timestamp(0, 0).expect("epoch"))
            .to_utc()
    };
    let Some(last) = last else {
        // First sighting: no catch-up, the cursor starts now.
        return now;
    };
    // Collect dues since the cursor.
    let mut dues = Vec::new();
    let mut cursor = last;
    while dues.len() < MAX_CATCH_UP_FIRES as usize {
        let next = match entry
            .parsed
            .next_after(after(cursor), entry.spec.tz.as_deref())
        {
            Ok(next) => next.timestamp(),
            Err(e) => {
                warn!(
                    "Schedule '{}' cannot compute the next fire: {}",
                    entry.name, e
                );
                return now;
            }
        };
        if next > now {
            break;
        }
        dues.push(next);
        cursor = next;
    }
    if dues.is_empty() {
        return last;
    }
    let fire = |fire_at: i64| {
        let execution_id = match &entry.spec.target {
            ScheduleTarget::ExecutionScoped => {
                let bound = deps.bindings.bound_execution(&entry.name);
                if bound.is_none() {
                    debug!(
                        "Schedule '{}' has no bound execution, skipping tick",
                        entry.name
                    );
                }
                bound
            }
            ScheduleTarget::Create { .. } => None,
        };
        // Execution-scoped ticks without a binding are skipped (above);
        // creation ticks always publish execution-less.
        let should_publish = !matches!(&entry.spec.target, ScheduleTarget::ExecutionScoped)
            || execution_id.is_some();
        if should_publish {
            publish_tick(&deps.event_bus, entry, fire_at, execution_id);
        }
    };
    match entry.spec.misfire {
        ScheduleMisfirePolicy::Skip => {
            if dues.len() == 1 {
                fire(dues[0]);
            } else {
                debug!(
                    "Schedule '{}' skipped {} missed tick(s)",
                    entry.name,
                    dues.len()
                );
            }
            now
        }
        ScheduleMisfirePolicy::FireOnce => {
            fire(*dues.last().expect("non-empty"));
            now
        }
        ScheduleMisfirePolicy::FireAll => {
            for due in &dues {
                fire(*due);
            }
            now
        }
    }
}

/// Run the scheduler loop until shutdown.
pub async fn run_scheduler(deps: SchedulerDeps) {
    loop {
        let entries = snapshot_schedules(&deps.registries);
        let now_ms = wf_common::now();
        let now = now_ms / 1000;
        // Fire dues first (catch-up from before this loop started).
        for entry in &entries {
            let last = deps.state_store.last_fire(&entry.name).await;
            let cursor = fire_due_entry(&deps, entry, now, last).await;
            deps.state_store.set_last_fire(&entry.name, cursor).await;
        }
        // Sleep until the nearest next fire (or the refresh interval).
        let mut sleep_for = TEMPLATE_REFRESH_INTERVAL;
        for entry in &entries {
            let anchor = deps.state_store.last_fire(&entry.name).await.unwrap_or(now);
            let anchor = DateTime::from_timestamp(anchor, 0)
                .unwrap_or_else(|| DateTime::from_timestamp(0, 0).expect("epoch").to_utc())
                .to_utc();
            if let Ok(next) = entry.parsed.next_after(anchor, entry.spec.tz.as_deref()) {
                let delta = next.timestamp() - now;
                if delta > 0 {
                    sleep_for = sleep_for.min(Duration::from_secs(delta as u64));
                } else {
                    sleep_for = Duration::from_secs(1);
                    break;
                }
            }
        }
        tokio::select! {
            _ = deps.shutdown.cancelled() => {
                debug!("Scheduler shutdown requested");
                break;
            }
            _ = tokio::time::sleep(sleep_for) => {}
        }
    }
}

/// Spawn the scheduler background task sharing the listener shutdown token.
pub fn spawn_scheduler(deps: SchedulerDeps) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move { run_scheduler(deps).await })
}
