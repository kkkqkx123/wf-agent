# Runtime Persistence & Recovery Plan

Date: 2026-07-27

## 1. Current State

### Persistence Architecture

```
Path A: Entity Persistence (wf-storage adapter)
┌────────────────────────────────────────────────────────┐
│ WorkflowExecution → WorkflowExecutionStorage<S>        │  ✅ Full type persisted
│ AgentLoop         → AgentLoopStorage<S>                │  ⚠️ Metadata only (no iteration_history/context)
│ Task              → TaskStorage<S>                     │  ⚠️ Metadata only (no payload)
└────────────────────────────────────────────────────────┘

Path B: Checkpoint Persistence (wf-checkpoint blob)
┌────────────────────────────────────────────────────────┐
│ WorkflowExecutionStateSnapshot → StorageBackedStateManager │
│ AgentStateSnapshot            → StorageBackedStateManager │
│ Returns: thin Entity { id, status }                    │  ❌ Full state lost at restore boundary
└────────────────────────────────────────────────────────┘
```

### What Works
- Save path: Runtime → serialization → storage (complete)
- Checkpoint save with delta chain (full/delta strategy)
- Storage backend abstraction (Memory/SQLite/Postgres)
- Typed entity persistence for `WorkflowExecution`

### What's Broken
- Restore path: Storage → deserialization → **cannot reconstruct full runtime state**
- `AgentExecution` has no storage adapter (only `AgentLoopStorageMetadata`)
- Checkpoint restore returns only `{ id, status }`, discarding all execution context
- No `From<StateSnapshot>` conversion to reconstruct runtime types
- Recovery loop not wired (no scanner, no orchestrator)

## 2. Design Decisions

### 2.1 Checkpoint Snapshot → Runtime Type Conversion

The checkpoint snapshots (`WorkflowExecutionStateSnapshot`, `AgentStateSnapshot`) contain all fields needed to reconstruct runtime state. The conversion should be a `From` impl on the runtime type.

**Key insight**: `WorkflowExecution` (runtime type) and `WorkflowExecutionStateSnapshot` (checkpoint type) have ~90% field overlap. The snapshot is essentially a "flattened" version where structured types (like `ForkJoinContext`) are serialized as `serde_json::Value`.

**Decision**: Add `From<WorkflowExecutionStateSnapshot> for WorkflowExecution` with explicit field mapping. Where the snapshot stores `serde_json::Value` but the runtime type has a structured field, use `serde_json::from_value` with fallback to `Default`.

### 2.2 AgentExecution Storage

Currently `AgentLoopStorageMetadata` is a flat record (id, definition_id, status, current_iteration, timestamps). The full `AgentExecution` type has richer fields (iteration_history, context, tool_call_count).

**Decision**: Add `AgentExecution` as a storable entity via `make_base_adapter!`. The existing `AgentLoopStorageMetadata` remains for lightweight listing queries, while `AgentExecution` provides full state persistence.

### 2.3 Recovery Loop Design

```
On startup:
1. RecoveryScanner queries execution storage for status = "running"
2. For each incomplete execution:
   a. Load latest checkpoint via CheckpointCoordinator
   b. Convert snapshot → runtime type via From impl
   c. Reconstruct execution context
   d. Resume from current_node_id
3. Report RecoveryScanResult { recovered: Vec<id>, failed: Vec<(id, error)> }
```

## 3. Implementation Phases

### Phase 1: Type Conversions (wf-types)

**Goal**: Enable checkpoint snapshot → runtime type reconstruction.

#### 1.1 Add `From<WorkflowExecutionStateSnapshot> for WorkflowExecution`

File: `crates/wf-types/src/workflow_execution/definition.rs`

```rust
impl From<WorkflowExecutionStateSnapshot> for WorkflowExecution {
    fn from(snapshot: WorkflowExecutionStateSnapshot) -> Self {
        Self {
            id: snapshot.execution_id,
            workflow_id: String::new(),  // restored from checkpoint metadata
            workflow_version: None,
            status: serde_json::from_value(serde_json::json!(&snapshot.status))
                .unwrap_or(WorkflowExecutionStatus::Unknown),
            current_node_id: snapshot.current_node_id,
            graph: None,  // restored from checkpoint if needed
            variables: snapshot.variable_state.variables
                .into_iter()
                .map(|(k, v)| VariableDefinition::from_value(k, v))
                .collect::<Option<_>>(),
            input: snapshot.input,
            output: snapshot.output,
            node_results: snapshot.node_results
                .map(|map| map.into_iter()
                    .filter_map(|(k, v)| NodeExecutionResult::from_value(k, v))
                    .collect()),
            errors: None,
            started_at: snapshot.timestamp,
            completed_at: None,
            error: None,
            execution_type: None,
            fork_join_context: None,  // stored as Value in snapshot
            hierarchy: None,
        }
    }
}
```

#### 1.2 Add `From<AgentStateSnapshot> for AgentExecution`

File: `crates/wf-types/src/agent_execution/definition.rs`

```rust
impl From<AgentStateSnapshot> for AgentExecution {
    fn from(snapshot: AgentStateSnapshot) -> Self {
        Self {
            id: snapshot.agent_loop_id,
            definition_id: String::new(),  // restored from checkpoint metadata
            status: serde_json::from_value(serde_json::json!(&snapshot.status))
                .unwrap_or(AgentExecutionStatus::Unknown),
            current_iteration: snapshot.current_iteration,
            tool_call_count: snapshot.tool_call_count,
            iteration_history: None,  // restored from checkpoint if available
            started_at: snapshot.started_at.unwrap_or(0),
            completed_at: snapshot.completed_at,
            error: snapshot.error,
            context: None,  // restored from checkpoint if available
        }
    }
}
```

#### 1.3 Add `CheckpointStatus` Enum

File: `crates/wf-types/src/checkpoint/base.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointStatus {
    Active,
    Completed,
    Expired,
    Corrupted,
}
```

Replace `status: String` in `CheckpointStorageMetadata` with `status: CheckpointStatus`.

### Phase 2: Restore Output Enrichment (wf-checkpoint)

**Goal**: Make `restore()` return full state, not just `{ id, status }`.

#### 2.1 Change `WorkflowExecutionEntity`

File: `crates/wf-checkpoint/src/coordinator/workflow.rs`

Replace:
```rust
pub struct WorkflowExecutionEntity {
    pub execution_id: String,
    pub status: String,
}
```

With:
```rust
pub struct WorkflowExecutionEntity {
    pub execution_id: String,
    pub status: String,
    pub snapshot: WorkflowExecutionStateSnapshot,
}
```

The `snapshot` field enables callers to reconstruct full runtime state via `From`.

#### 2.2 Change `AgentLoopEntity`

File: `crates/wf-checkpoint/src/coordinator/agent.rs`

Add `snapshot: AgentStateSnapshot` field.

#### 2.3 Update `restore()` Implementations

In both `WorkflowCheckpointCoordinator::restore()` and `AgentCheckpointCoordinator::restore()`, return the full snapshot alongside the entity:

```rust
Ok(WorkflowExecutionEntity {
    execution_id: snapshot.execution_id,
    status: snapshot.status,
    snapshot,  // full state preserved
})
```

### Phase 3: AgentExecution Storage Adapter (wf-storage)

**Goal**: Full `AgentExecution` persistence, not just metadata.

#### 3.1 Add `AgentExecutionListOptions`

File: `crates/wf-storage/src/adapter/agent_execution.rs` (new)

```rust
#[derive(Debug, Clone, Default)]
pub struct AgentExecutionListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub definition_id_filter: Option<String>,
    pub status_filter: Option<String>,
}

impl From<AgentExecutionListOptions> for QueryFilter { ... }

pub trait AgentExecutionStorageAdapter:
    BaseStorageAdapter<wf_types::AgentExecution, AgentExecutionListOptions>
{
    async fn list_by_definition(&self, definition_id: &str)
        -> Result<Vec<wf_types::AgentExecution>, StorageError>;
}
```

#### 3.2 Register in `concrete.rs`

```rust
make_base_adapter!(AgentExecutionStorage, wf_types::AgentExecution, AgentExecutionListOptions);
```

#### 3.3 Add `Entity` impl for `AgentExecution`

File: `crates/wf-storage/src/entity_impl.rs`

```rust
impl Entity for wf_types::AgentExecution {
    type Metadata = Value;
    fn entity_id(&self) -> &str { &self.id }
    fn entity_type() -> &'static str { "agent_execution" }
    fn metadata(&self) -> Self::Metadata {
        serde_json::json!({
            "definitionId": self.definition_id,
            "status": self.status,
            "currentIteration": self.current_iteration,
            "toolCallCount": self.tool_call_count,
        })
    }
}
```

### Phase 4: Recovery Loop (wf-runtime)

**Goal**: Wire startup recovery — scan incomplete executions, restore from checkpoint, resume.

#### 4.1 RecoveryScanner

File: `crates/wf-runtime/src/recovery/scanner.rs` (new)

```rust
pub struct RecoveryScanner<S: Store> {
    execution_storage: EntityStore<S, WorkflowExecution>,
    checkpoint_manager: Arc<dyn CheckpointStateManager>,
}

impl<S: Store> RecoveryScanner<S> {
    pub async fn scan_incomplete(&self) -> Result<Vec<WorkflowExecution>, StorageError> {
        let filter = QueryFilter::new()
            .with_field("status", "running")
            .with_field("status", "paused");
        self.execution_storage.list(Some(&filter)).await
    }
}
```

#### 4.2 RecoveryOrchestrator

File: `crates/wf-runtime/src/recovery/orchestrator.rs` (new)

```rust
pub struct RecoveryOrchestrator {
    scanner: RecoveryScanner,
    checkpoint_coordinator: WorkflowCheckpointCoordinator,
}

pub struct RecoveryResult {
    pub recovered: Vec<String>,
    pub failed: Vec<(String, String)>,
}

impl RecoveryOrchestrator {
    pub async fn recover_all(&self) -> Result<RecoveryResult, RuntimeError> {
        let incomplete = self.scanner.scan_incomplete().await?;
        let mut recovered = Vec::new();
        let mut failed = Vec::new();

        for execution in incomplete {
            match self.recover_one(&execution).await {
                Ok(_) => recovered.push(execution.id),
                Err(e) => failed.push((execution.id, e.to_string())),
            }
        }
        Ok(RecoveryResult { recovered, failed })
    }

    async fn recover_one(&self, execution: &WorkflowExecution) -> Result<(), RuntimeError> {
        // 1. Load latest checkpoint
        // 2. Convert snapshot → WorkflowExecution via From
        // 3. Validate state consistency
        // 4. Resume execution from current_node_id
        todo!()
    }
}
```

#### 4.3 Integrate with Runtime Lifecycle

File: `crates/wf-runtime/src/lifecycle.rs`

After `StorageManager::initialize()`, run recovery:

```rust
pub async fn bootstrap(config: RuntimeConfig) -> RuntimeResult<Runtime> {
    let mut storage = StorageManager::new(config.storage);
    storage.initialize().await?;

    // Recover incomplete executions
    let recovery = RecoveryOrchestrator::new(&storage);
    let result = recovery.recover_all().await?;
    info!("Recovery complete: {} recovered, {} failed", result.recovered.len(), result.failed.len());

    Ok(Runtime { storage, ... })
}
```

### Phase 5: Task Full Entity (wf-types + wf-storage)

**Goal**: Give `Task` a full entity type with payload.

#### 5.1 Define `Task` Entity

File: `crates/wf-types/src/agent_execution/task.rs` (new)

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Task {
    pub id: Id,
    pub task_type: String,
    pub status: String,
    pub payload: serde_json::Value,  // task-specific parameters
    pub result: Option<serde_json::Value>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}
```

#### 5.2 Replace `TaskStorageMetadata` Usage

Gradually migrate from `TaskStorageMetadata` (metadata-only) to `Task` (full entity). The metadata type can remain for backward compatibility during migration.

## 4. Type Changes Summary

| Crate | File | Change |
|-------|------|--------|
| `wf-types` | `workflow_execution/definition.rs` | Add `From<WorkflowExecutionStateSnapshot>` |
| `wf-types` | `agent_execution/definition.rs` | Add `From<AgentStateSnapshot>` |
| `wf-types` | `checkpoint/base.rs` | Add `CheckpointStatus` enum |
| `wf-types` | `agent_execution/task.rs` | New `Task` entity type |
| `wf-checkpoint` | `coordinator/workflow.rs` | Enrich `WorkflowExecutionEntity` with snapshot |
| `wf-checkpoint` | `coordinator/agent.rs` | Enrich `AgentLoopEntity` with snapshot |
| `wf-storage` | `adapter/agent_execution.rs` | New adapter for `AgentExecution` |
| `wf-storage` | `adapter/concrete.rs` | Register `AgentExecutionStorage` |
| `wf-storage` | `entity_impl.rs` | Add `Entity` impl for `AgentExecution` |
| `wf-runtime` | `recovery/scanner.rs` | New recovery scanner |
| `wf-runtime` | `recovery/orchestrator.rs` | New recovery orchestrator |
| `wf-runtime` | `lifecycle.rs` | Integrate recovery into bootstrap |

## 5. Migration Notes

- Phase 1 is pure addition, no breaking changes
- Phase 2 changes `WorkflowExecutionEntity` struct — update all callers
- Phase 3 adds new adapter, existing `AgentLoopStorage` unaffected
- Phase 4 is new module, no existing code changes
- Phase 5 introduces `Task` type alongside existing `TaskStorageMetadata`

## 6. Out of Scope

- Checkpoint garbage collection policy (separate concern)
- Distributed recovery (multi-node scenario)
- Checkpoint encryption at rest
- Migration from TS checkpoint format
