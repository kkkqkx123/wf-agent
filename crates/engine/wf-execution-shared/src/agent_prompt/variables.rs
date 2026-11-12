use std::collections::HashMap;

use serde_json::Value;
use wf_metrics::MetricsRegistry;
use wf_resource::registry::ResourceRegistries;
use wf_tools::registry::ToolRegistry;

/// Narrow variable access shared by the workflow variable table and the
/// direct path input context.
pub trait VariableSource {
    fn get_variable(&self, name: &str) -> Option<Value>;
}

impl VariableSource for HashMap<String, Value> {
    fn get_variable(&self, name: &str) -> Option<Value> {
        self.get(name).cloned()
    }
}

impl VariableSource for dashmap::DashMap<String, Value> {
    fn get_variable(&self, name: &str) -> Option<Value> {
        self.get(name).map(|v| v.clone())
    }
}

impl VariableSource for std::sync::Arc<dashmap::DashMap<String, Value>> {
    fn get_variable(&self, name: &str) -> Option<Value> {
        self.get(name).map(|v| v.clone())
    }
}

impl crate::context::NodeExecutionContext {
    fn variable_source_get(&self, name: &str) -> Option<Value> {
        self.variables.get(name).map(|v| v.clone())
    }
}

impl VariableSource for crate::context::NodeExecutionContext {
    fn get_variable(&self, name: &str) -> Option<Value> {
        self.variable_source_get(name)
    }
}

/// Environment the assembly reads through: resource templates, tool state
/// for skill and MCP enrichment, and metrics for template observability.
pub struct PromptEnvironment<'a> {
    pub resource_registries: Option<&'a ResourceRegistries>,
    pub tool_registry: Option<&'a ToolRegistry>,
    pub metrics: Option<&'a MetricsRegistry>,
}

impl<'a> PromptEnvironment<'a> {
    pub fn new(
        resource_registries: Option<&'a ResourceRegistries>,
        tool_registry: Option<&'a ToolRegistry>,
        metrics: Option<&'a MetricsRegistry>,
    ) -> Self {
        Self {
            resource_registries,
            tool_registry,
            metrics,
        }
    }

    pub fn empty() -> Self {
        Self {
            resource_registries: None,
            tool_registry: None,
            metrics: None,
        }
    }

    pub(super) fn template_metrics(
        &self,
    ) -> Option<std::sync::Arc<wf_metrics::TemplateMetricsCollector>> {
        self.metrics.map(|m| m.template())
    }
}
