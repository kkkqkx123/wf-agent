use crate::adapter::adapter_impls::{
    AgentExecutionStorage, AgentLoopStorage, AgentProfileStorage, CheckpointStorage,
    FileCheckpointStorage, HookTemplateStorage, MetricsStorage, NodeTemplateStorage, ScriptStorage,
    TaskStorage, ToolStorage, TriggerExecutionStorage, TriggerStorage, UserInteractionStorage,
    WorkflowExecutionStorage, WorkflowStorage,
};
use crate::backend::StorageBackend;
use crate::error::StorageError;
use crate::note::memory::MemoryNoteStore;
use crate::store::memory::MemoryStorage;
#[cfg(feature = "postgres")]
use crate::store::postgres::PostgresStorage;
#[cfg(feature = "sqlite")]
use crate::store::sqlite::SqliteStorage;

pub struct StorageContext {
    pub workflow: WorkflowStorage<StorageBackend>,
    pub workflow_execution: WorkflowExecutionStorage<StorageBackend>,
    pub checkpoint: CheckpointStorage<StorageBackend>,
    pub task: TaskStorage<StorageBackend>,
    pub agent_loop: AgentLoopStorage<StorageBackend>,
    pub agent_execution: AgentExecutionStorage<StorageBackend>,
    pub agent_profile: AgentProfileStorage<StorageBackend>,
    pub file_checkpoint: FileCheckpointStorage<StorageBackend>,
    pub trigger: TriggerStorage<StorageBackend>,
    pub trigger_execution: TriggerExecutionStorage<StorageBackend>,
    pub user_interaction: UserInteractionStorage<StorageBackend>,
    pub tool: ToolStorage<StorageBackend>,
    pub script: ScriptStorage<StorageBackend>,
    pub node_template: NodeTemplateStorage<StorageBackend>,
    pub hook_template: HookTemplateStorage<StorageBackend>,
    pub metrics: MetricsStorage<StorageBackend>,
    pub note_store: MemoryNoteStore,
}

macro_rules! make_backend {
    ($variant:ident, $name:expr) => {
        StorageBackend::$variant(MemoryStorage::new($name))
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
            file_checkpoint: FileCheckpointStorage::new(make_backend!(Memory, "file_checkpoint")),
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
            script: ScriptStorage::new(make_backend!(Memory, "script")),
            node_template: NodeTemplateStorage::new(make_backend!(Memory, "node_template")),
            hook_template: HookTemplateStorage::new(make_backend!(Memory, "hook_template")),
            metrics: MetricsStorage::new(make_backend!(Memory, "metrics")),
            note_store: MemoryNoteStore::new(),
        }
    }

    #[cfg(feature = "sqlite")]
    pub async fn new_sqlite(path: &str) -> Result<Self, StorageError> {
        macro_rules! sqlite_backend {
            ($table:expr) => {
                StorageBackend::Sqlite(SqliteStorage::new(path, $table).await?)
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
            file_checkpoint: FileCheckpointStorage::new(sqlite_backend!("file_checkpoint")),
            trigger: TriggerStorage::new(sqlite_backend!("trigger")),
            trigger_execution: TriggerExecutionStorage::new(sqlite_backend!("trigger_execution")),
            user_interaction: UserInteractionStorage::new(sqlite_backend!("user_interaction")),
            tool: ToolStorage::new(sqlite_backend!("tool")),
            script: ScriptStorage::new(sqlite_backend!("script")),
            node_template: NodeTemplateStorage::new(sqlite_backend!("node_template")),
            hook_template: HookTemplateStorage::new(sqlite_backend!("hook_template")),
            metrics: MetricsStorage::new(sqlite_backend!("metrics")),
            note_store: MemoryNoteStore::new(),
        })
    }

    #[cfg(feature = "postgres")]
    pub async fn new_postgres(connection_string: &str) -> Result<Self, StorageError> {
        macro_rules! pg_backend {
            ($table:expr) => {
                StorageBackend::Postgres(PostgresStorage::new(connection_string, $table).await?)
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
            file_checkpoint: FileCheckpointStorage::new(pg_backend!("file_checkpoint")),
            trigger: TriggerStorage::new(pg_backend!("trigger")),
            trigger_execution: TriggerExecutionStorage::new(pg_backend!("trigger_execution")),
            user_interaction: UserInteractionStorage::new(pg_backend!("user_interaction")),
            tool: ToolStorage::new(pg_backend!("tool")),
            script: ScriptStorage::new(pg_backend!("script")),
            node_template: NodeTemplateStorage::new(pg_backend!("node_template")),
            hook_template: HookTemplateStorage::new(pg_backend!("hook_template")),
            metrics: MetricsStorage::new(pg_backend!("metrics")),
            note_store: MemoryNoteStore::new(),
        })
    }
}
