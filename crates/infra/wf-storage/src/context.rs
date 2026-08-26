use crate::adapter::adapter_impls::{
    AgentExecutionStorage, AgentLoopStorage, AgentProfileStorage, CheckpointStorage,
    MessageStorage, MetricsStorage, NodeTemplateStorage, ScriptStorage, TaskStorage,
    ToolDefinitionStorage, ToolStorage, TriggerExecutionStorage, TriggerStorage,
    TriggerTemplateStorage, UserInteractionStorage, VariableStorage, WorkflowExecutionStorage,
    WorkflowStorage,
};
use crate::backend::StorageBackend;
use crate::decorator::instrumented::{InstrumentedStore, StorageMetrics};
#[cfg(any(feature = "sqlite", feature = "postgres"))]
use crate::error::StorageError;
use crate::store::memory::MemoryStorage;
#[cfg(feature = "postgres")]
use crate::store::postgres::PostgresStorage;

pub struct StorageContext {
    pub workflow: WorkflowStorage<StorageBackend>,
    pub workflow_execution: WorkflowExecutionStorage<StorageBackend>,
    pub checkpoint: CheckpointStorage<StorageBackend>,
    pub task: TaskStorage<StorageBackend>,
    pub agent_loop: AgentLoopStorage<StorageBackend>,
    pub agent_execution: AgentExecutionStorage<StorageBackend>,
    pub agent_profile: AgentProfileStorage<StorageBackend>,
    pub trigger_template: TriggerTemplateStorage<StorageBackend>,
    pub trigger: TriggerStorage<StorageBackend>,
    pub trigger_execution: TriggerExecutionStorage<StorageBackend>,
    pub user_interaction: UserInteractionStorage<StorageBackend>,
    pub tool: ToolStorage<StorageBackend>,
    pub tool_definition: ToolDefinitionStorage<StorageBackend>,
    pub script: ScriptStorage<StorageBackend>,
    pub node_template: NodeTemplateStorage<StorageBackend>,
    pub metrics: MetricsStorage<StorageBackend>,
    pub message: MessageStorage<StorageBackend>,
    pub variable: VariableStorage<StorageBackend>,
}

macro_rules! make_backend {
    ($variant:ident, $name:expr) => {
        StorageBackend::$variant(InstrumentedStore::new(MemoryStorage::new($name)))
    };
}

impl StorageContext {
    pub fn new_memory() -> Self {
        Self {
            workflow: WorkflowStorage::new(make_backend!(Memory, "workflow")),
            workflow_execution: WorkflowExecutionStorage::new(make_backend!(Memory, "execution")),
            checkpoint: CheckpointStorage::new(make_backend!(Memory, "checkpoint")),
            task: TaskStorage::new(make_backend!(Memory, "task")),
            agent_loop: AgentLoopStorage::new(make_backend!(Memory, "agent_loop")),
            agent_execution: AgentExecutionStorage::new(make_backend!(Memory, "agent_execution")),
            agent_profile: AgentProfileStorage::new(make_backend!(Memory, "agent_profile")),
            trigger_template: TriggerTemplateStorage::new(make_backend!(
                Memory,
                "trigger_template"
            )),
            trigger: TriggerStorage::new(make_backend!(Memory, "trigger")),
            trigger_execution: TriggerExecutionStorage::new(make_backend!(
                Memory,
                "trigger_execution"
            )),
            user_interaction: UserInteractionStorage::new(make_backend!(
                Memory,
                "user_interaction"
            )),
            tool: ToolStorage::new(make_backend!(Memory, "tool")),
            tool_definition: ToolDefinitionStorage::new(make_backend!(Memory, "tool_definition")),
            script: ScriptStorage::new(make_backend!(Memory, "script")),
            node_template: NodeTemplateStorage::new(make_backend!(Memory, "node_template")),
            metrics: MetricsStorage::new(make_backend!(Memory, "metrics")),
            message: MessageStorage::new(make_backend!(Memory, "message")),
            variable: VariableStorage::new(make_backend!(Memory, "variable")),
        }
    }

    #[cfg(feature = "sqlite")]
    pub async fn new_sqlite(path: &str) -> Result<Self, StorageError> {
        macro_rules! sqlite_backend {
            ($table:expr) => {
                StorageBackend::new_sqlite(path, $table).await?
            };
        }
        Ok(Self {
            workflow: WorkflowStorage::new(sqlite_backend!("workflow")),
            workflow_execution: WorkflowExecutionStorage::new(sqlite_backend!("execution")),
            checkpoint: CheckpointStorage::new(sqlite_backend!("checkpoint")),
            task: TaskStorage::new(sqlite_backend!("task")),
            agent_loop: AgentLoopStorage::new(sqlite_backend!("agent_loop")),
            agent_execution: AgentExecutionStorage::new(sqlite_backend!("agent_execution")),
            agent_profile: AgentProfileStorage::new(sqlite_backend!("agent_profile")),
            trigger_template: TriggerTemplateStorage::new(sqlite_backend!("trigger_template")),
            trigger: TriggerStorage::new(sqlite_backend!("trigger")),
            trigger_execution: TriggerExecutionStorage::new(sqlite_backend!("trigger_execution")),
            user_interaction: UserInteractionStorage::new(sqlite_backend!("user_interaction")),
            tool: ToolStorage::new(sqlite_backend!("tool")),
            tool_definition: ToolDefinitionStorage::new(sqlite_backend!("tool_definition")),
            script: ScriptStorage::new(sqlite_backend!("script")),
            node_template: NodeTemplateStorage::new(sqlite_backend!("node_template")),
            metrics: MetricsStorage::new(sqlite_backend!("metrics")),
            message: MessageStorage::new(sqlite_backend!("message")),
            variable: VariableStorage::new(sqlite_backend!("variable")),
        })
    }

    #[cfg(feature = "postgres")]
    pub async fn new_postgres(connection_string: &str) -> Result<Self, StorageError> {
        macro_rules! pg_backend {
            ($table:expr) => {
                StorageBackend::Postgres(InstrumentedStore::new(
                    PostgresStorage::new(connection_string, $table).await?,
                ))
            };
        }
        Ok(Self {
            workflow: WorkflowStorage::new(pg_backend!("workflow")),
            workflow_execution: WorkflowExecutionStorage::new(pg_backend!("execution")),
            checkpoint: CheckpointStorage::new(pg_backend!("checkpoint")),
            task: TaskStorage::new(pg_backend!("task")),
            agent_loop: AgentLoopStorage::new(pg_backend!("agent_loop")),
            agent_execution: AgentExecutionStorage::new(pg_backend!("agent_execution")),
            agent_profile: AgentProfileStorage::new(pg_backend!("agent_profile")),
            trigger_template: TriggerTemplateStorage::new(pg_backend!("trigger_template")),
            trigger: TriggerStorage::new(pg_backend!("trigger")),
            trigger_execution: TriggerExecutionStorage::new(pg_backend!("trigger_execution")),
            user_interaction: UserInteractionStorage::new(pg_backend!("user_interaction")),
            tool: ToolStorage::new(pg_backend!("tool")),
            tool_definition: ToolDefinitionStorage::new(pg_backend!("tool_definition")),
            script: ScriptStorage::new(pg_backend!("script")),
            node_template: NodeTemplateStorage::new(pg_backend!("node_template")),
            metrics: MetricsStorage::new(pg_backend!("metrics")),
            message: MessageStorage::new(pg_backend!("message")),
            variable: VariableStorage::new(pg_backend!("variable")),
        })
    }
}

impl StorageContext {
    /// Aggregate operation counters of every store backend (save/load/delete
    /// /list/exists/clear/batch), exported to the metrics sampler.
    pub fn ops_snapshot(&self) -> StorageMetrics {
        let mut total = StorageMetrics::default();
        let backends = [
            self.workflow.store().op_metrics(),
            self.workflow_execution.store().op_metrics(),
            self.checkpoint.store().op_metrics(),
            self.task.store().op_metrics(),
            self.agent_loop.store().op_metrics(),
            self.agent_execution.store().op_metrics(),
            self.agent_profile.store().op_metrics(),
            self.trigger_template.store().op_metrics(),
            self.trigger.store().op_metrics(),
            self.trigger_execution.store().op_metrics(),
            self.user_interaction.store().op_metrics(),
            self.tool.store().op_metrics(),
            self.tool_definition.store().op_metrics(),
            self.script.store().op_metrics(),
            self.node_template.store().op_metrics(),
            self.metrics.inner().op_metrics(),
            self.message.store().op_metrics(),
            self.variable.store().op_metrics(),
        ];
        for backend in backends {
            total = total.accumulate(backend);
        }
        total
    }
}
