use std::sync::Arc;

use dashmap::DashMap;
use serde_json::Value;

use wf_tools::callback::{AgentLoopConfig, AgentLoopInput, AgentLoopOutput};

use crate::entity::AgentLoopEntity;
use crate::error::{AgentError, AgentResult};

/// Callback that runs a child agent loop (usually backed by
/// AgentLoopExecutor).
pub type AgentExecutorCallback = Arc<
    dyn Fn(
            AgentLoopConfig,
            AgentLoopInput,
        ) -> futures::future::BoxFuture<'static, AgentResult<AgentLoopOutput>>
        + Send
        + Sync,
>;

/// Configuration for a triggered (nested) agent execution, mirroring TS
/// TriggeredAgentExecutionConfig.
#[derive(Clone)]
pub struct TriggeredAgentExecutionConfig {
    /// Parent execution entity that triggered the child agent.
    pub parent: Arc<AgentLoopEntity>,
    /// Variable name on the parent into which the child result is written.
    pub result_variable: String,
    /// Whether to wait for completion (sync vs async fire-and-forget).
    pub wait_for_completion: bool,
    /// Max child execution time in ms.
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TriggeredTaskSubmission {
    pub task_id: String,
    pub status: String,
    pub submit_time: i64,
}

/// Manages triggered (nested) agent loop executions started from a trigger
/// event. Children are registered on the parent entity and their results are
/// written back into the parent's variable snapshots; a failing child never
/// fails the parent.
pub struct TriggeredAgentExecutionManager {
    executor: AgentExecutorCallback,
    pending_task_ids: std::sync::Mutex<Vec<String>>,
    running_tasks: DashMap<String, ()>,
}

impl TriggeredAgentExecutionManager {
    pub fn new(executor: AgentExecutorCallback) -> Self {
        Self {
            executor,
            pending_task_ids: std::sync::Mutex::new(Vec::new()),
            running_tasks: DashMap::new(),
        }
    }

    /// Submit a triggered child agent execution.
    ///
    /// - `wait_for_completion == true`: returns the child output.
    /// - otherwise: spawns the child in the background and returns a task
    ///   submission descriptor.
    pub async fn submit_triggered_execution(
        &self,
        config: TriggeredAgentExecutionConfig,
        child_config: AgentLoopConfig,
        child_input: AgentLoopInput,
    ) -> AgentResult<TriggeredTaskSubmission> {
        let task_id = wf_common::generate_id();
        let parent = config.parent.clone();

        // Register the child on the parent entity.
        let child_entity = AgentLoopEntity::new(child_config.agent_id.clone())
            .with_parent_execution_id(parent.id().clone());
        parent.register_child(child_entity.id().clone()).await;

        let submission = TriggeredTaskSubmission {
            task_id: task_id.clone(),
            status: "QUEUED".to_string(),
            submit_time: wf_common::now(),
        };

        if config.wait_for_completion {
            let result = self
                .execute_child(
                    parent,
                    child_entity,
                    child_config,
                    child_input,
                    config.result_variable,
                    config.timeout_ms,
                )
                .await;
            match result {
                Ok(_) => Ok(submission),
                Err(e) => Err(e),
            }
        } else {
            let task_id_clone = task_id.clone();
            self.pending_task_ids.lock().unwrap().push(task_id.clone());
            self.running_tasks.insert(task_id.clone(), ());

            let parent_clone = parent.clone();
            let executor = self.executor.clone();
            let mut pending = self.pending_task_ids.lock().unwrap();
            pending.retain(|id| id != &task_id_clone);
            drop(pending);
            self.running_tasks.remove(&task_id_clone);

            tokio::spawn(async move {
                let _ = executor(child_config, child_input).await;
                parent_clone.unregister_child(child_entity.id()).await;
            });

            Ok(submission)
        }
    }

    /// Run a child agent to completion, write its result into the parent
    /// variable snapshot and unregister it. A child failure is reported back
    /// but does not touch the parent's execution state.
    async fn execute_child(
        &self,
        parent: Arc<AgentLoopEntity>,
        child_entity: AgentLoopEntity,
        child_config: AgentLoopConfig,
        child_input: AgentLoopInput,
        result_variable: String,
        timeout_ms: Option<u64>,
    ) -> AgentResult<Value> {
        let mut future = Box::pin((self.executor)(child_config, child_input));
        let output = match timeout_ms {
            Some(ms) if ms > 0 => {
                match tokio::time::timeout(std::time::Duration::from_millis(ms), &mut future).await
                {
                    Ok(result) => result,
                    Err(_) => {
                        parent.unregister_child(child_entity.id()).await;
                        return Err(AgentError::ExecutionError(format!(
                            "Triggered agent execution '{}' timed out after {}ms",
                            child_entity.id(),
                            ms
                        )));
                    }
                }
            }
            _ => future.await,
        };

        let output = match output {
            Ok(out) => out,
            Err(e) => {
                parent.unregister_child(child_entity.id()).await;
                return Err(e);
            }
        };

        parent
            .state
            .write()
            .await
            .set_variable_snapshot(result_variable, output.result.clone());
        parent.unregister_child(child_entity.id()).await;
        Ok(output.result)
    }

    pub fn pending_count(&self) -> usize {
        self.pending_task_ids.lock().unwrap().len()
    }

    pub fn running_count(&self) -> usize {
        self.running_tasks.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use wf_types::Id;

    fn make_parent() -> Arc<AgentLoopEntity> {
        Arc::new(AgentLoopEntity::new(Id::from("parent-1".to_string())))
    }

    fn child_config(id: &str) -> AgentLoopConfig {
        AgentLoopConfig {
            agent_id: Id::from(id.to_string()),
            model: "mock".to_string(),
            max_iterations: Some(5),
            max_execution_time: None,
            hooks: Vec::new(),
            available_tool_names: Vec::new(),
            tool_call_format: None,
            token_limit: None,
            token_warning_threshold: None,
            enable_token_tracking: None,
        }
    }

    fn success_executor(result: Value) -> AgentExecutorCallback {
        Arc::new(move |_config, _input| {
            let result = result.clone();
            Box::pin(async move {
                Ok(AgentLoopOutput {
                    result,
                    iterations: 1,
                    conversation: Vec::new(),
                })
            })
        })
    }

    fn failing_executor() -> AgentExecutorCallback {
        Arc::new(|_config, _input| {
            Box::pin(async move { Err(AgentError::ExecutionError("child boom".to_string())) })
        })
    }

    #[tokio::test]
    async fn test_sync_triggered_execution_writes_result() {
        let parent = make_parent();
        let manager =
            TriggeredAgentExecutionManager::new(success_executor(Value::from("child ok")));

        let submission = manager
            .submit_triggered_execution(
                TriggeredAgentExecutionConfig {
                    parent: parent.clone(),
                    result_variable: "trigger_result".to_string(),
                    wait_for_completion: true,
                    timeout_ms: Some(5000),
                },
                child_config("child-1"),
                AgentLoopInput {
                    message: "run".to_string(),
                    context: std::collections::HashMap::new(),
                    conversation: Vec::new(),
                },
            )
            .await
            .expect("sync child must succeed");

        assert_eq!(submission.status, "QUEUED");

        let state = parent.state.read().await;
        let snapshots = state.variable_snapshots();
        assert_eq!(
            snapshots.get("trigger_result"),
            Some(&Value::from("child ok"))
        );
        drop(state);
        assert_eq!(parent.child_execution_ids().read().await.len(), 0);
    }

    #[tokio::test]
    async fn test_child_failure_does_not_fail_parent() {
        let parent = make_parent();
        let manager = TriggeredAgentExecutionManager::new(failing_executor());

        let result = manager
            .submit_triggered_execution(
                TriggeredAgentExecutionConfig {
                    parent: parent.clone(),
                    result_variable: "trigger_result".to_string(),
                    wait_for_completion: true,
                    timeout_ms: Some(5000),
                },
                child_config("child-2"),
                AgentLoopInput {
                    message: "run".to_string(),
                    context: std::collections::HashMap::new(),
                    conversation: Vec::new(),
                },
            )
            .await;

        assert!(result.is_err());
        // Parent state untouched, child unregistered.
        assert!(!parent.state.read().await.is_failed());
        assert_eq!(parent.child_execution_ids().read().await.len(), 0);
    }

    #[tokio::test]
    async fn test_async_triggered_execution_submits_immediately() {
        let parent = make_parent();
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();
        let executor: AgentExecutorCallback = Arc::new(move |_config, _input| {
            let counter = counter_clone.clone();
            Box::pin(async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(AgentLoopOutput {
                    result: Value::Null,
                    iterations: 1,
                    conversation: Vec::new(),
                })
            })
        });
        let manager = TriggeredAgentExecutionManager::new(executor);

        let submission = manager
            .submit_triggered_execution(
                TriggeredAgentExecutionConfig {
                    parent: parent.clone(),
                    result_variable: "trigger_result".to_string(),
                    wait_for_completion: false,
                    timeout_ms: None,
                },
                child_config("child-3"),
                AgentLoopInput {
                    message: "run".to_string(),
                    context: std::collections::HashMap::new(),
                    conversation: Vec::new(),
                },
            )
            .await
            .expect("async submission must succeed");

        assert_eq!(submission.status, "QUEUED");
        assert!(!submission.task_id.is_empty());

        // Wait for the background task to finish.
        for _ in 0..50 {
            if counter.load(Ordering::SeqCst) > 0
                && parent.child_execution_ids().read().await.is_empty()
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert_eq!(counter.load(Ordering::SeqCst), 1);
        assert_eq!(parent.child_execution_ids().read().await.len(), 0);
    }

    #[tokio::test]
    async fn test_child_timeout() {
        let parent = make_parent();
        let executor: AgentExecutorCallback = Arc::new(|_config, _input| {
            Box::pin(async move {
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                Ok(AgentLoopOutput {
                    result: Value::Null,
                    iterations: 1,
                    conversation: Vec::new(),
                })
            })
        });
        let manager = TriggeredAgentExecutionManager::new(executor);

        let result = manager
            .submit_triggered_execution(
                TriggeredAgentExecutionConfig {
                    parent: parent.clone(),
                    result_variable: "trigger_result".to_string(),
                    wait_for_completion: true,
                    timeout_ms: Some(30),
                },
                child_config("child-4"),
                AgentLoopInput {
                    message: "run".to_string(),
                    context: std::collections::HashMap::new(),
                    conversation: Vec::new(),
                },
            )
            .await;

        assert!(result.is_err());
        assert_eq!(parent.child_execution_ids().read().await.len(), 0);
    }
}
