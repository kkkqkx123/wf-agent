use serde::{Deserialize, Serialize};

/// Per-event dispatch mode shared by every template subscribed to the same
/// competition scope (see the scope module).
///
/// Unique is the default: at most one template may subscribe to one scope,
/// and a second subscriber is a load-time configuration error. BestWin is an
/// explicit opt-in allowing several subscribers, in which case every
/// subscriber must declare an explicit priority and all priorities within
/// the scope must differ; the runtime runs the single highest-priority
/// winner. Equal priorities within one scope are never meaningful: under a
/// single winner the outcome would depend on load order, and under multiple
/// runners the state-changing actions would race.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TriggerDispatchMode {
    #[default]
    Unique,
    BestWin,
}

/// Runtime limits for trigger dispatch (concurrency gate + circuit
/// breaker). All fields are optional; absent means the historical default
/// (unbounded concurrency, single-winner dispatch, no burst cap), so
/// existing deployments see no behavior change.
///
/// Tuning notes:
/// - `max_concurrent_actions`: bound on concurrently executing trigger
///   actions. Set to roughly the number of worker slots reserved for
///   side effects (e.g. 8-32 for IO-bound actions). Lower it when actions
///   contend on a shared downstream (database, rate-limited API).
/// - `dispatch_burst_limit`: circuit breaker on winners executed for one
///   event. At most this many winners run per event; extras are dropped
///   with a warning (never silently queued). Keep at 1 unless
///   cross-scope fan-out is explicitly desired.
/// Over-limit behavior is always warn-and-drop, never silent queueing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TriggerRuntimeLimits {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_concurrent_actions: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dispatch_burst_limit: Option<u32>,
}

impl TriggerRuntimeLimits {
    /// Validate configured values (zero means no capacity and is rejected;
    /// use absent for unlimited).
    pub fn validate(&self) -> Result<(), String> {
        if self.max_concurrent_actions == Some(0) {
            return Err("max_concurrent_actions must be at least 1 when set".to_string());
        }
        if self.dispatch_burst_limit == Some(0) {
            return Err("dispatch_burst_limit must be at least 1 when set".to_string());
        }
        Ok(())
    }

    /// Apply the burst cap to a winner list, returning the dropped count.
    /// Pure helper so the listener and tests share the semantics.
    pub fn apply_burst_cap<T>(&self, winners: &mut Vec<T>) -> usize {
        let Some(cap) = self.dispatch_burst_limit else {
            return 0;
        };
        let cap = cap as usize;
        if winners.len() > cap {
            let dropped = winners.len() - cap;
            winners.truncate(cap);
            return dropped;
        }
        0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TriggerTemplate {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<super::TriggerCondition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<super::TriggerAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// Maximum firings counted per execution (`execution_id:template_name`).
    /// Concurrent executions hold independent budgets; the in-flight guard
    /// additionally prevents re-entrant runs of the same pair.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_triggers: Option<u32>,
    /// Template priority within its competition scope. Under the default
    /// Unique mode at most one template subscribes to a scope so no
    /// comparison happens; under BestWin every subscriber must set an
    /// explicit priority, all priorities within the scope must differ, and
    /// the highest one wins. Absent means Unique (the default).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<i32>,
    /// Dispatch mode required for the event scope this template subscribes
    /// to. Absent means Unique. When several templates share one scope they
    /// must agree: any BestWin declaration switches the scope to BestWin,
    /// while mixing explicit Unique with BestWin is a load-time error.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dispatch_mode: Option<TriggerDispatchMode>,
    /// Multi-effect opt-in for one event. Default (absent/false) keeps the
    /// single-winner single-execution behavior. Setting it to true declares
    /// that this template wants ordered multi-effect execution, which
    /// additionally requires `effect_order`; the runtime still executes the
    /// single winner until the ordered multi-effect design lands, so the
    /// declaration is validated now and executed later. Any future
    /// multi-execution must be explicit here plus an explicit order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_multi_effect: Option<bool>,
    /// Explicit execution order for multi-effect mode: non-empty list of
    /// effect names in execution order. Required when `allow_multi_effect`
    /// is true, forbidden otherwise. Exchangeability of the listed effects
    /// is a deployment obligation (state-changing effects that race must
    /// not share an event).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect_order: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<crate::Metadata>,
    pub created_at: super::super::Timestamp,
    pub updated_at: super::super::Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_checkpoint: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint_description_template: Option<String>,
}
