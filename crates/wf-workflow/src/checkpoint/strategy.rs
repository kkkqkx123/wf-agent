use wf_types::checkpoint::CheckpointTrigger;

#[derive(Debug, Clone, PartialEq)]
pub enum CheckpointTiming {
    BeforeNode,
    AfterNode,
    OnNodeError,
    OnWorkflowStart,
    OnWorkflowEnd,
}

#[derive(Debug, Clone)]
pub struct NodeCheckpointStrategy {
    pub timings: Vec<CheckpointTiming>,
    pub after_n_nodes: u32,
}

impl Default for NodeCheckpointStrategy {
    fn default() -> Self {
        Self {
            timings: vec![CheckpointTiming::AfterNode],
            after_n_nodes: 1,
        }
    }
}

impl NodeCheckpointStrategy {
    pub fn never() -> Self {
        Self {
            timings: vec![],
            after_n_nodes: u32::MAX,
        }
    }

    pub fn always() -> Self {
        Self {
            timings: vec![
                CheckpointTiming::BeforeNode,
                CheckpointTiming::AfterNode,
                CheckpointTiming::OnNodeError,
                CheckpointTiming::OnWorkflowStart,
                CheckpointTiming::OnWorkflowEnd,
            ],
            after_n_nodes: 1,
        }
    }

    pub fn on_error() -> Self {
        Self {
            timings: vec![CheckpointTiming::OnNodeError],
            after_n_nodes: 0,
        }
    }

    pub fn every_n_nodes(n: u32) -> Self {
        Self {
            timings: vec![CheckpointTiming::AfterNode],
            after_n_nodes: n,
        }
    }

    pub fn should_checkpoint(&self, timing: &CheckpointTiming, node_count: u32) -> bool {
        if self.timings.is_empty() {
            return false;
        }
        if !self.timings.contains(timing) {
            return false;
        }
        if *timing == CheckpointTiming::AfterNode && self.after_n_nodes > 1 {
            return node_count % self.after_n_nodes == 0;
        }
        true
    }

    pub fn to_trigger(&self, timing: &CheckpointTiming) -> Option<CheckpointTrigger> {
        match timing {
            CheckpointTiming::BeforeNode => Some(CheckpointTrigger::BeforeExecute),
            CheckpointTiming::AfterNode => Some(CheckpointTrigger::AfterExecute),
            CheckpointTiming::OnNodeError => Some(CheckpointTrigger::OnError),
            CheckpointTiming::OnWorkflowStart => Some(CheckpointTrigger::OnComplete),
            CheckpointTiming::OnWorkflowEnd => Some(CheckpointTrigger::OnComplete),
        }
    }
}
