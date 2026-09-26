#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BranchResult {
    pub branch_id: String,
    pub success: bool,
    pub output: serde_json::Value,
    pub error: Option<String>,
    /// Routing category of the terminal failure that ended the branch, kept
    /// typed across the child-coordinator boundary so business consumers can
    /// distinguish cancellation and timeout from plain business failures.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_category: Option<wf_types::workflow::error_branch::NodeErrorCategory>,
    /// Snapshot of the branch's public variables (non-internal) taken when
    /// the branch settled. `None` for branches that expose no variables or
    /// failed before the snapshot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variables: Option<std::collections::HashMap<String, serde_json::Value>>,
}

impl BranchResult {
    pub fn success(branch_id: impl Into<String>, output: serde_json::Value) -> Self {
        Self {
            branch_id: branch_id.into(),
            success: true,
            output,
            error: None,
            error_category: None,
            variables: None,
        }
    }

    pub fn success_with_variables(
        branch_id: impl Into<String>,
        output: serde_json::Value,
        variables: std::collections::HashMap<String, serde_json::Value>,
    ) -> Self {
        Self {
            branch_id: branch_id.into(),
            success: true,
            output,
            error: None,
            error_category: None,
            variables: Some(variables),
        }
    }

    pub fn failure(branch_id: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            branch_id: branch_id.into(),
            success: false,
            output: serde_json::Value::Null,
            error: Some(error.into()),
            error_category: None,
            variables: None,
        }
    }

    pub fn with_category(
        mut self,
        category: wf_types::workflow::error_branch::NodeErrorCategory,
    ) -> Self {
        self.error_category = Some(category);
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum FailureStrategy {
    FailFast,
    ContinueOnError,
    FailOnThreshold { threshold: f64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForkOutcome {
    Succeeded,
    Failed,
    Partial,
}

impl FailureStrategy {
    pub fn evaluate(&self, results: &[BranchResult]) -> ForkOutcome {
        let total = results.len();
        let failures = results.iter().filter(|r| !r.success).count();

        match self {
            FailureStrategy::FailFast => {
                if failures > 0 {
                    ForkOutcome::Failed
                } else {
                    ForkOutcome::Succeeded
                }
            }
            FailureStrategy::ContinueOnError => ForkOutcome::Succeeded,
            FailureStrategy::FailOnThreshold { threshold } => {
                if total == 0 {
                    return ForkOutcome::Succeeded;
                }
                let failure_rate = failures as f64 / total as f64;
                if failure_rate > *threshold {
                    ForkOutcome::Failed
                } else if failures > 0 {
                    ForkOutcome::Partial
                } else {
                    ForkOutcome::Succeeded
                }
            }
        }
    }
}
