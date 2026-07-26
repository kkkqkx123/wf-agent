use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    Validation,
    Execution,
    NotFound,
    BusinessLogic,
    SystemExecution,
    AgentCheckpoint,
    WorkflowCheckpoint,
    EventSystem,
    DependencyInjection,
    StateManagement,
    Tool,
    Storage,
    Network,
    Resource,
    General,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ErrorContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<ErrorSeverity>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WfError {
    pub message: String,
    pub kind: ErrorKind,
    pub severity: ErrorSeverity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<HashMap<String, serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cause: Option<Box<WfError>>,
    pub name: String,
}

impl WfError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: ErrorKind::General,
            severity: ErrorSeverity::Error,
            context: None,
            cause: None,
            name: "WfError".into(),
        }
    }

    pub fn validation(message: impl Into<String>, field: impl Into<String>) -> Self {
        let mut ctx = HashMap::new();
        ctx.insert("field".into(), serde_json::Value::String(field.into()));
        Self {
            message: message.into(),
            kind: ErrorKind::Validation,
            severity: ErrorSeverity::Error,
            context: Some(ctx),
            cause: None,
            name: "ValidationError".into(),
        }
    }

    pub fn execution(message: impl Into<String>, node_id: Option<String>, workflow_id: Option<String>) -> Self {
        let mut ctx = HashMap::new();
        if let Some(v) = node_id {
            ctx.insert("node_id".into(), serde_json::Value::String(v));
        }
        if let Some(v) = workflow_id {
            ctx.insert("workflow_id".into(), serde_json::Value::String(v));
        }
        Self {
            message: message.into(),
            kind: ErrorKind::Execution,
            severity: ErrorSeverity::Error,
            context: Some(ctx),
            cause: None,
            name: "ExecutionError".into(),
        }
    }

    pub fn not_found(resource_type: impl Into<String>, resource_id: impl Into<String>) -> Self {
        let rt: String = resource_type.into();
        let ri: String = resource_id.into();
        let mut ctx = HashMap::new();
        ctx.insert("resource_type".into(), serde_json::Value::String(rt.clone()));
        ctx.insert("resource_id".into(), serde_json::Value::String(ri.clone()));
        Self {
            message: format!("{} not found: {}", rt, ri),
            kind: ErrorKind::NotFound,
            severity: ErrorSeverity::Error,
            context: Some(ctx),
            cause: None,
            name: "NotFoundError".into(),
        }
    }

    pub fn with_severity(mut self, severity: ErrorSeverity) -> Self {
        self.severity = severity;
        self
    }

    pub fn with_kind(mut self, kind: ErrorKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn with_context(mut self, context: HashMap<String, serde_json::Value>) -> Self {
        self.context = Some(context);
        self
    }

    pub fn with_cause(mut self, cause: WfError) -> Self {
        self.cause = Some(Box::new(cause));
        self
    }
}

impl std::fmt::Display for WfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{:?}:{:?}] {}: {}", self.kind, self.severity, self.name, self.message)
    }
}

impl std::error::Error for WfError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.cause
            .as_ref()
            .map(|c| c as &(dyn std::error::Error + 'static))
    }
}

pub type WfResult<T> = std::result::Result<T, WfError>;
