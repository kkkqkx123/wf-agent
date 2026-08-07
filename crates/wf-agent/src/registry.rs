use std::sync::Arc;

use dashmap::DashMap;
use wf_execution_shared::types::execution_entity::ExecutionStatus;
use wf_types::Id;

use crate::entity::AgentLoopEntity;

/// Query filter for agent loop registry lookups.
#[derive(Debug, Clone, Default)]
pub struct AgentExecutionFilter {
    pub status: Option<ExecutionStatus>,
    pub parent_execution_id: Option<Id>,
    pub agent_id: Option<Id>,
}

/// Immutable summary of an agent loop execution, aligned with the TS
/// agent-execution-registry record shape.
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

/// In-memory registry of active agent loop executions, providing the query
/// interface of the TS agent-loop-registry.
#[derive(Default)]
pub struct AgentLoopRegistry {
    entities: DashMap<Id, Arc<AgentLoopEntity>>,
}

impl AgentLoopRegistry {
    pub fn new() -> Self {
        Self {
            entities: DashMap::new(),
        }
    }

    pub fn register(&self, entity: Arc<AgentLoopEntity>) {
        self.entities.insert(entity.id().clone(), entity);
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
    }

    pub fn get_all_ids(&self) -> Vec<Id> {
        self.entities.iter().map(|e| e.id().clone()).collect()
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

    /// Execution records for a given agent definition id (the TS registry
    /// query by agent_id). Records are sorted newest first.
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

    /// Remove all terminated (completed/failed/cancelled) executions.
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
                removed += 1;
            }
        }
        removed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn make_entity(id: &str, status: ExecutionStatus) -> Arc<AgentLoopEntity> {
        let entity = Arc::new(AgentLoopEntity::new(Id::from(id.to_string())));
        {
            let mut state = entity.state.write().await;
            match status {
                ExecutionStatus::Running => state.start(),
                ExecutionStatus::Completed => {
                    state.start();
                    state.complete();
                }
                ExecutionStatus::Failed => {
                    state.start();
                    state.fail("boom".to_string());
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
        registry.register(e1.clone());
        registry.register(e2.clone());

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
        let registry = AgentLoopRegistry::new();
        registry.register(make_entity("agent-1/run-1", ExecutionStatus::Running).await);
        registry.register(make_entity("agent-1/run-2", ExecutionStatus::Completed).await);
        registry.register(make_entity("agent-2/run-1", ExecutionStatus::Running).await);

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
        let registry = AgentLoopRegistry::new();
        registry.register(make_entity("a1", ExecutionStatus::Completed).await);
        registry.register(make_entity("a2", ExecutionStatus::Running).await);
        registry.register(make_entity("a3", ExecutionStatus::Failed).await);

        let removed = registry.cleanup_terminated().await;
        assert_eq!(removed, 2);
        assert_eq!(registry.size(), 1);
    }
}
