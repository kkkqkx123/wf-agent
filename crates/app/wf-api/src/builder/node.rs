//! Typed node builder.
//!
//! A consuming builder whose construction phase lives in the type system:
//! `NoType` (before a node type is chosen) and `Typed` (after, where
//! [`NodeBuilder::build`] becomes available). Convenience constructors
//! (`start`, `llm`, `variable`, `fork`, ...) produce a `Typed` builder
//! directly and build canonical configs for the matching node type.

use std::marker::PhantomData;

use serde_json::Value;

use wf_types::node::configs::{
    AgentLoopNodeConfig, ForkNodeConfig, ForkPath, ForkStrategy, InteractionOperationType,
    JoinNodeConfig, JoinStrategy, LlmNodeConfig, LoopEndNodeConfig, LoopStartNodeConfig,
    RouteCondition, RouteNodeConfig, ScriptNodeConfig, ScriptRisk, SubgraphNodeConfig,
    SyncNodeConfig, UserInteractionNodeConfig, VariableNodeConfig, VariableNodeType,
};
use wf_types::node::{BaseStaticNode, NodeExecutionConfig, StaticNodeType};

/// Marker: the node type has not been assigned yet.
#[derive(Debug)]
pub struct NoType;

/// Marker: the node type has been assigned; [`NodeBuilder::build`] is
/// available in this phase.
#[derive(Debug)]
pub struct Typed;

/// Consuming node builder with type-level phase tracking.
#[derive(Debug)]
pub struct NodeBuilder<S> {
    id: String,
    node_type: StaticNodeType,
    name: Option<String>,
    description: Option<String>,
    config: Value,
    execution_config: Option<NodeExecutionConfig>,
    _marker: PhantomData<S>,
}

impl NodeBuilder<NoType> {
    /// Start building a node with the given id.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            node_type: StaticNodeType::Start,
            name: None,
            description: None,
            config: Value::Object(Default::default()),
            execution_config: None,
            _marker: PhantomData,
        }
    }

    /// Assign the node type, entering the `Typed` phase.
    pub fn type_(self, node_type: StaticNodeType) -> NodeBuilder<Typed> {
        NodeBuilder {
            id: self.id,
            node_type,
            name: self.name,
            description: self.description,
            config: self.config,
            execution_config: self.execution_config,
            _marker: PhantomData,
        }
    }

    /// START node.
    pub fn start(id: impl Into<String>) -> NodeBuilder<Typed> {
        Self::new(id).type_(StaticNodeType::Start)
    }

    /// END node.
    pub fn end(id: impl Into<String>) -> NodeBuilder<Typed> {
        Self::new(id).type_(StaticNodeType::End)
    }

    /// LLM node referencing a registered `profile_id`.
    pub fn llm(id: impl Into<String>, profile_id: impl Into<String>) -> NodeBuilder<Typed> {
        let config = LlmNodeConfig {
            profile_id: profile_id.into(),
            context_id: None,
            output_context: None,
            parameters: None,
            max_tool_calls_per_request: None,
            tool_call_format: None,
        };
        Self::new(id)
            .type_(StaticNodeType::Llm)
            .with_config(to_value(config))
    }

    /// SCRIPT node referencing a registered script.
    pub fn script(
        id: impl Into<String>,
        script_name: impl Into<String>,
        risk: ScriptRisk,
    ) -> NodeBuilder<Typed> {
        let config = ScriptNodeConfig {
            script_name: script_name.into(),
            risk,
            inline: None,
            template: None,
            executor: None,
            flow_id: None,
            arguments: None,
            output_mapping: None,
            sandbox: None,
        };
        Self::new(id)
            .type_(StaticNodeType::Script)
            .with_config(to_value(config))
    }

    /// VARIABLE node that stores a computed value.
    pub fn variable(
        id: impl Into<String>,
        variable_name: impl Into<String>,
        variable_type: VariableNodeType,
        expression: impl Into<String>,
    ) -> NodeBuilder<Typed> {
        let config = VariableNodeConfig {
            variable_name: variable_name.into(),
            variable_type: Some(variable_type),
            expression: Some(expression.into()),
            readonly: None,
        };
        Self::new(id)
            .type_(StaticNodeType::Variable)
            .with_config(to_value(config))
    }

    /// ROUTE node with conditional branches.
    pub fn route(
        id: impl Into<String>,
        conditions: Vec<(impl Into<String>, impl Into<String>)>,
        default_target_node_id: Option<String>,
    ) -> NodeBuilder<Typed> {
        let conditions = conditions
            .into_iter()
            .map(|(expression, target_node_id)| RouteCondition {
                expression: expression.into(),
                target_node_id: target_node_id.into(),
                priority: None,
            })
            .collect();
        let config = RouteNodeConfig {
            conditions,
            default_target_node_id,
        };
        Self::new(id)
            .type_(StaticNodeType::Route)
            .with_config(to_value(config))
    }

    /// FORK node with parallel branches (`(path_id, child_node_id)` pairs).
    pub fn fork(
        id: impl Into<String>,
        paths: Vec<(impl Into<String>, impl Into<String>)>,
    ) -> NodeBuilder<Typed> {
        let fork_paths: Vec<ForkPath> = paths
            .into_iter()
            .map(|(path_id, child_node_id)| ForkPath {
                path_id: path_id.into(),
                child_node_id: child_node_id.into(),
            })
            .collect();
        let config = ForkNodeConfig {
            fork_paths,
            fork_strategy: ForkStrategy::Parallel,
            failure_strategy: None,
            max_failed_branches: None,
            child_execution_timeout: None,
            total_branch_timeout: None,
            wait_for_completion: None,
        };
        Self::new(id)
            .type_(StaticNodeType::Fork)
            .with_config(to_value(config))
    }

    /// JOIN node re-merging the branches of a FORK.
    pub fn join(
        id: impl Into<String>,
        fork_path_ids: Vec<impl Into<String>>,
    ) -> NodeBuilder<Typed> {
        let config = JoinNodeConfig {
            fork_path_ids: fork_path_ids.into_iter().map(Into::into).collect(),
            join_strategy: JoinStrategy::WaitForAll,
            threshold: None,
            timeout: None,
            main_path_id: None,
        };
        Self::new(id)
            .type_(StaticNodeType::Join)
            .with_config(to_value(config))
    }

    /// SYNC node exchanging variables between fork paths.
    pub fn sync(
        id: impl Into<String>,
        source_path_id: impl Into<String>,
        target_path_id: Option<String>,
    ) -> NodeBuilder<Typed> {
        let config = SyncNodeConfig {
            source_path_id: source_path_id.into(),
            target_path_id,
            variable_mappings: None,
            wait_for_completion: None,
            timeout: None,
            variable_exchanges: None,
        };
        Self::new(id)
            .type_(StaticNodeType::Sync)
            .with_config(to_value(config))
    }

    /// LOOP_START node opening a loop body.
    pub fn loop_start(id: impl Into<String>, loop_id: impl Into<String>) -> NodeBuilder<Typed> {
        let config = LoopStartNodeConfig {
            loop_id: loop_id.into(),
            variable_inputs: None,
            data_source: None,
            max_iterations: 1,
            on_iteration_failure: None,
            max_consecutive_failures: None,
            break_condition: None,
        };
        Self::new(id)
            .type_(StaticNodeType::LoopStart)
            .with_config(to_value(config))
    }

    /// LOOP_END node closing a loop body.
    pub fn loop_end(
        id: impl Into<String>,
        loop_id: impl Into<String>,
        loop_start_node_id: Option<String>,
    ) -> NodeBuilder<Typed> {
        let config = LoopEndNodeConfig {
            loop_id: loop_id.into(),
            break_condition: None,
            loop_start_node_id,
        };
        Self::new(id)
            .type_(StaticNodeType::LoopEnd)
            .with_config(to_value(config))
    }

    /// SUBGRAPH node referencing another workflow.
    pub fn subgraph(id: impl Into<String>, subgraph_id: impl Into<String>) -> NodeBuilder<Typed> {
        let config = SubgraphNodeConfig {
            subgraph_id: Some(subgraph_id.into()),
            embed_id: None,
            async_: None,
            retry_policy: None,
            on_failure: None,
            fallback_output: None,
            variable_inputs: None,
            variable_outputs: None,
        };
        Self::new(id)
            .type_(StaticNodeType::Subgraph)
            .with_config(to_value(config))
    }

    /// USER_INTERACTION node pausing for external input.
    pub fn user_interaction(
        id: impl Into<String>,
        prompt: impl Into<String>,
        operation_type: InteractionOperationType,
    ) -> NodeBuilder<Typed> {
        let config = UserInteractionNodeConfig {
            operation_type,
            variables: None,
            message: None,
            prompt: prompt.into(),
            timeout: None,
            metadata: None,
        };
        Self::new(id)
            .type_(StaticNodeType::UserInteraction)
            .with_config(to_value(config))
    }

    /// AGENT_LOOP node running a registered agent loop.
    pub fn agent_loop(
        id: impl Into<String>,
        agent_loop_id: impl Into<String>,
    ) -> NodeBuilder<Typed> {
        let config = AgentLoopNodeConfig {
            agent_loop_id: Some(agent_loop_id.into()),
            inline_definition: None,
            retry_policy: None,
            execution_timeout: None,
            on_failure: None,
            fallback_output: None,
        };
        Self::new(id)
            .type_(StaticNodeType::AgentLoop)
            .with_config(to_value(config))
    }
}

impl<S> NodeBuilder<S> {
    /// Set the node name (defaults to the node id).
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set the node description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set the node config.
    pub fn with_config(mut self, config: Value) -> Self {
        self.config = config;
        self
    }

    /// Set the node execution config.
    pub fn with_execution_config(mut self, execution_config: NodeExecutionConfig) -> Self {
        self.execution_config = Some(execution_config);
        self
    }

    /// Set the node-level checkpoint configuration (overrides the workflow
    /// checkpoint policy for this node where explicit).
    pub fn with_checkpoint(
        mut self,
        checkpoint: wf_types::checkpoint::NodeCheckpointConfig,
    ) -> Self {
        let mut execution_config = self.execution_config.take().unwrap_or_default();
        execution_config.checkpoint = Some(checkpoint);
        self.execution_config = Some(execution_config);
        self
    }

    /// Node id.
    pub fn id(&self) -> &str {
        &self.id
    }
}

impl NodeBuilder<Typed> {
    /// Build the static node. The node type is always assigned in this phase.
    pub fn build(self) -> BaseStaticNode {
        BaseStaticNode {
            id: self.id.clone(),
            node_type: self.node_type,
            name: self.name.or(Some(self.id)),
            description: self.description,
            config: Some(self.config),
            execution_config: self.execution_config,
        }
    }
}

/// Serialize a node config struct into its canonical JSON representation.
fn to_value(config: impl serde::Serialize) -> Value {
    serde_json::to_value(config).unwrap_or_else(|_| Value::Object(Default::default()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_typed_node_after_assigning_type() {
        let node = NodeBuilder::new("n1")
            .with_name("greeting")
            .type_(StaticNodeType::Llm)
            .with_config(serde_json::json!({"profile_id": "mock"}))
            .build();
        assert_eq!(node.id, "n1");
        assert_eq!(node.node_type, StaticNodeType::Llm);
        assert_eq!(node.name.as_deref(), Some("greeting"));
        assert_eq!(node.config.unwrap()["profile_id"], "mock");
    }

    #[test]
    fn name_defaults_to_id() {
        let node =
            NodeBuilder::variable("v1", "final", VariableNodeType::String, "${input.a}").build();
        assert_eq!(node.name.as_deref(), Some("v1"));
        let config = node.config.unwrap();
        assert_eq!(config["variable_name"], "final");
        assert_eq!(config["variable_type"], "string");
        assert_eq!(config["expression"], "${input.a}");
    }

    #[test]
    fn fork_and_join_build_canonical_configs() {
        let fork = NodeBuilder::fork("fork", vec![("p1", "a"), ("p2", "b")]).build();
        let fork_config = fork.config.unwrap();
        assert_eq!(fork_config["fork_strategy"], "parallel");
        assert_eq!(fork_config["fork_paths"][0]["path_id"], "p1");

        let join = NodeBuilder::join("join", vec!["p1", "p2"]).build();
        let join_config = join.config.unwrap();
        assert_eq!(join_config["join_strategy"], "wait_for_all");
        assert_eq!(
            join_config["fork_path_ids"],
            serde_json::json!(["p1", "p2"])
        );
    }

    #[test]
    fn loop_nodes_build_canonical_configs() {
        let start = NodeBuilder::loop_start("ls", "l1").build();
        assert_eq!(start.config.unwrap()["loop_id"], "l1");

        let end = NodeBuilder::loop_end("le", "l1", Some("ls".into())).build();
        let config = end.config.unwrap();
        assert_eq!(config["loop_id"], "l1");
        assert_eq!(config["loop_start_node_id"], "ls");
    }

    #[test]
    fn script_and_route_configs_are_valid() {
        let script = NodeBuilder::script("s", "greet", ScriptRisk::Low).build();
        let config = script.config.unwrap();
        assert_eq!(config["risk"], "low");

        let route = NodeBuilder::route(
            "r",
            vec![("${a} > 1", "next")],
            Some("fallback".to_string()),
        )
        .build();
        let config = route.config.unwrap();
        assert_eq!(config["conditions"][0]["expression"], "${a} > 1");
        assert_eq!(config["default_target_node_id"], "fallback");
    }
}
