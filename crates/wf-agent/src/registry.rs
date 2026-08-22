use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use dashmap::DashMap;
use wf_common::gate::GateStats;
use wf_execution_shared::types::execution_entity::ExecutionStatus;
use wf_tools::callback::AgentLoopOutput;
use wf_types::Id;

use crate::capacity::AgentCapacityGate;
use crate::entity::AgentLoopEntity;
use crate::error::{AgentError, AgentResult};

/// Default sub-agent recursion depth limit (root = depth 0; a chain of up to
/// 8 nested child loops is allowed). Aligned with Codex's spawn depth default.
pub const DEFAULT_MAX_SUB_AGENT_DEPTH: u32 = 8;

/// Query filter for agent loop registry lookups.
#[derive(Debug, Clone, Default)]
pub struct AgentExecutionFilter {
    pub status: Option<ExecutionStatus>,
    pub parent_execution_id: Option<Id>,
    pub agent_id: Option<Id>,
}

/// Immutable summary of an agent loop execution record.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentExecutionRecord {
    pub execution_id: Id,
    pub status: ExecutionStatus,
    pub current_iteration: u32,
    pub tool_call_count: u32,
    pub start_time: i64,
    pub end_time: Option<i64>,
    pub error: Option<String>,
    pub parent_execution_id: Option<Id>,
}

impl AgentExecutionRecord {
    pub async fn from_entity(entity: &AgentLoopEntity) -> Self {
        let state = entity.state.read().await;
        Self {
            execution_id: entity.id().clone(),
            status: state.status(),
            current_iteration: state.current_iteration(),
            tool_call_count: state.tool_call_count(),
            start_time: state.start_time(),
            end_time: state.end_time(),
            error: state.error().map(String::from),
            parent_execution_id: entity.parent_execution_id().cloned(),
        }
    }
}

/// In-memory registry of active agent loop executions with the query
/// interface. Enforces the execution capacity
/// gate: a global concurrent-execution limit and a sub-agent depth
/// limit, aligned with Codex's `reserve_spawn_slot` + `session_depth`.
pub struct AgentLoopRegistry {
    entities: DashMap<Id, Arc<AgentLoopEntity>>,
    /// Terminal results of finished executions, keyed by execution id.
    /// Written by the spawning path, taken by `query_execution_status` and
    /// cleared together with the entity by `cleanup_terminated`.
    results: DashMap<Id, AgentLoopOutput>,
    /// Join handles of background execution tasks, keyed by execution id.
    /// Used to abort a still-running task on cancel; removed by the task
    /// itself on completion.
    tasks: DashMap<Id, tokio::task::JoinHandle<()>>,
    /// Capacity gate: `register` acquires a permit and rejects beyond
    /// `max_concurrent` with `AgentError::ExecutionLimitReached`. The permit
    /// lives in the entity and is released when the execution reaches a
    /// terminal state.
    gate: Arc<AgentCapacityGate>,
    /// Maximum sub-agent recursion depth (root = depth 0). `depth_allowed`
    /// rejects a child whose resolved depth would exceed the limit.
    max_sub_agent_depth: AtomicU32,
}

impl Default for AgentLoopRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentLoopRegistry {
    pub fn new() -> Self {
        let max_concurrent = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        Self {
            entities: DashMap::new(),
            results: DashMap::new(),
            tasks: DashMap::new(),
            gate: Arc::new(AgentCapacityGate::new(max_concurrent)),
            max_sub_agent_depth: AtomicU32::new(DEFAULT_MAX_SUB_AGENT_DEPTH),
        }
    }

    /// Builder-style concurrent-execution limit (must be set before any
    /// execution is registered).
    pub fn with_max_concurrent(self, max: usize) -> Self {
        self.set_max_concurrent(max);
        self
    }

    /// Builder-style sub-agent depth limit.
    pub fn with_max_sub_agent_depth(self, max: u32) -> Self {
        self.set_max_sub_agent_depth(max);
        self
    }

    /// Reconfigure the concurrent-execution limit in place. Intended for
    /// pre-start configuration; existing permits keep their original gate.
    pub fn set_max_concurrent(&self, max: usize) {
        self.gate.set_max_concurrent(max);
    }

    /// Reconfigure the sub-agent depth limit in place.
    pub fn set_max_sub_agent_depth(&self, max: u32) {
        self.max_sub_agent_depth.store(max, Ordering::Relaxed);
    }

    pub fn max_concurrent(&self) -> usize {
        self.gate.max_concurrent()
    }

    pub fn available_permits(&self) -> usize {
        self.gate.available_permits()
    }

    /// Shared reference to the capacity gate so external consumers (e.g.
    /// runtime metrics) can observe live stats without owning the gate.
    pub fn capacity_gate(&self) -> Arc<AgentCapacityGate> {
        self.gate.clone()
    }

    /// Snapshot of the capacity gate counters for observability.
    pub fn gate_stats(&self) -> GateStats {
        self.gate.stats()
    }

    pub fn max_sub_agent_depth(&self) -> u32 {
        self.max_sub_agent_depth.load(Ordering::Relaxed)
    }

    /// Register an execution under the capacity gate: registering beyond
    /// `max_concurrent` returns `AgentError::ExecutionLimitReached`. Replacing
    /// an already-registered id (spawn placeholder → real entity) moves the
    /// existing permit so the replacement neither consumes a new slot nor
    /// releases early.
    pub fn register(&self, entity: Arc<AgentLoopEntity>) -> AgentResult<()> {
        let id = entity.id().clone();
        if let Some(existing) = self.entities.get(&id) {
            let permit = existing.take_gate_permit();
            drop(existing);
            entity.set_gate_permit(permit);
            self.entities.insert(id, entity);
            return Ok(());
        }
        let max = self.max_concurrent();
        let permit = self.gate.try_acquire().map_err(|_| {
            AgentError::ExecutionLimitReached(format!(
                "concurrent execution limit {max} reached"
            ))
        })?;
        entity.set_gate_permit(Some(permit));
        self.entities.insert(id, entity);
        Ok(())
    }

    /// Whether a child execution at `parent_depth` (0 for a root run) fits
    /// within the sub-agent depth limit.
    pub fn depth_allowed(&self, parent_depth: u32) -> bool {
        parent_depth.saturating_add(1) <= self.max_sub_agent_depth()
    }

    pub fn unregister(&self, id: &Id) -> bool {
        self.entities.remove(id).is_some()
    }

    pub fn get(&self, id: &Id) -> Option<Arc<AgentLoopEntity>> {
        self.entities.get(id).map(|e| e.clone())
    }

    pub fn has(&self, id: &Id) -> bool {
        self.entities.contains_key(id)
    }

    pub fn size(&self) -> usize {
        self.entities.len()
    }

    pub fn clear(&self) {
        self.entities.clear();
        self.results.clear();
        self.tasks.clear();
    }

    pub fn get_all_ids(&self) -> Vec<Id> {
        self.entities.iter().map(|e| e.id().clone()).collect()
    }

    /// Record the terminal output of a finished execution.
    pub fn store_result(&self, id: Id, output: AgentLoopOutput) {
        self.results.insert(id, output);
    }

    /// Read the stored output without removing it.
    pub fn result(&self, id: &Id) -> Option<AgentLoopOutput> {
        self.results.get(id).map(|e| e.clone())
    }

    /// Take the stored output, removing it from the result slot.
    pub fn take_result(&self, id: &Id) -> Option<AgentLoopOutput> {
        self.results.remove(id).map(|(_, v)| v)
    }

    /// Register the background task driving `id`. The task removes its own
    /// handle when it finishes; `abort_task` covers the cancel path.
    pub fn register_task(&self, id: Id, handle: tokio::task::JoinHandle<()>) {
        self.tasks.insert(id, handle);
    }

    /// Abort the background task of `id` (cancel path) and forget its handle.
    pub fn abort_task(&self, id: &Id) -> bool {
        if let Some((_, handle)) = self.tasks.remove(id) {
            handle.abort();
            return true;
        }
        false
    }

    /// Forget the background task handle of `id` without aborting it
    /// (normal completion path).
    pub fn unregister_task(&self, id: &Id) -> bool {
        self.tasks.remove(id).is_some()
    }

    pub async fn get_by_status(&self, status: ExecutionStatus) -> Vec<Arc<AgentLoopEntity>> {
        let mut result = Vec::new();
        for entry in self.entities.iter() {
            let entity = entry.clone();
            if entity.state.read().await.status() == status {
                result.push(entity);
            }
        }
        result
    }

    /// Query entities with an optional filter. `AgentExecutionFilter::agent_id`
    /// matches the agent definition id (`definition_id`), not the per-run
    /// loop id, so all runs of a definition are returned.
    pub async fn query(&self, filter: &AgentExecutionFilter) -> Vec<Arc<AgentLoopEntity>> {
        let mut results: Vec<Arc<AgentLoopEntity>> = self
            .entities
            .iter()
            .map(|e| e.clone())
            .filter(|e| {
                if let Some(agent_id) = &filter.agent_id {
                    if e.definition_id() != agent_id {
                        return false;
                    }
                }
                if let Some(parent_id) = &filter.parent_execution_id {
                    if e.parent_execution_id() != Some(parent_id) {
                        return false;
                    }
                }
                true
            })
            .collect();

        if let Some(status) = &filter.status {
            let mut kept = Vec::with_capacity(results.len());
            for entity in results {
                if entity.state.read().await.status() == *status {
                    kept.push(entity);
                }
            }
            results = kept;
        }

        results
    }

    /// Execution records for a given agent definition id. Records are
    /// sorted newest first.
    pub async fn execution_records(&self, definition_id: &Id) -> Vec<AgentExecutionRecord> {
        let ids: Vec<Id> = self
            .entities
            .iter()
            .filter(|e| e.definition_id() == definition_id)
            .map(|e| e.id().clone())
            .collect();

        let mut records = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(entity) = self.get(&id) {
                records.push(AgentExecutionRecord::from_entity(&entity).await);
            }
        }
        records.sort_by_key(|r| std::cmp::Reverse(r.start_time));
        records
    }

    /// Remove all terminated (completed/failed/cancelled) executions
    /// together with their result slots and task handles.
    pub async fn cleanup_terminated(&self) -> usize {
        let mut removed = 0;
        let ids: Vec<Id> = self.entities.iter().map(|e| e.id().clone()).collect();
        for id in ids {
            let Some(entity) = self.get(&id) else {
                continue;
            };
            let status = entity.state.read().await.status();
            if matches!(
                status,
                ExecutionStatus::Completed
                    | ExecutionStatus::Failed
                    | ExecutionStatus::Cancelled
                    | ExecutionStatus::Stopped
            ) {
                self.entities.remove(&id);
                self.results.remove(&id);
                self.tasks.remove(&id);
                removed += 1;
            }
        }
        removed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    async fn make_entity(id: &str, status: ExecutionStatus) -> Arc<AgentLoopEntity> {
        let entity = Arc::new(AgentLoopEntity::new(Id::from(id.to_string())));
        {
            let mut state = entity.state.write().await;
            match status {
                ExecutionStatus::Running => state.start().unwrap(),
                ExecutionStatus::Completed => {
                    state.start().unwrap();
                    state.complete().unwrap();
                }
                ExecutionStatus::Failed => {
                    state.start().unwrap();
                    state.fail("boom".to_string()).unwrap();
                }
                _ => {}
            }
        }
        entity
    }

    #[tokio::test]
    async fn test_register_and_query() {
        let registry = AgentLoopRegistry::new();
        let e1 = make_entity("a1", ExecutionStatus::Running).await;
        let e2 = make_entity("a2", ExecutionStatus::Completed).await;
        registry.register(e1.clone()).unwrap();
        registry.register(e2.clone()).unwrap();

        assert_eq!(registry.size(), 2);
        assert!(registry.has(&Id::from("a1".to_string())));

        let running = registry
            .query(&AgentExecutionFilter {
                status: Some(ExecutionStatus::Running),
                ..Default::default()
            })
            .await;
        assert_eq!(running.len(), 1);
        assert_eq!(running[0].id().as_str(), "a1");

        let by_id = registry
            .query(&AgentExecutionFilter {
                agent_id: Some(Id::from("a2".to_string())),
                ..Default::default()
            })
            .await;
        assert_eq!(by_id.len(), 1);
        assert_eq!(by_id[0].id().as_str(), "a2");

        assert!(registry.unregister(&Id::from("a1".to_string())));
        assert_eq!(registry.size(), 1);
    }

    #[tokio::test]
    async fn test_execution_records_by_agent_id() {
        let registry = AgentLoopRegistry::new().with_max_concurrent(4);
        registry
            .register(make_entity("agent-1/run-1", ExecutionStatus::Running).await)
            .unwrap();
        registry
            .register(make_entity("agent-1/run-2", ExecutionStatus::Completed).await)
            .unwrap();
        registry
            .register(make_entity("agent-2/run-1", ExecutionStatus::Running).await)
            .unwrap();

        let records = registry
            .execution_records(&Id::from("agent-1/run-1".to_string()))
            .await;
        assert_eq!(records.len(), 1);
        assert_eq!(
            records[0].execution_id,
            Id::from("agent-1/run-1".to_string())
        );

        let all = registry.get_all_ids();
        assert_eq!(all.len(), 3);
    }

    #[tokio::test]
    async fn test_cleanup_terminated() {
        let registry = AgentLoopRegistry::new().with_max_concurrent(4);
        registry
            .register(make_entity("a1", ExecutionStatus::Completed).await)
            .unwrap();
        registry
            .register(make_entity("a2", ExecutionStatus::Running).await)
            .unwrap();
        registry
            .register(make_entity("a3", ExecutionStatus::Failed).await)
            .unwrap();

        let removed = registry.cleanup_terminated().await;
        assert_eq!(removed, 2);
        assert_eq!(registry.size(), 1);
    }

    fn output(id: &str, result: Value) -> AgentLoopOutput {
        AgentLoopOutput {
            agent_loop_id: Id::from(id.to_string()),
            result,
            iterations: 1,
            conversation: Vec::new(),
        }
    }

    #[tokio::test]
    async fn test_result_slot_store_take_and_cleanup() {
        let registry = AgentLoopRegistry::new();
        let id = Id::from("run-1".to_string());
        registry.store_result(id.clone(), output("run-1", Value::from("done")));

        assert_eq!(
            registry.result(&id).expect("result present").result,
            Value::from("done")
        );
        // take removes the slot.
        assert!(registry.take_result(&id).is_some());
        assert!(registry.take_result(&id).is_none());

        // Terminal entity cleanup also clears the result slot.
        registry.store_result(id.clone(), output("run-1", Value::from("done")));
        registry
            .register(make_entity("run-1", ExecutionStatus::Completed).await)
            .unwrap();
        assert_eq!(registry.cleanup_terminated().await, 1);
        assert!(registry.result(&id).is_none());
    }

    #[tokio::test]
    async fn test_task_handle_register_and_abort() {
        let registry = AgentLoopRegistry::new();
        let id = Id::from("run-2".to_string());
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let handle = tokio::spawn(async move {
            let _ = rx.await;
        });
        registry.register_task(id.clone(), handle);

        assert!(registry.abort_task(&id));
        // Aborting again reports false (handle forgotten).
        assert!(!registry.abort_task(&id));
        let _ = tx;
    }

    /// Registering beyond the concurrent limit is rejected with
    /// `ExecutionLimitReached`; re-registering an existing id (spawn
    /// placeholder → real entity) does not consume a second slot.
    #[tokio::test]
    async fn test_capacity_gate_rejects_overflow_and_permits_replace() {
        let registry = AgentLoopRegistry::new().with_max_concurrent(2);
        registry
            .register(make_entity("r1", ExecutionStatus::Running).await)
            .unwrap();
        registry
            .register(make_entity("r2", ExecutionStatus::Running).await)
            .unwrap();

        let err = registry
            .register(make_entity("r3", ExecutionStatus::Running).await)
            .unwrap_err();
        assert!(
            matches!(err, AgentError::ExecutionLimitReached(_)),
            "overflow must surface as ExecutionLimitReached: {err}"
        );

        // Replacing an existing id is not an overflow.
        registry
            .register(make_entity("r1", ExecutionStatus::Running).await)
            .expect("replace of a registered id must succeed");
        assert_eq!(registry.size(), 2);
    }

    /// Depth gate — a child at `parent_depth + 1` fits until the limit,
    /// beyond which it is rejected.
    #[tokio::test]
    async fn test_depth_gate_rejects_beyond_limit() {
        let registry = AgentLoopRegistry::new().with_max_sub_agent_depth(2);
        assert!(registry.depth_allowed(0), "root parent -> depth 1 ok");
        assert!(registry.depth_allowed(1), "depth 1 parent -> depth 2 ok");
        assert!(
            !registry.depth_allowed(2),
            "depth 2 parent -> depth 3 exceeds the limit"
        );
        assert_eq!(registry.max_sub_agent_depth(), 2);
    }

    /// Terminal transitions release the capacity permit: a finished
    /// execution must not keep occupying a slot (the historical
    /// `entities.len()` gate bug).
    #[tokio::test]
    async fn test_terminal_transition_releases_capacity() {
        use crate::coordinator::state_transitor::AgentLoopStateTransitor;

        let registry = AgentLoopRegistry::new().with_max_concurrent(2);
        let e1 = make_entity("r1", ExecutionStatus::Running).await;
        let e2 = make_entity("r2", ExecutionStatus::Running).await;
        registry.register(e1.clone()).unwrap();
        registry.register(e2.clone()).unwrap();
        assert_eq!(registry.available_permits(), 0);

        AgentLoopStateTransitor::complete_agent_loop(&e1, None)
            .await
            .unwrap();
        assert_eq!(registry.available_permits(), 1);

        // The freed slot admits a new execution.
        registry
            .register(make_entity("r3", ExecutionStatus::Running).await)
            .expect("released slot must admit a new execution");
    }

    /// Cancelling an execution releases the permit as well, so an aborted
    /// run never permanently consumes capacity.
    #[tokio::test]
    async fn test_cancel_releases_capacity() {
        use crate::coordinator::state_transitor::AgentLoopStateTransitor;

        let registry = AgentLoopRegistry::new().with_max_concurrent(1);
        let e1 = make_entity("c1", ExecutionStatus::Running).await;
        registry.register(e1.clone()).unwrap();
        assert_eq!(registry.available_permits(), 0);

        AgentLoopStateTransitor::cancel_agent_loop(&e1, None)
            .await
            .unwrap();
        assert_eq!(registry.available_permits(), 1);

        registry
            .register(make_entity("c2", ExecutionStatus::Running).await)
            .expect("cancelled slot must be reusable");
    }

    /// A spawn placeholder that is removed before it is replaced (the run
    /// failed before the coordinator registered the real entity) releases
    /// its permit, so the capacity slot is not leaked.
    #[tokio::test]
    async fn test_placeholder_removal_releases_capacity() {
        let registry = AgentLoopRegistry::new().with_max_concurrent(1);
        registry
            .register(Arc::new(AgentLoopEntity::new(Id::from("p1".to_string()))))
            .unwrap();
        assert_eq!(registry.available_permits(), 0);

        assert!(registry.unregister(&Id::from("p1".to_string())));
        assert_eq!(registry.available_permits(), 1);
    }

    /// Replacing a spawn placeholder with the real entity moves the permit:
    /// the replacement neither acquires a second slot nor releases early,
    /// and the moved permit is released by the real entity's terminal
    /// transition.
    #[tokio::test]
    async fn test_placeholder_replace_moves_permit() {
        use crate::coordinator::state_transitor::AgentLoopStateTransitor;

        let registry = AgentLoopRegistry::new().with_max_concurrent(1);
        let placeholder = Arc::new(AgentLoopEntity::new(Id::from("p2".to_string())));
        registry.register(placeholder.clone()).unwrap();
        assert_eq!(registry.available_permits(), 0);

        let real = make_entity("p2", ExecutionStatus::Running).await;
        registry.register(real.clone()).unwrap();
        // Same id: the permit is moved, capacity still consumed exactly once.
        assert_eq!(registry.available_permits(), 0);

        AgentLoopStateTransitor::complete_agent_loop(&real, None)
            .await
            .unwrap();
        assert_eq!(registry.available_permits(), 1);
    }
}
