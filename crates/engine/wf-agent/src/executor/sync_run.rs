use wf_tools::callback::{AgentLoopConfig, AgentLoopInput, AgentLoopOutput, ParentLink};
use wf_types::Id;

use crate::error::AgentResult;

use super::reclaim::ChildRunReclaim;
use super::AgentLoopExecutor;

impl AgentLoopExecutor {
    pub async fn execute(
        &self,
        config: AgentLoopConfig,
        input: AgentLoopInput,
    ) -> AgentResult<AgentLoopOutput> {
        self.execute_with_parent(config, input, None).await
    }

    /// Synchronous dispatch linked under a live parent execution. `None`
    /// runs as a root.
    pub async fn execute_with_parent(
        &self,
        config: AgentLoopConfig,
        input: AgentLoopInput,
        parent: Option<ParentLink>,
    ) -> AgentResult<AgentLoopOutput> {
        Self::gate_config(&config, &self.registry, self.max_iterations_cap)?;
        Self::gate_input_with_config(&config, &input)?;
        self.check_sub_agent_depth(parent.as_ref())?;
        let config = Self::inherit_file_scope(config, parent.as_ref())?;
        // Pre-allocate the child id so the parent stream can attribute the
        // run before it starts instead of only after it settles. The
        // display name derives from the same config/input on both ends.
        let child_id = Id::from(wf_common::generate_id());
        let child_name = crate::subagent::display_name(&config, &input);
        if let Some(ref link) = parent {
            crate::subagent::emit_started(link, &child_id, &config, &input, false);
        }
        let parent_manager = parent.as_ref().map(|link| link.manager.clone());
        let reclaim = ChildRunReclaim::arm(
            self.agent_registry.clone(),
            child_id.clone(),
            parent_manager,
        );
        let coordinator = self
            .coordinator(parent.as_ref())
            .with_agent_loop_id(child_id.clone());
        let output = coordinator.execute(config, input).await;
        // The run settled (successfully or not): the child reference leaves
        // the parent hierarchy and the reclaim guard disarms.
        reclaim.finish();
        if let Some(ref link) = parent {
            crate::subagent::emit_ended(link, &child_id, &child_name, output.is_ok(), false);
        }
        // Sync path writes the result slot too, keeping both dispatch paths
        // consistent: a later query_execution_status returns the output.
        if let Ok(output) = &output {
            self.agent_registry
                .store_result(output.agent_loop_id.clone(), output.clone());
        }
        output
    }
}
