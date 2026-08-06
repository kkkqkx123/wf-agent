//! Typed workflow builder.
//!
//! Construction is split into two phases tracked in the type system: `Empty`
//! (no nodes yet) and `Building` (at least one node; `build()` / `save()` are
//! available). `build()` runs the config-level and full graph validation, so
//! a workflow saved through [`WorkflowBuilder`] always passes the graph
//! validator.

use std::marker::PhantomData;

use wf_types::node::BaseStaticNode;
use wf_types::tool::AvailableTools;
use wf_types::trigger::TriggerDefinition;
use wf_types::workflow::config::WorkflowConfig;
use wf_types::workflow::definition::{
    WorkflowDefinition, WorkflowDefinitionType, WorkflowMetadata,
};
use wf_types::workflow::edge::{Edge, EdgeType};
use wf_types::workflow_execution::VariableDefinition;

use crate::builder::NodeBuilder;
use crate::ApiContext;

/// Marker: no nodes have been added yet.
#[derive(Debug)]
pub struct Empty;

/// Marker: at least one node has been added; the builder can be built/saved.
#[derive(Debug)]
pub struct Building;

/// Consuming workflow builder with type-level phase tracking.
#[derive(Debug)]
pub struct WorkflowBuilder<S> {
    id: String,
    name: String,
    description: Option<String>,
    r#type: Option<WorkflowDefinitionType>,
    version: Option<String>,
    config: Option<WorkflowConfig>,
    variables: Vec<VariableDefinition>,
    triggers: Vec<TriggerDefinition>,
    metadata: Option<WorkflowMetadata>,
    available_tools: Option<AvailableTools>,
    nodes: Vec<BaseStaticNode>,
    edges: Vec<Edge>,
    _marker: PhantomData<S>,
}

impl WorkflowBuilder<Empty> {
    /// Start building a workflow with the given id.
    pub fn new(id: impl Into<String>) -> Self {
        let id = id.into();
        Self {
            id: id.clone(),
            name: id,
            description: None,
            r#type: None,
            version: None,
            config: None,
            variables: Vec::new(),
            triggers: Vec::new(),
            metadata: None,
            available_tools: None,
            nodes: Vec::new(),
            edges: Vec::new(),
            _marker: PhantomData,
        }
    }

    /// Add a node, entering the `Building` phase.
    pub fn add_node(self, node: BaseStaticNode) -> crate::ApiResult<WorkflowBuilder<Building>> {
        if self.nodes.iter().any(|n| n.id == node.id) {
            return Err(crate::ApiError::Validation(format!(
                "duplicate node id '{}' in workflow '{}'",
                node.id, self.id
            )));
        }
        let mut nodes = self.nodes;
        nodes.push(node);
        Ok(WorkflowBuilder {
            id: self.id,
            name: self.name,
            description: self.description,
            r#type: self.r#type,
            version: self.version,
            config: self.config,
            variables: self.variables,
            triggers: self.triggers,
            metadata: self.metadata,
            available_tools: self.available_tools,
            nodes,
            edges: self.edges,
            _marker: PhantomData,
        })
    }

    /// Add a START node (default id `start`).
    pub fn add_start_node(self) -> crate::ApiResult<WorkflowBuilder<Building>> {
        self.add_node(NodeBuilder::start("start").build())
    }

    /// Add an END node (default id `end`).
    pub fn add_end_node(self) -> crate::ApiResult<WorkflowBuilder<Building>> {
        self.add_node(NodeBuilder::end("end").build())
    }
}

impl WorkflowBuilder<Building> {
    /// Add another node to the workflow.
    pub fn add_node(mut self, node: BaseStaticNode) -> crate::ApiResult<Self> {
        if self.nodes.iter().any(|n| n.id == node.id) {
            return Err(crate::ApiError::Validation(format!(
                "duplicate node id '{}' in workflow '{}'",
                node.id, self.id
            )));
        }
        self.nodes.push(node);
        Ok(self)
    }

    /// Add another END node.
    pub fn add_end_node(self) -> crate::ApiResult<Self> {
        self.add_node(NodeBuilder::end("end").build())
    }

    /// Validate the workflow (config + graph) and produce its definition.
    pub fn build(self) -> crate::ApiResult<WorkflowDefinition> {
        let definition = self.into_definition();
        crate::workflow::validate_workflow(&definition)?;
        Ok(definition)
    }

    /// Build and persist the workflow through the unified save path (storage +
    /// execution registry), which re-runs the same validation.
    pub async fn save(self, ctx: &ApiContext) -> crate::ApiResult<()> {
        let definition = self.build()?;
        crate::workflow::save_workflow(ctx, &definition).await
    }
}

impl<S> WorkflowBuilder<S> {
    /// Set the workflow name.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Set the workflow description.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set the workflow version.
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// Set the workflow definition type.
    pub fn with_type(mut self, r#type: WorkflowDefinitionType) -> Self {
        self.r#type = Some(r#type);
        self
    }

    /// Set the workflow-level config.
    pub fn config(mut self, config: WorkflowConfig) -> Self {
        self.config = Some(config);
        self
    }

    /// Set the workflow metadata.
    pub fn metadata(mut self, metadata: WorkflowMetadata) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Set the available tools of the workflow.
    pub fn available_tools(mut self, available_tools: AvailableTools) -> Self {
        self.available_tools = Some(available_tools);
        self
    }

    /// Add a variable definition.
    pub fn add_variable(mut self, variable: VariableDefinition) -> Self {
        self.variables.push(variable);
        self
    }

    /// Add a trigger definition.
    pub fn add_trigger(mut self, trigger: TriggerDefinition) -> Self {
        self.triggers.push(trigger);
        self
    }

    /// Connect two registered nodes with a default edge.
    pub fn add_edge(
        mut self,
        from: impl Into<String>,
        to: impl Into<String>,
    ) -> crate::ApiResult<Self> {
        let from = from.into();
        let to = to.into();
        if !self.nodes.iter().any(|n| n.id == from) {
            return Err(crate::ApiError::Validation(format!(
                "edge references unknown source node '{}' in workflow '{}'",
                from, self.id
            )));
        }
        if !self.nodes.iter().any(|n| n.id == to) {
            return Err(crate::ApiError::Validation(format!(
                "edge references unknown target node '{}' in workflow '{}'",
                to, self.id
            )));
        }
        self.edges.push(Edge {
            id: wf_common::generate_id(),
            source_node_id: from,
            target_node_id: to,
            r#type: EdgeType::Default,
            condition: None,
            label: None,
            description: None,
            weight: None,
            metadata: None,
        });
        Ok(self)
    }

    /// Node ids currently registered.
    pub fn node_ids(&self) -> Vec<&str> {
        self.nodes.iter().map(|n| n.id.as_str()).collect()
    }
}

impl<S> WorkflowBuilder<S> {
    fn into_definition(self) -> WorkflowDefinition {
        let now = wf_common::now();
        WorkflowDefinition {
            id: self.id,
            name: self.name,
            description: self.description,
            r#type: self.r#type,
            version: self.version,
            nodes: self.nodes,
            edges: self.edges,
            config: self.config,
            variables: (!self.variables.is_empty()).then_some(self.variables),
            triggers: (!self.triggers.is_empty()).then_some(self.triggers),
            triggered_subworkflow_config: None,
            metadata: self.metadata,
            created_at: now,
            updated_at: now,
            available_tools: self.available_tools,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_valid_workflow_definition() {
        let definition = WorkflowBuilder::new("wf-builder-1")
            .name("Builder workflow")
            .add_start_node()
            .unwrap()
            .add_end_node()
            .unwrap()
            .add_edge("start", "end")
            .unwrap()
            .build()
            .expect("valid graph must build");
        assert_eq!(definition.id, "wf-builder-1");
        assert_eq!(definition.name, "Builder workflow");
        assert_eq!(definition.nodes.len(), 2);
        assert_eq!(definition.edges.len(), 1);
    }

    #[test]
    fn add_node_rejects_duplicates() {
        let start = NodeBuilder::start("start").build();
        let err = WorkflowBuilder::new("wf-dup")
            .add_node(start.clone())
            .and_then(|b| b.add_node(start))
            .unwrap_err();
        assert!(matches!(err, crate::ApiError::Validation(_)));
    }

    #[test]
    fn build_rejects_workflow_without_end() {
        let err = WorkflowBuilder::new("wf-no-end")
            .add_start_node()
            .unwrap()
            .build()
            .unwrap_err();
        assert!(matches!(err, crate::ApiError::Validation(_)));
        assert!(err.to_string().contains("END"));
    }

    #[test]
    fn build_rejects_unpaired_loop() {
        let builder = WorkflowBuilder::new("wf-bad-loop")
            .add_start_node()
            .unwrap()
            .add_node(NodeBuilder::loop_start("ls", "l1").build())
            .unwrap()
            .add_end_node()
            .unwrap()
            .add_edge("start", "ls")
            .unwrap()
            .add_edge("ls", "end")
            .unwrap();
        let err = builder.build().unwrap_err();
        assert!(matches!(err, crate::ApiError::Validation(_)));
        assert!(err.to_string().contains("LOOP_END"));
    }

    #[test]
    fn build_accepts_fork_join_graph() {
        let builder = WorkflowBuilder::new("wf-fork-join")
            .add_start_node()
            .unwrap()
            .add_node(NodeBuilder::fork("fork", vec![("p1", "a"), ("p2", "b")]).build())
            .unwrap()
            .add_node(
                NodeBuilder::variable(
                    "a",
                    "va",
                    wf_types::node::configs::VariableNodeType::String,
                    "${input.x}",
                )
                .build(),
            )
            .unwrap()
            .add_node(
                NodeBuilder::variable(
                    "b",
                    "vb",
                    wf_types::node::configs::VariableNodeType::String,
                    "${input.y}",
                )
                .build(),
            )
            .unwrap()
            .add_node(NodeBuilder::join("join", vec!["p1", "p2"]).build())
            .unwrap()
            .add_end_node()
            .unwrap()
            .add_edge("start", "fork")
            .unwrap()
            .add_edge("fork", "a")
            .unwrap()
            .add_edge("fork", "b")
            .unwrap()
            .add_edge("a", "join")
            .unwrap()
            .add_edge("b", "join")
            .unwrap()
            .add_edge("join", "end")
            .unwrap();
        assert!(builder.build().is_ok());
    }
}
