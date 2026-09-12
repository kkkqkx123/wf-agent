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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<crate::Metadata>,
    pub created_at: super::super::Timestamp,
    pub updated_at: super::super::Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_checkpoint: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint_description_template: Option<String>,
}
