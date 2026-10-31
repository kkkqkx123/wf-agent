//! Typed workflow builder.
//!
//! A single consuming [`WorkflowBuilder`] assembles a `WorkflowDefinition`.
//! Correctness is enforced by [`WorkflowBuilder::build`] / [`WorkflowBuilder::save`],
//! which run the config-level and full graph validation, so a workflow saved
//! through the builder always passes the graph validator. Construction never
//! reaches an invalid graph through a separate type: the validator is the one
//! source of truth for what a well-formed workflow is.

use wf_types::hook::HookPointConfig;
use wf_types::node::BaseStaticNode;
use wf_types::tool::AvailableTools;
use wf_types::workflow::config::WorkflowConfig;
use wf_types::workflow::definition::{
    TriggeredSubworkflowConfig, WorkflowDefinition, WorkflowDefinitionType, WorkflowLayout,
    WorkflowMetadata,
};
use wf_types::workflow::edge::{Edge, EdgeType};
use wf_types::workflow_execution::VariableDefinition;

use crate::workflow::node_builder::NodeBuilder;
use crate::ApiContext;

/// Consuming workflow builder.
#[derive(Debug)]
pub struct WorkflowBuilder {
    id: String,
    name: String,
    description: Option<String>,
    r#type: Option<WorkflowDefinitionType>,
    version: Option<String>,
    config: Option<WorkflowConfig>,
    variables: Vec<VariableDefinition>,
    input_strict: Option<bool>,
    metadata: Option<WorkflowMetadata>,
    available_tools: Option<AvailableTools>,
    hooks: Option<Vec<HookPointConfig>>,
    triggered_subworkflow_config: Option<TriggeredSubworkflowConfig>,
    nodes: Vec<BaseStaticNode>,
    edges: Vec<Edge>,
    layout: Option<WorkflowLayout>,
}

/// Serialization format accepted by [`WorkflowBuilder::from_config`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowConfigFormat {
    Json,
    Toml,
}

impl WorkflowBuilder {
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
            input_strict: None,
            metadata: None,
            available_tools: None,
            hooks: None,
            triggered_subworkflow_config: None,
            nodes: Vec::new(),
            edges: Vec::new(),
            layout: None,
        }
    }

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

    /// Set the triggered-subworkflow config owned exclusively by the
    /// TriggeredSubworkflow declaration.
    pub fn triggered_subworkflow_config(mut self, config: TriggeredSubworkflowConfig) -> Self {
        self.triggered_subworkflow_config = Some(config);
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

    /// Set the persisted canvas layout (node positions and settings). Layout
    /// is display data; it survives topology-only round-trips so re-saving a
    /// rehydrated workflow never drops the saved graph positions.
    pub fn layout(mut self, layout: WorkflowLayout) -> Self {
        self.layout = Some(layout);
        self
    }

    /// Attach workflow-level hooks (BEFORE_EXECUTE / AFTER_EXECUTE per node).
    pub fn hooks(mut self, hooks: Vec<HookPointConfig>) -> Self {
        self.hooks = Some(hooks);
        self
    }

    /// Add a variable definition.
    pub fn add_variable(mut self, variable: VariableDefinition) -> Self {
        self.variables.push(variable);
        self
    }

    /// Add a node.
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

    /// Add a START node (default id `start`).
    pub fn add_start_node(self) -> crate::ApiResult<Self> {
        self.add_node(NodeBuilder::start("start").build())
    }

    /// Add an END node (default id `end`).
    pub fn add_end_node(self) -> crate::ApiResult<Self> {
        self.add_node(NodeBuilder::end("end").build())
    }

    /// Connect two registered nodes with a default edge.
    pub fn add_edge(
        mut self,
        from: impl Into<String>,
        to: impl Into<String>,
    ) -> crate::ApiResult<Self> {
        let from = from.into();
        let to = to.into();
        self.ensure_nodes_exist(&from, &to)?;
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
            error_route: None,
        });
        Ok(self)
    }

    /// Connect two registered nodes with a conditional edge. The condition is
    /// a `${...}` expression evaluated at runtime to select the branch.
    pub fn add_conditional_edge(
        mut self,
        from: impl Into<String>,
        to: impl Into<String>,
        condition: impl Into<String>,
    ) -> crate::ApiResult<Self> {
        let from = from.into();
        let to = to.into();
        self.ensure_nodes_exist(&from, &to)?;
        self.edges.push(Edge {
            id: wf_common::generate_id(),
            source_node_id: from,
            target_node_id: to,
            r#type: EdgeType::Conditional,
            condition: Some(condition.into()),
            label: None,
            description: None,
            weight: None,
            metadata: None,
            error_route: None,
        });
        Ok(self)
    }

    /// Node ids currently registered.
    pub fn node_ids(&self) -> Vec<&str> {
        self.nodes.iter().map(|n| n.id.as_str()).collect()
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

    /// Rehydrate a builder from a serialized `WorkflowDefinition` (JSON or
    /// TOML). Validation is deferred to [`WorkflowBuilder::build`] /
    /// [`WorkflowBuilder::save`].
    pub fn from_config(content: &str, format: WorkflowConfigFormat) -> crate::ApiResult<Self> {
        let definition: WorkflowDefinition = match format {
            WorkflowConfigFormat::Json => wf_config::parser::parse_json(content)?,
            WorkflowConfigFormat::Toml => wf_config::parser::parse_toml(content)?,
        };
        Ok(Self::from_definition(definition))
    }

    /// Rehydrate a builder from a JSON-serialized `WorkflowDefinition`.
    pub fn from_config_json(content: &str) -> crate::ApiResult<Self> {
        WorkflowBuilder::from_config(content, WorkflowConfigFormat::Json)
    }

    /// Rehydrate a builder from a TOML-serialized `WorkflowDefinition`.
    pub fn from_config_toml(content: &str) -> crate::ApiResult<Self> {
        WorkflowBuilder::from_config(content, WorkflowConfigFormat::Toml)
    }

    fn from_definition(definition: WorkflowDefinition) -> Self {
        Self {
            id: definition.id,
            name: definition.name,
            description: definition.description,
            r#type: definition.r#type,
            version: definition.version,
            config: definition.config,
            variables: definition.variables.unwrap_or_default(),
            input_strict: definition.input_strict,
            metadata: definition.metadata,
            available_tools: definition.available_tools,
            hooks: definition.hooks,
            triggered_subworkflow_config: definition.triggered_subworkflow_config,
            nodes: definition.nodes,
            edges: definition.edges,
            layout: definition.layout,
        }
    }

    fn ensure_nodes_exist(&self, from: &str, to: &str) -> crate::ApiResult<()> {
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
        Ok(())
    }

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
            input_strict: self.input_strict,
            triggered_subworkflow_config: self.triggered_subworkflow_config,
            metadata: self.metadata,
            created_at: now,
            updated_at: now,
            available_tools: self.available_tools,
            hooks: self.hooks,
            layout: self.layout,
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
    fn add_start_node_available_after_first_node() {
        let builder = WorkflowBuilder::new("wf-start-late")
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
            .add_start_node()
            .unwrap();
        assert!(builder.node_ids().contains(&"start"));
    }

    #[test]
    fn build_rejects_empty_workflow() {
        let err = WorkflowBuilder::new("wf-empty").build().unwrap_err();
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

    #[test]
    fn conditional_edge_carries_condition() {
        let builder = WorkflowBuilder::new("wf-cond")
            .add_start_node()
            .unwrap()
            .add_node(
                NodeBuilder::variable(
                    "branch",
                    "v",
                    wf_types::node::configs::VariableNodeType::String,
                    "${input.x}",
                )
                .build(),
            )
            .unwrap()
            .add_end_node()
            .unwrap()
            .add_edge("start", "branch")
            .unwrap()
            .add_conditional_edge("branch", "end", "${input.x == 'yes'}")
            .unwrap();

        let definition = builder.build().expect("graph must build");
        let edge = definition
            .edges
            .iter()
            .find(|e| e.source_node_id == "branch")
            .expect("conditional edge");
        assert_eq!(edge.r#type, EdgeType::Conditional);
        assert_eq!(edge.condition.as_deref(), Some("${input.x == 'yes'}"));
    }

    #[test]
    fn layout_survives_build_and_rehydration() {
        let mut positions = std::collections::BTreeMap::new();
        positions.insert(
            "start".to_string(),
            wf_types::workflow::definition::LayoutPoint { x: 40, y: 80 },
        );
        let layout = WorkflowLayout {
            positions,
            settings: None,
            updated_at: None,
        };
        let definition = WorkflowBuilder::new("wf-layout")
            .add_start_node()
            .unwrap()
            .add_end_node()
            .unwrap()
            .add_edge("start", "end")
            .unwrap()
            .layout(layout)
            .build()
            .expect("valid graph must build");
        let saved = definition.layout.as_ref().expect("layout persisted");
        assert_eq!(saved.positions.get("start").map(|p| p.x), Some(40));

        // Rehydrating the same definition must keep the layout, so a
        // topology-only re-save never silently drops saved positions.
        let rehydrated = WorkflowBuilder::from_definition(definition)
            .build()
            .expect("rehydrated graph must build");
        assert_eq!(
            rehydrated
                .layout
                .as_ref()
                .and_then(|l| l.positions.get("start"))
                .map(|p| p.y),
            Some(80)
        );
    }

    #[test]
    fn from_config_rehydrates_json_and_toml() {
        let json = r#"{
            "id": "wf-from-json",
            "name": "From JSON",
            "nodes": [
                {"id": "start", "node_type": "START"},
                {"id": "end", "node_type": "END"}
            ],
            "edges": [
                {"id": "e1", "source_node_id": "start", "target_node_id": "end", "type": "DEFAULT"}
            ],
            "created_at": 1000,
            "updated_at": 1000
        }"#;
        let builder = WorkflowBuilder::from_config_json(json).unwrap();
        let definition = builder.build().expect("json workflow must build");
        assert_eq!(definition.id, "wf-from-json");
        assert_eq!(definition.nodes.len(), 2);

        let toml = r#"
id = "wf-from-toml"
name = "From TOML"
created_at = 1000
updated_at = 1000

[[nodes]]
id = "start"
node_type = "START"

[[nodes]]
id = "end"
node_type = "END"

[[edges]]
id = "e1"
source_node_id = "start"
target_node_id = "end"
type = "DEFAULT"
"#;
        let builder = WorkflowBuilder::from_config_toml(toml).unwrap();
        let definition = builder.build().expect("toml workflow must build");
        assert_eq!(definition.id, "wf-from-toml");
        assert_eq!(definition.nodes.len(), 2);
    }
}
