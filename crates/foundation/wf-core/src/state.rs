use std::fmt;

use serde::{Deserialize, Serialize};
use wf_common::now;

use crate::error::CoreError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NodeState {
    Pending,
    Running {
        started_at: i64,
    },
    Completed {
        completed_at: i64,
        output: Option<serde_json::Value>,
    },
    Failed {
        error: String,
        failed_at: i64,
    },
    Skipped {
        reason: Option<String>,
    },
    Paused {
        paused_at: i64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WorkflowState {
    Created,
    Running {
        started_at: i64,
        current_node_id: Option<String>,
    },
    Paused {
        paused_at: i64,
        reason: Option<String>,
    },
    Completed {
        completed_at: i64,
        result: Option<serde_json::Value>,
    },
    Failed {
        error: String,
        failed_at: i64,
    },
    Cancelled {
        cancelled_at: i64,
        reason: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub struct StateTransition {
    pub from: String,
    pub to: String,
    pub timestamp: i64,
}

impl fmt::Display for NodeState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NodeState::Pending => write!(f, "Pending"),
            NodeState::Running { .. } => write!(f, "Running"),
            NodeState::Completed { .. } => write!(f, "Completed"),
            NodeState::Failed { .. } => write!(f, "Failed"),
            NodeState::Skipped { .. } => write!(f, "Skipped"),
            NodeState::Paused { .. } => write!(f, "Paused"),
        }
    }
}

impl fmt::Display for WorkflowState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WorkflowState::Created => write!(f, "Created"),
            WorkflowState::Running { .. } => write!(f, "Running"),
            WorkflowState::Paused { .. } => write!(f, "Paused"),
            WorkflowState::Completed { .. } => write!(f, "Completed"),
            WorkflowState::Failed { .. } => write!(f, "Failed"),
            WorkflowState::Cancelled { .. } => write!(f, "Cancelled"),
        }
    }
}

pub struct NodeStateMachine {
    node_id: String,
    state: NodeState,
    transition_log: Vec<StateTransition>,
}

impl NodeStateMachine {
    pub fn new(node_id: impl Into<String>) -> Self {
        Self {
            node_id: node_id.into(),
            state: NodeState::Pending,
            transition_log: Vec::new(),
        }
    }

    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    pub fn transition(&mut self, new_state: NodeState) -> Result<(), CoreError> {
        if self.is_terminal() {
            return Err(CoreError::InvalidStateTransition {
                message: format!("cannot transition from terminal state: {}", self.state),
            });
        }
        if !self.is_valid_transition(&new_state) {
            return Err(CoreError::InvalidStateTransition {
                message: format!("cannot transition from {} to {}", self.state, new_state),
            });
        }
        let transition = StateTransition {
            from: self.state.to_string(),
            to: new_state.to_string(),
            timestamp: now(),
        };
        self.transition_log.push(transition);
        self.state = new_state;
        Ok(())
    }

    pub fn state(&self) -> &NodeState {
        &self.state
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self.state,
            NodeState::Completed { .. } | NodeState::Failed { .. } | NodeState::Skipped { .. }
        )
    }

    pub fn transition_history(&self) -> &[StateTransition] {
        &self.transition_log
    }

    fn is_valid_transition(&self, new_state: &NodeState) -> bool {
        matches!(
            (&self.state, new_state),
            (NodeState::Pending, NodeState::Running { .. })
                | (NodeState::Running { .. }, NodeState::Completed { .. })
                | (NodeState::Running { .. }, NodeState::Failed { .. })
                | (NodeState::Running { .. }, NodeState::Skipped { .. })
                | (NodeState::Running { .. }, NodeState::Paused { .. })
                | (NodeState::Paused { .. }, NodeState::Running { .. })
        )
    }
}

#[derive(Debug, Clone)]
pub struct ErrorRecord {
    pub error: String,
    pub timestamp: i64,
}

pub struct WorkflowStateMachine {
    execution_id: String,
    state: WorkflowState,
    error_records: Vec<ErrorRecord>,
    transition_log: Vec<StateTransition>,
}

impl WorkflowStateMachine {
    pub fn new(execution_id: impl Into<String>) -> Self {
        Self {
            execution_id: execution_id.into(),
            state: WorkflowState::Created,
            error_records: Vec::new(),
            transition_log: Vec::new(),
        }
    }

    pub fn execution_id(&self) -> &str {
        &self.execution_id
    }

    pub fn start(&mut self) -> Result<(), CoreError> {
        self.transition(WorkflowState::Running {
            started_at: now(),
            current_node_id: None,
        })
    }

    pub fn pause(&mut self, reason: Option<String>) -> Result<(), CoreError> {
        self.transition(WorkflowState::Paused {
            paused_at: now(),
            reason,
        })
    }

    pub fn resume(&mut self) -> Result<(), CoreError> {
        self.transition(WorkflowState::Running {
            started_at: now(),
            current_node_id: None,
        })
    }

    pub fn complete(&mut self, result: Option<serde_json::Value>) -> Result<(), CoreError> {
        self.transition(WorkflowState::Completed {
            completed_at: now(),
            result,
        })
    }

    pub fn fail(&mut self, error: impl Into<String>) -> Result<(), CoreError> {
        let error = error.into();
        self.record_error(error.clone());
        self.transition(WorkflowState::Failed {
            error,
            failed_at: now(),
        })
    }

    pub fn cancel(&mut self, reason: Option<String>) -> Result<(), CoreError> {
        self.transition(WorkflowState::Cancelled {
            cancelled_at: now(),
            reason,
        })
    }

    pub fn record_error(&mut self, error: impl Into<String>) {
        self.error_records.push(ErrorRecord {
            error: error.into(),
            timestamp: now(),
        });
    }

    pub fn error_chain(&self) -> &[ErrorRecord] {
        &self.error_records
    }

    pub fn state(&self) -> &WorkflowState {
        &self.state
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self.state,
            WorkflowState::Completed { .. }
                | WorkflowState::Failed { .. }
                | WorkflowState::Cancelled { .. }
        )
    }

    pub fn transition_history(&self) -> &[StateTransition] {
        &self.transition_log
    }

    pub fn set_current_node(&mut self, node_id: Option<String>) {
        if let WorkflowState::Running {
            current_node_id, ..
        } = &mut self.state
        {
            *current_node_id = node_id;
        }
    }

    fn transition(&mut self, new_state: WorkflowState) -> Result<(), CoreError> {
        if self.is_terminal() {
            return Err(CoreError::InvalidStateTransition {
                message: format!("cannot transition from terminal state: {}", self.state),
            });
        }
        if !self.is_valid_transition(&new_state) {
            return Err(CoreError::InvalidStateTransition {
                message: format!("cannot transition from {} to {}", self.state, new_state),
            });
        }
        let transition = StateTransition {
            from: self.state.to_string(),
            to: new_state.to_string(),
            timestamp: now(),
        };
        self.transition_log.push(transition);
        self.state = new_state;
        Ok(())
    }

    fn is_valid_transition(&self, new_state: &WorkflowState) -> bool {
        matches!(
            (&self.state, new_state),
            (WorkflowState::Created, WorkflowState::Running { .. })
                | (WorkflowState::Running { .. }, WorkflowState::Paused { .. })
                | (
                    WorkflowState::Running { .. },
                    WorkflowState::Completed { .. }
                )
                | (WorkflowState::Running { .. }, WorkflowState::Failed { .. })
                | (
                    WorkflowState::Running { .. },
                    WorkflowState::Cancelled { .. }
                )
                | (WorkflowState::Paused { .. }, WorkflowState::Running { .. })
                | (
                    WorkflowState::Paused { .. },
                    WorkflowState::Cancelled { .. }
                )
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_pending_to_running() {
        let mut sm = NodeStateMachine::new("node-1");
        assert_eq!(*sm.state(), NodeState::Pending);
        sm.transition(NodeState::Running { started_at: 0 }).unwrap();
        assert!(matches!(*sm.state(), NodeState::Running { .. }));
    }

    #[test]
    fn test_node_running_to_completed() {
        let mut sm = NodeStateMachine::new("node-1");
        sm.transition(NodeState::Running { started_at: 0 }).unwrap();
        sm.transition(NodeState::Completed {
            completed_at: 100,
            output: None,
        })
        .unwrap();
        assert!(sm.is_terminal());
    }

    #[test]
    fn test_node_running_to_failed() {
        let mut sm = NodeStateMachine::new("node-1");
        sm.transition(NodeState::Running { started_at: 0 }).unwrap();
        sm.transition(NodeState::Failed {
            error: "oops".to_string(),
            failed_at: 50,
        })
        .unwrap();
        assert!(sm.is_terminal());
    }

    #[test]
    fn test_node_running_to_paused_to_running() {
        let mut sm = NodeStateMachine::new("node-1");
        sm.transition(NodeState::Running { started_at: 0 }).unwrap();
        sm.transition(NodeState::Paused { paused_at: 50 }).unwrap();
        sm.transition(NodeState::Running { started_at: 100 })
            .unwrap();
        assert!(matches!(*sm.state(), NodeState::Running { .. }));
    }

    #[test]
    fn test_node_invalid_transition() {
        let mut sm = NodeStateMachine::new("node-1");
        let result = sm.transition(NodeState::Completed {
            completed_at: 0,
            output: None,
        });
        assert!(matches!(
            result,
            Err(CoreError::InvalidStateTransition { .. })
        ));
    }

    #[test]
    fn test_node_transition_from_terminal() {
        let mut sm = NodeStateMachine::new("node-1");
        sm.transition(NodeState::Running { started_at: 0 }).unwrap();
        sm.transition(NodeState::Completed {
            completed_at: 100,
            output: None,
        })
        .unwrap();
        let result = sm.transition(NodeState::Running { started_at: 200 });
        assert!(matches!(
            result,
            Err(CoreError::InvalidStateTransition { .. })
        ));
    }

    #[test]
    fn test_node_transition_history() {
        let mut sm = NodeStateMachine::new("node-1");
        sm.transition(NodeState::Running { started_at: 10 })
            .unwrap();
        sm.transition(NodeState::Completed {
            completed_at: 100,
            output: None,
        })
        .unwrap();
        let history = sm.transition_history();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].from, "Pending");
        assert_eq!(history[0].to, "Running");
        assert_eq!(history[1].from, "Running");
        assert_eq!(history[1].to, "Completed");
    }

    #[test]
    fn test_workflow_full_lifecycle() {
        let mut sm = WorkflowStateMachine::new("exec-1");
        assert_eq!(*sm.state(), WorkflowState::Created);

        sm.start().unwrap();
        assert!(matches!(*sm.state(), WorkflowState::Running { .. }));

        sm.pause(Some("user pause".to_string())).unwrap();
        assert!(matches!(*sm.state(), WorkflowState::Paused { .. }));

        sm.resume().unwrap();
        assert!(matches!(*sm.state(), WorkflowState::Running { .. }));

        sm.complete(Some(serde_json::json!({"ok": true}))).unwrap();
        assert!(sm.is_terminal());
    }

    #[test]
    fn test_workflow_fail() {
        let mut sm = WorkflowStateMachine::new("exec-1");
        sm.start().unwrap();
        sm.fail("something broke").unwrap();
        assert!(sm.is_terminal());
        assert_eq!(sm.error_chain().len(), 1);
    }

    #[test]
    fn test_workflow_cancel_from_paused() {
        let mut sm = WorkflowStateMachine::new("exec-1");
        sm.start().unwrap();
        sm.pause(None).unwrap();
        sm.cancel(Some("user cancelled".to_string())).unwrap();
        assert!(sm.is_terminal());
    }

    #[test]
    fn test_workflow_invalid_transition() {
        let mut sm = WorkflowStateMachine::new("exec-1");
        let result = sm.complete(None);
        assert!(matches!(
            result,
            Err(CoreError::InvalidStateTransition { .. })
        ));
    }

    #[test]
    fn test_workflow_transition_from_terminal() {
        let mut sm = WorkflowStateMachine::new("exec-1");
        sm.start().unwrap();
        sm.complete(None).unwrap();
        let result = sm.pause(None);
        assert!(matches!(
            result,
            Err(CoreError::InvalidStateTransition { .. })
        ));
    }

    #[test]
    fn test_workflow_set_current_node() {
        let mut sm = WorkflowStateMachine::new("exec-1");
        sm.start().unwrap();
        sm.set_current_node(Some("node-42".to_string()));
        if let WorkflowState::Running {
            current_node_id, ..
        } = sm.state()
        {
            assert_eq!(current_node_id, &Some("node-42".to_string()));
        } else {
            panic!("expected Running state");
        }
    }

    #[test]
    fn test_workflow_error_chain() {
        let mut sm = WorkflowStateMachine::new("exec-1");
        sm.start().unwrap();
        sm.record_error("first error");
        sm.record_error("second error");
        assert_eq!(sm.error_chain().len(), 2);
        assert_eq!(sm.error_chain()[0].error, "first error");
        assert_eq!(sm.error_chain()[1].error, "second error");
    }
}
