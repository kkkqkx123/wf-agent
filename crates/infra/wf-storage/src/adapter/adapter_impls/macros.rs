use crate::adapter::agent_draft::AgentDraftListOptions;
use crate::adapter::agent_execution::AgentExecutionListOptions;
use crate::adapter::agent_loop::AgentLoopListOptions;
use crate::adapter::agent_profile::AgentProfileListOptions;
use crate::adapter::agent_template::AgentTemplateListOptions;
use crate::adapter::checkpoint::CheckpointListOptions;
use crate::adapter::execution::WorkflowExecutionListOptions;
use crate::adapter::message::MessageListOptions;
use crate::adapter::node_draft::NodeDraftListOptions;
use crate::adapter::node_template::NodeTemplateListOptions;
use crate::adapter::pricing_rule::{PricingRule, PricingRuleListOptions};
use crate::adapter::script::ScriptListOptions;
use crate::adapter::task::TaskListOptions;
use crate::adapter::template_usage::TemplateUsageListOptions;
use crate::adapter::tool::ToolListOptions;
use crate::adapter::tool_definition::ToolDefinitionListOptions;
use crate::adapter::trigger_draft::TriggerDraftListOptions;
use crate::adapter::trigger_execution::TriggerExecutionListOptions;
use crate::adapter::trigger_template::TriggerTemplateListOptions;
use crate::adapter::user_interaction::UserInteractionListOptions;
use crate::adapter::variable::VariableListOptions;
use crate::adapter::workflow::WorkflowListOptions;
use crate::adapter::workflow_draft::WorkflowDraftListOptions;
use crate::make_base_adapter;

// ─── Macro invocation: generates BaseStorageAdapter impl + struct ───

make_base_adapter!(
    WorkflowStorage,
    wf_types::WorkflowDefinition,
    WorkflowListOptions
);
make_base_adapter!(
    WorkflowDraftStorage,
    wf_types::WorkflowDefinition,
    WorkflowDraftListOptions
);
make_base_adapter!(
    WorkflowExecutionStorage,
    wf_types::WorkflowExecution,
    WorkflowExecutionListOptions
);
make_base_adapter!(
    CheckpointStorage,
    wf_types::Checkpoint,
    CheckpointListOptions
);
make_base_adapter!(TaskStorage, wf_types::TaskStorageMetadata, TaskListOptions);
make_base_adapter!(
    AgentLoopStorage,
    wf_types::AgentLoopStorageMetadata,
    AgentLoopListOptions
);
make_base_adapter!(
    AgentExecutionStorage,
    wf_types::AgentExecution,
    AgentExecutionListOptions
);
make_base_adapter!(
    AgentDraftStorage,
    wf_types::agent::AgentDefinition,
    AgentDraftListOptions
);
make_base_adapter!(ToolStorage, wf_types::ToolStorageMetadata, ToolListOptions);
make_base_adapter!(
    ToolDefinitionStorage,
    wf_types::tool::Tool,
    ToolDefinitionListOptions
);
make_base_adapter!(
    AgentTemplateStorage,
    wf_types::agent::AgentTemplate,
    AgentTemplateListOptions
);
make_base_adapter!(
    ScriptStorage,
    wf_types::ScriptStorageMetadata,
    ScriptListOptions
);
make_base_adapter!(
    NodeTemplateStorage,
    wf_types::NodeTemplateStorageMetadata,
    NodeTemplateListOptions
);
make_base_adapter!(
    NodeDraftStorage,
    wf_types::NodeTemplateStorageMetadata,
    NodeDraftListOptions
);
make_base_adapter!(
    AgentProfileStorage,
    wf_types::AgentProfileStorageMetadata,
    AgentProfileListOptions
);
make_base_adapter!(
    TriggerTemplateStorage,
    wf_types::TriggerTemplateStorageMetadata,
    TriggerTemplateListOptions
);
make_base_adapter!(
    TriggerDraftStorage,
    wf_types::TriggerTemplateStorageMetadata,
    TriggerDraftListOptions
);
make_base_adapter!(
    UserInteractionStorage,
    wf_types::UserInteractionStorageMetadata,
    UserInteractionListOptions
);
make_base_adapter!(
    TriggerExecutionStorage,
    wf_types::TriggerExecutionStorageMetadata,
    TriggerExecutionListOptions
);
make_base_adapter!(
    MessageStorage,
    wf_types::MessageStorageMetadata,
    MessageListOptions
);
make_base_adapter!(
    VariableStorage,
    wf_types::VariableStorageMetadata,
    VariableListOptions
);
make_base_adapter!(
    TemplateUsageStorage,
    wf_types::TemplateUsageMetadata,
    TemplateUsageListOptions
);
make_base_adapter!(PricingRuleStorage, PricingRule, PricingRuleListOptions);
