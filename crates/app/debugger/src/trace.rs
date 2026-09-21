use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::step::StepRecord;
use crate::views::{BudgetView, TriggerTemplateView};

pub const TRACE_SCHEMA_V1: &str = "wf-debug-trace/v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TraceKind {
    Workflow,
    Agent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Trace {
    #[serde(default = "default_schema")]
    pub schema: String,
    pub kind: TraceKind,
    #[serde(default)]
    pub graph_ref: String,
    /// Id of the agent template that produced this trace (for example
    /// `@standard/explorer`). Empty means the source agent is unknown and
    /// builtin policy analysis is skipped.
    #[serde(default)]
    pub agent_template: String,
    #[serde(default)]
    pub initial_variables: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub steps: Vec<StepRecord>,
    #[serde(default)]
    pub assertions: Vec<crate::assert::Assertion>,
    #[serde(default)]
    pub trigger_templates: Vec<TriggerTemplateView>,
    /// Trace-level LLM budget the cost analyzer checks totals against.
    #[serde(default)]
    pub budget: Option<BudgetView>,
}

fn default_schema() -> String {
    TRACE_SCHEMA_V1.to_string()
}
